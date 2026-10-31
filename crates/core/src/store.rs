//! On-disk persistence for client state.
//!
//! The MLS store lives in SQLite, through openmls's own `StorageProvider`
//! for it (`openmls_sqlite_storage`), so a change writes the rows it touches
//! rather than the whole store. This replaced a snapshot that rewrote every
//! group's state on every send and receive, a cost that grew with group
//! size and would have grown again with each channel a client joins (#107).
//!
//! # Encryption at rest
//!
//! The database holds private key material, so it is never written in the
//! clear. It is SQLCipher: every page is encrypted and authenticated, so the
//! file has no readable header, no readable rows, and a flipped byte fails
//! the page's MAC rather than being read. The 256-bit data key lives in the
//! macOS Keychain, not beside the file, and is handed to SQLCipher raw, so
//! there is no passphrase stretching on open.
//!
//! Encryption could not live in `openmls_sqlite_storage`'s codec instead: the
//! codec is stateless, so it cannot carry a per-client key, and table keys
//! such as group ids would have stayed in plaintext.
//!
//! # Atomicity
//!
//! openmls writes several rows per operation. A crash or an error between
//! two of them must not leave a half-applied epoch on disk, so every
//! operation runs inside a savepoint (see [`Database::atomically`]) and is
//! rolled back as a whole if any part of it fails. Savepoints rather than
//! transactions because operations nest: receiving a message can resend one.
//!
//! # Durability
//!
//! The file uses WAL with `synchronous = FULL`, so an operation that has
//! returned survives the app or the OS crashing. That is not enough for
//! sending. On macOS, `fsync` hands data to the drive without making it flush
//! its cache, so a power cut can lose the last writes, and a client that comes
//! back with an older sender ratchet would encrypt again under key
//! generations it has already used.
//!
//! So operations that produce something other members will see (a message,
//! a commit, a new group, a `KeyPackage`) ask for a true flush
//! (`F_FULLFSYNC`) when they commit; see [`Database::flush_on_commit`]. It
//! costs about 4 ms where an ordinary commit costs about 0.1 ms, which is fine
//! once per send and too much once per received frame. Receives don't need
//! it: one lost to a power cut is fetched and applied again. A flushed commit
//! also makes durable everything committed before it, receives included,
//! because the WAL is flushed as a whole.

use crate::{Error, Result};
use openmls_rust_crypto::RustCrypto;
use openmls_sqlite_storage::{Codec, SqliteStorageProvider};
use openmls_traits::OpenMlsProvider;
use rusqlite::{Connection, OptionalExtension, params};
use std::cell::Cell;
use std::path::{Path, PathBuf};

const KEY_LEN: usize = 32;

/// Bumped whenever this module's own tables change. openmls's tables are
/// versioned separately, by the storage crate's migrations.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    -- Who this client is. One row: a database belongs to one identity.
    CREATE TABLE client (
        id                   INTEGER PRIMARY KEY CHECK (id = 0),
        identity             TEXT NOT NULL,
        signature_public_key BLOB NOT NULL,
        -- Set once the client has created or joined a group.
        group_id             BLOB
    );
";

/// How values are serialised into openmls's tables. JSON, as openmls's own
/// in-memory store uses: the values are small, and the database encrypts
/// them whatever their format.
#[derive(Debug, Default)]
pub struct JsonCodec;

impl Codec for JsonCodec {
    type Error = serde_json::Error;

    fn to_vec<T: serde::Serialize>(value: &T) -> std::result::Result<Vec<u8>, Self::Error> {
        serde_json::to_vec(value)
    }

    fn from_slice<T: serde::de::DeserializeOwned>(
        slice: &[u8],
    ) -> std::result::Result<T, Self::Error> {
        serde_json::from_slice(slice)
    }
}

/// openmls's view of a client: its crypto backend and its database.
///
/// Borrowed rather than owned, and built per operation. `Member` owns the
/// database and needs it directly, for its own table and for savepoints, and
/// the storage provider offers no way back to a connection it owns.
pub struct Provider<'a> {
    crypto: &'a RustCrypto,
    storage: SqliteStorageProvider<JsonCodec, &'a Connection>,
}

/// Opaque on purpose: the storage behind it holds private key material.
impl std::fmt::Debug for Provider<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Provider").finish_non_exhaustive()
    }
}

impl<'a> OpenMlsProvider for Provider<'a> {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = SqliteStorageProvider<JsonCodec, &'a Connection>;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }
    fn crypto(&self) -> &Self::CryptoProvider {
        self.crypto
    }
    fn rand(&self) -> &Self::RandProvider {
        self.crypto
    }
}

/// What a client records about itself, beside openmls's own state.
pub(crate) struct Saved {
    /// Display name carried in the MLS credential.
    pub(crate) identity: String,
    /// Identifies which key pair in the store belongs to this client.
    pub(crate) signature_public_key: Vec<u8>,
    /// Present once the client has created or joined a group.
    pub(crate) group_id: Option<Vec<u8>>,
}

/// One client's state: an open SQLite connection with the schema in place.
pub(crate) struct Database {
    conn: Connection,
    /// The database file, or `None` for an in-memory database, which keeps
    /// nothing across a restart.
    path: Option<PathBuf>,
    /// Savepoints currently open. Only the outermost release commits.
    depth: Cell<u32>,
    /// Whether the open transaction must be flushed to the drive when it
    /// commits. Set by any operation inside it, and cleared when it ends.
    flush: Cell<bool>,
    /// Commits that were flushed to the drive, so tests can see which
    /// operations asked for it.
    #[cfg(test)]
    pub(crate) flushed: Cell<u32>,
}

/// Never prints the connection: the tables behind it hold key material.
impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("persistent", &self.path.is_some())
            .finish_non_exhaustive()
    }
}

fn store<E: std::fmt::Display>(e: E) -> Error {
    Error::Store(e.to_string())
}

impl Database {
    /// A database that lives only as long as this value.
    pub(crate) fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(store)?, None)
    }

    /// Open, or create, the encrypted database at `path` with `key`.
    fn open(path: &Path, key: &[u8; KEY_LEN]) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(store)?;
        }
        let conn = Connection::open(path).map_err(store)?;
        // Must come before anything else touches the file. A raw key in
        // SQLCipher's `x'…'` form skips its passphrase derivation.
        conn.pragma_update(None, "key", format!("x'{}'", hex(key)))
            .map_err(store)?;

        // A wrong key fails the first read: SQLCipher cannot tell a wrong key
        // from a file that is not a database. Surface it rather than starting
        // fresh, which would look like data loss.
        if conn
            .query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
            .is_err()
        {
            return Err(Error::Store(
                "could not decrypt store (wrong key or tampered)".into(),
            ));
        }
        // Checks every page's MAC, so tampering anywhere is refused at open
        // rather than discovered later in whichever page happens to be read.
        // The store is small and opened once per launch, so reading it all is
        // cheap.
        let damaged = conn
            .prepare("PRAGMA cipher_integrity_check")
            .and_then(|mut stmt| stmt.query([])?.next().map(|row| row.is_some()))
            .map_err(store)?;
        if damaged {
            return Err(Error::Store(
                "store failed its integrity check (tampered or corrupt)".into(),
            ));
        }

        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
            .map_err(store)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(store)?;
        // A checkpoint copies the WAL into the database and then lets the WAL
        // be overwritten. On macOS a plain `fsync` can leave the copied pages
        // in the drive's cache, so a power cut after the WAL moves on would
        // lose them. Checkpoints are rare (see below), so flushing them is
        // cheap.
        conn.pragma_update(None, "checkpoint_fullfsync", true)
            .map_err(store)?;
        // By default the WAL grows to about 1000 pages (4 MiB) before it is
        // checkpointed into the database, and the file is never shrunk
        // afterwards, so every client would carry a 4 MiB WAL against a store
        // of a few hundred KiB. Checkpoint at 128 pages and truncate what is
        // left beyond 512 KiB.
        conn.pragma_update(None, "wal_autocheckpoint", 128)
            .map_err(store)?;
        conn.query_row("PRAGMA journal_size_limit = 524288", [], |_| Ok(()))
            .map_err(store)?;
        // SQLite gives the WAL and shared-memory files the same permissions as
        // the database, so restricting this one covers them.
        restrict(path)?;
        Self::init(conn, Some(path.to_owned()))
    }

    fn init(mut conn: Connection, path: Option<PathBuf>) -> Result<Self> {
        SqliteStorageProvider::<JsonCodec, &mut Connection>::new(&mut conn)
            .run_migrations()
            .map_err(store)?;

        let tx = conn.transaction().map_err(store)?;
        let version: i64 = tx
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(store)?;
        match version {
            0 => {
                tx.execute_batch(SCHEMA).map_err(store)?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)
                    .map_err(store)?;
            }
            SCHEMA_VERSION => {}
            _ => {
                return Err(Error::CorruptState(
                    "store was written by a newer version of OpenConv",
                ));
            }
        }
        tx.commit().map_err(store)?;
        Ok(Self {
            conn,
            path,
            depth: Cell::new(0),
            flush: Cell::new(false),
            #[cfg(test)]
            flushed: Cell::new(0),
        })
    }

    /// Whether this database outlives the process.
    pub(crate) fn is_persistent(&self) -> bool {
        self.path.is_some()
    }

    /// openmls's provider over this database.
    pub(crate) fn provider<'a>(&'a self, crypto: &'a RustCrypto) -> Provider<'a> {
        Provider {
            crypto,
            storage: SqliteStorageProvider::new(&self.conn),
        }
    }

    /// Run `op` so that its writes land together or not at all.
    ///
    /// On error everything `op` wrote is rolled back and the error returned.
    /// Nests: an `op` that calls `atomically` again gets an inner savepoint,
    /// and only the outermost release makes anything durable.
    pub(crate) fn atomically<T>(&self, op: impl FnOnce() -> Result<T>) -> Result<T> {
        self.begin()?;
        let result = op().and_then(|value| self.release().map(|()| value));
        if result.is_err() {
            self.rollback();
        }
        result
    }

    /// Open a savepoint. Pair with [`Database::release`] on success and
    /// [`Database::rollback`] on failure; [`Database::atomically`] does both,
    /// for callers that do not need to touch anything else in between.
    pub(crate) fn begin(&self) -> Result<()> {
        self.conn.execute_batch("SAVEPOINT op").map_err(store)?;
        self.depth.set(self.depth.get() + 1);
        Ok(())
    }

    /// Keep everything since the matching [`Database::begin`]. The outermost
    /// release is the commit, and is flushed to the drive if anything inside
    /// it asked for that.
    pub(crate) fn release(&self) -> Result<()> {
        self.conn.execute_batch("RELEASE op").map_err(store)?;
        self.depth.set(self.depth.get() - 1);
        if self.depth.get() == 0 && self.flush.replace(false) {
            self.flush_to_drive()?;
        }
        Ok(())
    }

    /// Make the drive write out what SQLite has handed it.
    ///
    /// Done after the commit rather than by switching on `PRAGMA fullfsync`
    /// for it: set inside an open transaction, that pragma measurably did not
    /// reach the commit's sync. `sync_all` on macOS is `F_FULLFSYNC`, which
    /// flushes the drive's whole cache, so syncing the WAL covers the commit
    /// SQLite has just written to it and everything committed before.
    fn flush_to_drive(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut wal = path.clone().into_os_string();
        wal.push("-wal");
        std::fs::File::open(wal)
            .and_then(|wal| wal.sync_all())
            .map_err(store)?;
        // Counted only once the flush has happened, after a real commit.
        #[cfg(test)]
        self.flushed.set(self.flushed.get() + 1);
        Ok(())
    }

    /// Ask for the current transaction to be flushed to the drive when it
    /// commits, rather than only handed to it. For operations whose result
    /// other members will see; see the module docs for why.
    pub(crate) fn flush_on_commit(&self) {
        self.flush.set(true);
    }

    /// Undo everything since the matching [`Database::begin`].
    ///
    /// Best effort, so it returns nothing: if a failed release has already
    /// made SQLite roll back, the savepoint is gone and this fails too. The
    /// error worth reporting is whichever one caused the rollback.
    pub(crate) fn rollback(&self) {
        let _ = self.conn.execute_batch("ROLLBACK TO op; RELEASE op");
        self.depth.set(self.depth.get().saturating_sub(1));
        if self.depth.get() == 0 {
            self.flush.set(false);
        }
    }

    /// What this client recorded about itself, or `None` for a new database.
    pub(crate) fn load(&self) -> Result<Option<Saved>> {
        self.conn
            .query_row(
                "SELECT identity, signature_public_key, group_id FROM client WHERE id = 0",
                [],
                |r| {
                    Ok(Saved {
                        identity: r.get(0)?,
                        signature_public_key: r.get(1)?,
                        group_id: r.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(store)
    }

    /// Record who this client is and which group it is in.
    pub(crate) fn save(&self, saved: &Saved) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO client (id, identity, signature_public_key, group_id)
                 VALUES (0, ?1, ?2, ?3)
                 ON CONFLICT (id) DO UPDATE SET
                     identity = excluded.identity,
                     signature_public_key = excluded.signature_public_key,
                     group_id = excluded.group_id",
                params![saved.identity, saved.signature_public_key, saved.group_id],
            )
            .map(drop)
            .map_err(store)
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Where one client's encrypted state lives, and the key that opens it.
pub struct Vault {
    path: PathBuf,
    key: [u8; KEY_LEN],
}

/// Prints the path but never the key.
impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// The vault at `path`, its data key fetched from (or created in) the
    /// Keychain under `account`.
    ///
    /// # Errors
    /// If the data key cannot be read or created.
    pub fn open(path: impl Into<PathBuf>, account: &str) -> Result<Self> {
        Ok(Self {
            path: path.into(),
            key: data_key(account)?,
        })
    }

    /// A vault with an explicit data key, bypassing the Keychain.
    ///
    /// Used by tests, which must not touch the real Keychain, and the hook a
    /// passphrase-derived key would use.
    #[must_use]
    pub fn with_key(path: impl Into<PathBuf>, key: [u8; KEY_LEN]) -> Self {
        Self {
            path: path.into(),
            key,
        }
    }

    /// Where this vault is stored.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the vault file has been written yet.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Open the database, creating it if it does not exist.
    ///
    /// Consumes the vault: once SQLCipher has the key, nothing here needs
    /// it, and it is better not kept in memory longer than that.
    pub(crate) fn connect(self) -> Result<Database> {
        Database::open(&self.path, &self.key)
    }
}

/// Owner-only permissions on a state file.
#[cfg(unix)]
fn restrict(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(store)
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<()> {
    Ok(())
}

/// Gated with its only user: on non-macOS builds `platform_key` is compiled
/// out, and an ungated constant would be dead code — which `-D warnings`
/// turns into a build failure.
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "com.openconv.vault";

/// Development override: keep the data key in a file beside the vault instead
/// of the Keychain.
///
/// An unsigned binary gets a new code identity on every rebuild, and the
/// Keychain then shows an ACL prompt when it reads an item an earlier build
/// created. Launched headlessly that prompt is invisible and the process
/// hangs forever, which is how this was found. Real installs are code-signed
/// and use the Keychain; dev and test runs set `OPENCONV_DATA_DIR` and get a
/// throwaway key file.
///
/// This is weaker on purpose: the key sits next to the data it protects, so
/// it guards against casual reads and backups, not against someone with file
/// access. It is only ever used for disposable data directories.
const DEV_DATA_DIR: &str = "OPENCONV_DATA_DIR";

/// Fetch this account's data key, from a dev key file when
/// `OPENCONV_DATA_DIR` is set and otherwise from the Keychain.
fn data_key(account: &str) -> Result<[u8; KEY_LEN]> {
    match std::env::var(DEV_DATA_DIR) {
        Ok(dir) if !dir.is_empty() => file_key(&Path::new(&dir).join(format!(".{account}.key"))),
        _ => platform_key(account),
    }
}

/// Read, or create, a 0600 key file.
fn file_key(path: &Path) -> Result<[u8; KEY_LEN]> {
    match std::fs::read(path) {
        Ok(bytes) => bytes
            .as_slice()
            .try_into()
            .map_err(|_| Error::Store("dev key file is malformed".into())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut key = [0u8; KEY_LEN];
            getrandom(&mut key)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(store)?;
            }
            std::fs::write(path, key).map_err(store)?;
            restrict(path)?;
            Ok(key)
        }
        Err(e) => Err(store(e)),
    }
}

/// Fetch this account's data key from the Keychain, creating one on first use.
#[cfg(target_os = "macos")]
fn platform_key(account: &str) -> Result<[u8; KEY_LEN]> {
    use security_framework::passwords::{get_generic_password, set_generic_password};

    if let Ok(existing) = get_generic_password(KEYCHAIN_SERVICE, account) {
        let bytes: [u8; KEY_LEN] = existing
            .as_slice()
            .try_into()
            .map_err(|_| Error::Store("keychain holds a malformed data key".into()))?;
        return Ok(bytes);
    }

    let mut key = [0u8; KEY_LEN];
    getrandom(&mut key)?;
    set_generic_password(KEYCHAIN_SERVICE, account, &key)
        .map_err(|e| Error::Store(format!("keychain: {e}")))?;
    Ok(key)
}

/// Non-macOS builds have no Keychain equivalent wired up yet. Refusing is
/// deliberate: silently falling back to an unencrypted or file-resident key
/// would weaken the guarantee without saying so.
#[cfg(not(target_os = "macos"))]
fn platform_key(_account: &str) -> Result<[u8; KEY_LEN]> {
    Err(Error::Store(
        "encrypted storage is only implemented on macOS".into(),
    ))
}

fn getrandom(buf: &mut [u8]) -> Result<()> {
    use rand::RngCore;
    rand::rngs::OsRng
        .try_fill_bytes(buf)
        .map_err(|e| Error::Store(format!("rng: {e}")))
}
