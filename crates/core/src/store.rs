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
const SCHEMA_VERSION: i64 = 8;

/// Each version's changes, applied in order from the store's own version.
/// Never edit one that has shipped; add the next.
const MIGRATIONS: [&str; 8] = [
    SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7, SCHEMA_V8,
];

const SCHEMA_V8: &str = "
    -- Several groups per client (#134). The groups this client is in, in
    -- the order it joined them, instead of one group id on the client row.
    CREATE TABLE groups (channel BLOB PRIMARY KEY);
    INSERT INTO groups (channel) SELECT group_id FROM client WHERE group_id IS NOT NULL;

    -- What was this client's state about its one group becomes state about
    -- a group, keyed by its channel. Rows that belonged to a group this
    -- client is no longer in cannot be placed, and go: a removed member's
    -- record of who it saw removed, for instance.
    CREATE TABLE pending_commit_v8 (
        channel BLOB PRIMARY KEY,
        commit_ BLOB NOT NULL,
        welcome BLOB,
        mailbox BLOB,
        until   INTEGER
    );
    INSERT INTO pending_commit_v8
        SELECT c.group_id, p.commit_, p.welcome, p.mailbox, p.until
        FROM pending_commit p, client c WHERE c.group_id IS NOT NULL;
    DROP TABLE pending_commit;
    ALTER TABLE pending_commit_v8 RENAME TO pending_commit;

    CREATE TABLE leaving_v8 (
        channel  BLOB PRIMARY KEY,
        proposal BLOB NOT NULL,
        epoch    INTEGER NOT NULL
    );
    INSERT INTO leaving_v8
        SELECT c.group_id, l.proposal, l.epoch
        FROM leaving l, client c WHERE c.group_id IS NOT NULL;
    DROP TABLE leaving;
    ALTER TABLE leaving_v8 RENAME TO leaving;

    ALTER TABLE sent ADD COLUMN channel BLOB;
    UPDATE sent SET channel = (SELECT group_id FROM client);
    DELETE FROM sent WHERE channel IS NULL;

    ALTER TABLE deferred ADD COLUMN channel BLOB;
    UPDATE deferred SET channel = (SELECT group_id FROM client);
    DELETE FROM deferred WHERE channel IS NULL;

    CREATE TABLE join_requests_v8 (
        channel       BLOB NOT NULL,
        signature_key BLOB NOT NULL,
        key_package   BLOB NOT NULL,
        name          TEXT NOT NULL,
        seen_at       INTEGER NOT NULL,
        declined      INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (channel, signature_key)
    );
    INSERT INTO join_requests_v8
        SELECT c.group_id, j.signature_key, j.key_package, j.name, j.seen_at, j.declined
        FROM join_requests j, client c WHERE c.group_id IS NOT NULL;
    DROP TABLE join_requests;
    ALTER TABLE join_requests_v8 RENAME TO join_requests;

    CREATE TABLE removed_v8 (
        channel       BLOB NOT NULL,
        signature_key BLOB NOT NULL,
        name          TEXT NOT NULL,
        by            TEXT NOT NULL,
        at            INTEGER NOT NULL,
        PRIMARY KEY (channel, signature_key)
    );
    INSERT INTO removed_v8
        SELECT c.group_id, r.signature_key, r.name, r.by, r.at
        FROM removed r, client c WHERE c.group_id IS NOT NULL;
    DROP TABLE removed;
    ALTER TABLE removed_v8 RENAME TO removed;

    CREATE TABLE approved_v8 (
        channel       BLOB NOT NULL,
        signature_key BLOB NOT NULL,
        name          TEXT NOT NULL,
        at            INTEGER NOT NULL,
        PRIMARY KEY (channel, signature_key)
    );
    INSERT INTO approved_v8
        SELECT c.group_id, a.signature_key, a.name, a.at
        FROM approved a, client c WHERE c.group_id IS NOT NULL;
    DROP TABLE approved;
    ALTER TABLE approved_v8 RENAME TO approved;

    -- A KeyPackage can be published to any channel, so whether it was
    -- admitted is a fact about a group, not about the KeyPackage.
    CREATE TABLE admitted_v8 (
        channel         BLOB NOT NULL,
        key_package_ref BLOB NOT NULL,
        until           INTEGER NOT NULL,
        PRIMARY KEY (channel, key_package_ref)
    );
    INSERT INTO admitted_v8
        SELECT c.group_id, a.key_package_ref, a.until
        FROM admitted a, client c WHERE c.group_id IS NOT NULL;
    DROP TABLE admitted;
    ALTER TABLE admitted_v8 RENAME TO admitted;

    ALTER TABLE client DROP COLUMN group_id;
";

const SCHEMA_V7: &str = "
    -- The conversation as this client saw it (#129): every message it
    -- decrypted and every one it sent, per channel, by MIMI message id.
    -- Written in the same transaction that decrypts or encrypts the
    -- message, so a crash can neither lose one nor store it twice.
    CREATE TABLE transcript (
        channel     BLOB NOT NULL,
        id          BLOB NOT NULL,
        -- The order this client applied it in, which is the relay's order.
        -- Numbered here rather than taken from the relay, whose numbering
        -- starts again if its store is reset. NULL for this client's own
        -- message until the relay's echo shows where it landed.
        position    INTEGER UNIQUE,
        -- Where the relay put it and when, if it came through the relay.
        seq         INTEGER,
        accepted_ms INTEGER,
        -- When this client stored it, by its own clock.
        stored_ms   INTEGER NOT NULL,
        mine        INTEGER NOT NULL,
        sender      TEXT NOT NULL,
        sender_uri  TEXT NOT NULL,
        -- The MIMI content as it arrived, so a later version can show what
        -- this one cannot.
        content     BLOB NOT NULL,
        -- Copied out of the content for the replies, edits and reactions
        -- that will look messages up by them (#131).
        replaces    BLOB,
        in_reply_to BLOB,
        PRIMARY KEY (channel, id)
    );
    CREATE INDEX transcript_by_position ON transcript (channel, position);
";

const SCHEMA_V6: &str = "
    -- A sent message's MIMI content (#128), salt and all, so a resend reuses
    -- it and keeps the message id. Rows from before keep only their text.
    ALTER TABLE sent ADD COLUMN content BLOB;
";

const SCHEMA_V5: &str = "
    -- Requests to join, waiting for a member to accept or decline them
    -- (#125). One per identity: a joiner's renewed requests replace its
    -- earlier ones, and a decline sticks to the identity.
    CREATE TABLE join_requests (
        signature_key BLOB PRIMARY KEY,
        key_package   BLOB NOT NULL,
        name          TEXT NOT NULL,
        seen_at       INTEGER NOT NULL,
        declined      INTEGER NOT NULL DEFAULT 0
    );

    -- Members this client saw removed, or leave, so a request to join from
    -- the same identity, or the same name, can be flagged.
    CREATE TABLE removed (
        signature_key BLOB PRIMARY KEY,
        name          TEXT NOT NULL,
        by            TEXT NOT NULL,
        at            INTEGER NOT NULL
    );

    -- Identities a member here has accepted, until they are admitted (an
    -- approval is used once) or removed. Approval is of the person, not of
    -- one request: a request can expire before a member gets to it, and the
    -- joiner's next one is then let in without asking again.
    CREATE TABLE approved (
        signature_key BLOB PRIMARY KEY,
        name          TEXT NOT NULL,
        at            INTEGER NOT NULL
    );

    -- Join requests used to be answered automatically. Any still held from
    -- then were never accepted by anyone, so they go; the joiner's next
    -- renewal brings them back as requests to approve.
    DELETE FROM deferred WHERE rejoin = 0;
";

const SCHEMA_V4: &str = "
    -- Join requests this client has seen admitted, by KeyPackageRef, until
    -- the KeyPackage expires. A request that repeats one is a replay (#123).
    CREATE TABLE admitted (
        key_package_ref BLOB PRIMARY KEY,
        until           INTEGER NOT NULL
    );
    -- For a staged add: when its KeyPackage expires, so that admission can
    -- be remembered that long once the commit is confirmed.
    ALTER TABLE pending_commit ADD COLUMN until INTEGER;
";

const SCHEMA_V3: &str = "
    -- A staged commit of any kind (#112): an add carries a Welcome and the
    -- mailbox it goes to, a removal does not.
    CREATE TABLE pending_commit (
        id      INTEGER PRIMARY KEY CHECK (id = 0),
        commit_ BLOB NOT NULL,
        welcome BLOB,
        mailbox BLOB
    );
    INSERT INTO pending_commit SELECT id, commit_, welcome, mailbox FROM pending_add;
    DROP TABLE pending_add;

    -- Whether a held request, or this client's own offer, asks to be re-added
    -- (replacing a leaf the same identity already has) rather than added.
    ALTER TABLE deferred ADD COLUMN rejoin INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE follows ADD COLUMN rejoin INTEGER NOT NULL DEFAULT 0;

    -- This client's request to leave, until a member commits it: the
    -- proposal, and the epoch it was made in. Re-made in each new epoch.
    CREATE TABLE leaving (
        id       INTEGER PRIMARY KEY CHECK (id = 0),
        proposal BLOB NOT NULL,
        epoch    INTEGER NOT NULL
    );
";

const SCHEMA_V2: &str = "
    -- Addresses this client reads from the relay, and the first sequence
    -- number on each it has not yet applied (#105). Its channel, or, while
    -- it waits to be admitted, its mailbox. A mailbox row also holds the
    -- offer that was made from it, re-sent on every reconnect until a
    -- Welcome arrives.
    CREATE TABLE follows (
        address  BLOB PRIMARY KEY,
        next_seq INTEGER NOT NULL,
        offer_to BLOB,
        offer    BLOB
    );

    -- Messages sent and not yet seen coming back, so an echo that lands
    -- behind a commit can be resent even after a restart.
    CREATE TABLE sent (
        id    INTEGER PRIMARY KEY AUTOINCREMENT,
        wire  BLOB NOT NULL,
        epoch INTEGER NOT NULL,
        text  TEXT NOT NULL
    );

    -- An add staged and not yet confirmed by the relay's ordering. Kept, so
    -- a client that restarts mid-add recognises its own commit in catch-up.
    CREATE TABLE pending_add (
        id      INTEGER PRIMARY KEY CHECK (id = 0),
        commit_ BLOB NOT NULL,
        welcome BLOB NOT NULL,
        mailbox BLOB NOT NULL
    );

    -- Join requests seen while catching up or while another add was in
    -- flight, answered once this client is live and free.
    CREATE TABLE deferred (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        key_package BLOB NOT NULL
    );
";

const SCHEMA_V1: &str = "
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
        let Some(pending) = usize::try_from(version)
            .ok()
            .and_then(|v| MIGRATIONS.get(v..))
        else {
            return Err(Error::CorruptState(
                "store was written by a newer version of OpenConv",
            ));
        };
        for migration in pending {
            tx.execute_batch(migration).map_err(store)?;
        }
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(store)?;
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

    /// Make openmls's writes of group state fail, or work again, while this
    /// module's own tables keep working: a save failure inside openmls, as
    /// a full disk partway through applying a commit would cause.
    #[cfg(test)]
    pub(crate) fn fail_group_writes(&self, fail: bool) {
        let sql = if fail {
            "CREATE TRIGGER no_insert BEFORE INSERT ON openmls_group_data
                 BEGIN SELECT RAISE(FAIL, 'disk full (simulated)'); END;
             CREATE TRIGGER no_update BEFORE UPDATE ON openmls_group_data
                 BEGIN SELECT RAISE(FAIL, 'disk full (simulated)'); END;"
        } else {
            "DROP TRIGGER no_insert; DROP TRIGGER no_update;"
        };
        self.conn
            .execute_batch(sql)
            .expect("toggle simulated failure");
    }

    /// Make writes to the transcript fail, or work again, while everything
    /// else keeps working: a message decrypted but not stored.
    #[cfg(test)]
    pub(crate) fn fail_transcript_writes(&self, fail: bool) {
        let sql = if fail {
            "CREATE TRIGGER no_log_insert BEFORE INSERT ON transcript
                 BEGIN SELECT RAISE(FAIL, 'disk full (simulated)'); END;
             CREATE TRIGGER no_log_update BEFORE UPDATE ON transcript
                 BEGIN SELECT RAISE(FAIL, 'disk full (simulated)'); END;"
        } else {
            "DROP TRIGGER no_log_insert; DROP TRIGGER no_log_update;"
        };
        self.conn
            .execute_batch(sql)
            .expect("toggle simulated failure");
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
                "SELECT identity, signature_public_key FROM client WHERE id = 0",
                [],
                |r| {
                    Ok(Saved {
                        identity: r.get(0)?,
                        signature_public_key: r.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(store)
    }

    /// Record who this client is.
    pub(crate) fn save(&self, saved: &Saved) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO client (id, identity, signature_public_key)
                 VALUES (0, ?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET
                     identity = excluded.identity,
                     signature_public_key = excluded.signature_public_key",
                params![saved.identity, saved.signature_public_key],
            )
            .map(drop)
            .map_err(store)
    }

    /// The channels of the groups this client is in, in the order it joined
    /// them.
    pub(crate) fn groups(&self) -> Result<Vec<Vec<u8>>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT channel FROM groups ORDER BY rowid")
            .map_err(store)?;
        let rows = stmt.query_map([], |r| r.get(0)).map_err(store)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store)
    }

    /// Record that this client is in the group on `channel`.
    pub(crate) fn add_group(&self, channel: &[u8]) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO groups (channel) VALUES (?1)",
                [channel],
            )
            .map(drop)
            .map_err(store)
    }

    /// Record that this client is no longer in the group on `channel`.
    pub(crate) fn remove_group(&self, channel: &[u8]) -> Result<()> {
        self.conn
            .execute("DELETE FROM groups WHERE channel = ?1", [channel])
            .map(drop)
            .map_err(store)
    }
}

/// A sent message as stored: wire bytes, epoch, MIMI content (absent for rows
/// from before #128), and the text such rows kept instead.
pub(crate) type SentRow = (Vec<u8>, u64, Option<Vec<u8>>, String);

/// A staged add's `Welcome`, the mailbox it goes to (the joiner's
/// `KeyPackageRef`), and when that `KeyPackage` expires.
pub(crate) type Admits = (Vec<u8>, Vec<u8>, u64);

/// A staged commit as stored: the commit, and for an add, what it admits.
pub(crate) type Staged = (Vec<u8>, Option<Admits>);

/// [`Staged`], borrowed.
pub(crate) type StagedRef<'a> = (&'a [u8], Option<(&'a [u8], &'a [u8], u64)>);

/// An offer to join, as made from a mailbox.
pub(crate) struct Offer {
    /// The channel it was published to.
    pub(crate) to: Vec<u8>,
    /// The `KeyPackage`.
    pub(crate) key_package: Vec<u8>,
    /// Whether it asks to be re-added rather than added.
    pub(crate) rejoin: bool,
}

/// An address this client reads from the relay. See the `follows` table.
pub(crate) struct Follow {
    pub(crate) address: Vec<u8>,
    pub(crate) next_seq: u64,
    /// For a mailbox: the offer that was made from it.
    pub(crate) offer: Option<Offer>,
}

fn to_i64(n: u64) -> Result<i64> {
    i64::try_from(n).map_err(|_| Error::CorruptState("sequence number out of range"))
}

fn to_u64(n: i64) -> rusqlite::Result<u64> {
    u64::try_from(n).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Integer, Box::new(e))
    })
}

impl Database {
    /// Every address this client reads.
    pub(crate) fn follows(&self) -> Result<Vec<Follow>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT address, next_seq, offer_to, offer, rejoin FROM follows")
            .map_err(store)?;
        let rows = stmt
            .query_map([], |r| {
                let offer = match (r.get(2)?, r.get(3)?) {
                    (Some(to), Some(key_package)) => Some(Offer {
                        to,
                        key_package,
                        rejoin: r.get(4)?,
                    }),
                    _ => None,
                };
                Ok(Follow {
                    address: r.get(0)?,
                    next_seq: to_u64(r.get(1)?)?,
                    offer,
                })
            })
            .map_err(store)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store)
    }

    /// Start reading `address` from `next_seq`, replacing any earlier entry.
    pub(crate) fn follow(
        &self,
        address: &[u8],
        next_seq: u64,
        offer: Option<&Offer>,
    ) -> Result<()> {
        let next_seq = to_i64(next_seq)?;
        self.conn
            .prepare_cached(
                "INSERT OR REPLACE INTO follows (address, next_seq, offer_to, offer, rejoin)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .and_then(|mut stmt| {
                stmt.execute(params![
                    address,
                    next_seq,
                    offer.map(|o| &o.to),
                    offer.map(|o| &o.key_package),
                    offer.is_some_and(|o| o.rejoin),
                ])
            })
            .map(drop)
            .map_err(store)
    }

    /// Stop reading `address`.
    pub(crate) fn unfollow(&self, address: &[u8]) -> Result<()> {
        self.conn
            .execute("DELETE FROM follows WHERE address = ?1", [address])
            .map(drop)
            .map_err(store)
    }

    /// Stop reading every mailbox made for an offer to `channel`: the offers
    /// are answered. Offers to other channels keep theirs.
    pub(crate) fn unfollow_mailboxes(&self, channel: &[u8]) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM follows WHERE offer IS NOT NULL AND offer_to = ?1",
                [channel],
            )
            .map(drop)
            .map_err(store)
    }

    /// The channel an offer made from `mailbox` asked to join, if `mailbox`
    /// is one of this client's mailboxes.
    pub(crate) fn offered_to(&self, mailbox: &[u8]) -> Result<Option<Vec<u8>>> {
        self.conn
            .prepare_cached("SELECT offer_to FROM follows WHERE address = ?1 AND offer IS NOT NULL")
            .and_then(|mut stmt| stmt.query_row([mailbox], |r| r.get(0)).optional())
            .map_err(store)
    }

    /// The first unapplied sequence number on `address`, or `None` if this
    /// client does not read it.
    pub(crate) fn cursor(&self, address: &[u8]) -> Result<Option<u64>> {
        let next: Option<i64> = self
            .conn
            .prepare_cached("SELECT next_seq FROM follows WHERE address = ?1")
            .and_then(|mut stmt| stmt.query_row([address], |r| r.get(0)).optional())
            .map_err(store)?;
        next.map(|n| u64::try_from(n).map_err(|_| Error::CorruptState("negative cursor")))
            .transpose()
    }

    /// Record that everything on `address` before `next_seq` is applied.
    pub(crate) fn advance(&self, address: &[u8], next_seq: u64) -> Result<()> {
        let next_seq = to_i64(next_seq)?;
        self.conn
            .prepare_cached("UPDATE follows SET next_seq = ?2 WHERE address = ?1")
            .and_then(|mut stmt| stmt.execute(params![address, next_seq]))
            .map(drop)
            .map_err(store)
    }

    /// Messages sent to `channel` awaiting their echo, oldest first.
    pub(crate) fn sent(&self, channel: &[u8]) -> Result<Vec<SentRow>> {
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT wire, epoch, content, text FROM sent WHERE channel = ?1 ORDER BY id",
            )
            .map_err(store)?;
        let rows = stmt
            .query_map([channel], |r| {
                Ok((r.get(0)?, to_u64(r.get(1)?)?, r.get(2)?, r.get(3)?))
            })
            .map_err(store)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store)
    }

    /// Record a message sent to `channel`, keeping only the newest `keep`
    /// there.
    pub(crate) fn push_sent(
        &self,
        channel: &[u8],
        wire: &[u8],
        epoch: u64,
        content: &[u8],
        keep: usize,
    ) -> Result<()> {
        let keep = i64::try_from(keep).unwrap_or(i64::MAX);
        self.conn
            .execute(
                "INSERT INTO sent (channel, wire, epoch, content, text) VALUES (?1, ?2, ?3, ?4, '')",
                params![channel, wire, to_i64(epoch)?, content],
            )
            .and_then(|_| {
                self.conn.execute(
                    "DELETE FROM sent WHERE channel = ?1 AND id NOT IN
                         (SELECT id FROM sent WHERE channel = ?1 ORDER BY id DESC LIMIT ?2)",
                    params![channel, keep],
                )
            })
            .map(drop)
            .map_err(store)
    }

    /// Forget a message sent to `channel` whose echo has arrived.
    pub(crate) fn remove_sent(&self, channel: &[u8], wire: &[u8]) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM sent WHERE channel = ?1 AND wire = ?2",
                params![channel, wire],
            )
            .map(drop)
            .map_err(store)
    }

    /// Forget every message sent to `channel`: this client left its group.
    pub(crate) fn clear_sent(&self, channel: &[u8]) -> Result<()> {
        self.conn
            .execute("DELETE FROM sent WHERE channel = ?1", [channel])
            .map(drop)
            .map_err(store)
    }

    /// The commit staged in the group on `channel`, if there is one.
    pub(crate) fn pending_commit(&self, channel: &[u8]) -> Result<Option<Staged>> {
        self.conn
            .query_row(
                "SELECT commit_, welcome, mailbox, until FROM pending_commit WHERE channel = ?1",
                [channel],
                |r| {
                    let admits = match (r.get(1)?, r.get(2)?, r.get::<_, Option<i64>>(3)?) {
                        (Some(welcome), Some(mailbox), until) => {
                            Some((welcome, mailbox, to_u64(until.unwrap_or(0))?))
                        }
                        _ => None,
                    };
                    Ok((r.get(0)?, admits))
                },
            )
            .optional()
            .map_err(store)
    }

    /// Record the commit staged in the group on `channel`, or clear it.
    pub(crate) fn set_pending_commit(
        &self,
        channel: &[u8],
        pending: Option<StagedRef<'_>>,
    ) -> Result<()> {
        self.conn
            .execute("DELETE FROM pending_commit WHERE channel = ?1", [channel])
            .map_err(store)?;
        if let Some((commit, admits)) = pending {
            let (welcome, mailbox, until) = match admits {
                Some((w, m, u)) => (Some(w), Some(m), Some(to_i64(u)?)),
                None => (None, None, None),
            };
            self.conn
                .execute(
                    "INSERT INTO pending_commit (channel, commit_, welcome, mailbox, until)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![channel, commit, welcome, mailbox, until],
                )
                .map_err(store)?;
        }
        Ok(())
    }

    /// This client's standing request to leave the group on `channel`, and
    /// the epoch it was made in.
    pub(crate) fn leaving(&self, channel: &[u8]) -> Result<Option<(Vec<u8>, u64)>> {
        self.conn
            .query_row(
                "SELECT proposal, epoch FROM leaving WHERE channel = ?1",
                [channel],
                |r| Ok((r.get(0)?, to_u64(r.get(1)?)?)),
            )
            .optional()
            .map_err(store)
    }

    /// Record a request to leave the group on `channel`, or clear it.
    pub(crate) fn set_leaving(&self, channel: &[u8], leaving: Option<(&[u8], u64)>) -> Result<()> {
        self.conn
            .execute("DELETE FROM leaving WHERE channel = ?1", [channel])
            .map_err(store)?;
        if let Some((proposal, epoch)) = leaving {
            self.conn
                .execute(
                    "INSERT INTO leaving (channel, proposal, epoch) VALUES (?1, ?2, ?3)",
                    params![channel, proposal, to_i64(epoch)?],
                )
                .map_err(store)?;
        }
        Ok(())
    }

    /// Remember that the request with this `KeyPackageRef` was admitted to
    /// the group on `channel`, until it expires. Forgets any that already
    /// have, in every group.
    pub(crate) fn remember_admitted(
        &self,
        channel: &[u8],
        key_package_ref: &[u8],
        until: u64,
        now: u64,
    ) -> Result<()> {
        self.conn
            .execute("DELETE FROM admitted WHERE until <= ?1", [to_i64(now)?])
            .map_err(store)?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO admitted (channel, key_package_ref, until)
                 VALUES (?1, ?2, ?3)",
                params![channel, key_package_ref, to_i64(until)?],
            )
            .map(drop)
            .map_err(store)
    }

    /// Whether a request with this `KeyPackageRef` was already admitted to
    /// the group on `channel`.
    pub(crate) fn was_admitted(&self, channel: &[u8], key_package_ref: &[u8]) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT 1 FROM admitted WHERE channel = ?1 AND key_package_ref = ?2",
                params![channel, key_package_ref],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
            .map_err(store)
    }

    /// Record a request to join the group on `channel`, or replace the
    /// identity's earlier one. Returns false if that identity was declined
    /// there, in which case nothing changes.
    pub(crate) fn note_request(
        &self,
        channel: &[u8],
        signature_key: &[u8],
        key_package: &[u8],
        name: &str,
        now: u64,
    ) -> Result<bool> {
        let declined: Option<bool> = self
            .conn
            .query_row(
                "SELECT declined FROM join_requests WHERE channel = ?1 AND signature_key = ?2",
                params![channel, signature_key],
                |r| r.get(0),
            )
            .optional()
            .map_err(store)?;
        if declined == Some(true) {
            return Ok(false);
        }
        self.conn
            .execute(
                "INSERT INTO join_requests (channel, signature_key, key_package, name, seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (channel, signature_key) DO UPDATE SET
                     key_package = excluded.key_package,
                     name = excluded.name",
                params![channel, signature_key, key_package, name, to_i64(now)?],
            )
            .map(|_| true)
            .map_err(store)
    }

    /// Requests to join the group on `channel` that nobody here has accepted
    /// or declined, oldest first: `(signature key, name, first seen)`.
    pub(crate) fn pending_requests(&self, channel: &[u8]) -> Result<Vec<(Vec<u8>, String, u64)>> {
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT signature_key, name, seen_at FROM join_requests
                 WHERE channel = ?1 AND declined = 0 ORDER BY seen_at, rowid",
            )
            .map_err(store)?;
        let rows = stmt
            .query_map([channel], |r| {
                Ok((r.get(0)?, r.get(1)?, to_u64(r.get(2)?)?))
            })
            .map_err(store)?;
        rows.collect::<rusqlite::Result<_>>().map_err(store)
    }

    /// The latest `KeyPackage` from a pending request to join the group on
    /// `channel`, taking it off the list.
    pub(crate) fn take_request(
        &self,
        channel: &[u8],
        signature_key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let key_package = self.request_of(channel, signature_key)?;
        self.drop_request(channel, signature_key)?;
        Ok(key_package)
    }

    /// Decline an identity's request to join the group on `channel`; its
    /// later ones there are ignored too.
    pub(crate) fn decline_request(&self, channel: &[u8], signature_key: &[u8]) -> Result<bool> {
        self.conn
            .execute(
                "UPDATE join_requests SET declined = 1 WHERE channel = ?1 AND signature_key = ?2",
                params![channel, signature_key],
            )
            .map(|n| n > 0)
            .map_err(store)
    }

    /// Forget an identity's request to join the group on `channel`: it was
    /// accepted, by anyone.
    pub(crate) fn drop_request(&self, channel: &[u8], signature_key: &[u8]) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM join_requests
                 WHERE channel = ?1 AND signature_key = ?2 AND declined = 0",
                params![channel, signature_key],
            )
            .map(drop)
            .map_err(store)
    }

    /// Record that this client accepted an identity into the group on
    /// `channel`.
    pub(crate) fn approve(
        &self,
        channel: &[u8],
        signature_key: &[u8],
        name: &str,
        at: u64,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO approved (channel, signature_key, name, at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![channel, signature_key, name, to_i64(at)?],
            )
            .map(drop)
            .map_err(store)
    }

    /// Whether this client has accepted an identity into the group on
    /// `channel` and not yet seen it admitted.
    pub(crate) fn is_approved(&self, channel: &[u8], signature_key: &[u8]) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT 1 FROM approved WHERE channel = ?1 AND signature_key = ?2",
                params![channel, signature_key],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
            .map_err(store)
    }

    /// Spend or cancel an approval to the group on `channel`: the identity
    /// was admitted, or removed.
    pub(crate) fn unapprove(&self, channel: &[u8], signature_key: &[u8]) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM approved WHERE channel = ?1 AND signature_key = ?2",
                params![channel, signature_key],
            )
            .map(drop)
            .map_err(store)
    }

    /// The latest `KeyPackage` from an identity's request to join the group
    /// on `channel`, if it has one that was not declined.
    pub(crate) fn request_of(
        &self,
        channel: &[u8],
        signature_key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        self.conn
            .query_row(
                "SELECT key_package FROM join_requests
                 WHERE channel = ?1 AND signature_key = ?2 AND declined = 0",
                params![channel, signature_key],
                |r| r.get(0),
            )
            .optional()
            .map_err(store)
    }

    /// Record that a member of the group on `channel` was removed (`by`
    /// someone else) or left (`by` themselves).
    pub(crate) fn note_removed(
        &self,
        channel: &[u8],
        signature_key: &[u8],
        name: &str,
        by: &str,
        at: u64,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO removed (channel, signature_key, name, by, at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![channel, signature_key, name, by, to_i64(at)?],
            )
            .map(drop)
            .map_err(store)
    }

    /// The latest removal of this identity from the group on `channel`, or
    /// failing that of anyone there with this name: `(same identity, name,
    /// by, at)`.
    pub(crate) fn removal_of(
        &self,
        channel: &[u8],
        signature_key: &[u8],
        name: &str,
    ) -> Result<Option<(bool, String, String, u64)>> {
        self.conn
            .query_row(
                "SELECT signature_key = ?2, name, by, at FROM removed
                 WHERE channel = ?1 AND (signature_key = ?2 OR name = ?3)
                 ORDER BY signature_key = ?2 DESC, at DESC LIMIT 1",
                params![channel, signature_key, name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, to_u64(r.get(3)?)?)),
            )
            .optional()
            .map_err(store)
    }

    /// Forget every request held for the group on `channel`.
    pub(crate) fn clear_deferred(&self, channel: &[u8]) -> Result<()> {
        self.conn
            .execute("DELETE FROM deferred WHERE channel = ?1", [channel])
            .map(drop)
            .map_err(store)
    }

    /// Hold a request to join, or to be re-added to, the group on `channel`,
    /// to answer later.
    pub(crate) fn defer(&self, channel: &[u8], key_package: &[u8], rejoin: bool) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO deferred (channel, key_package, rejoin) VALUES (?1, ?2, ?3)",
                params![channel, key_package, rejoin],
            )
            .map(drop)
            .map_err(store)
    }

    /// The oldest request held for the group on `channel`, removed from the
    /// list, and whether it asks to be re-added.
    pub(crate) fn take_deferred(&self, channel: &[u8]) -> Result<Option<(Vec<u8>, bool)>> {
        let oldest: Option<(i64, Vec<u8>, bool)> = self
            .conn
            .query_row(
                "SELECT id, key_package, rejoin FROM deferred
                 WHERE channel = ?1 ORDER BY id LIMIT 1",
                [channel],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(store)?;
        let Some((id, key_package, rejoin)) = oldest else {
            return Ok(None);
        };
        self.conn
            .execute("DELETE FROM deferred WHERE id = ?1", [id])
            .map_err(store)?;
        Ok(Some((key_package, rejoin)))
    }

    /// Add a message to the transcript, or place one already there.
    ///
    /// A message already placed is left alone, so a duplicate changes
    /// nothing. This client's own message is written [`Placement::Unsent`]
    /// when it is encrypted, and placed when its echo comes back: the same
    /// row, since a resend keeps the message id.
    pub(crate) fn log_message(
        &self,
        entry: &LogEntry<'_>,
        placement: Placement,
        now_ms: u64,
    ) -> Result<()> {
        let (seq, accepted_ms) = match placement {
            Placement::Applied(Some(relayed)) => (
                Some(to_i64(relayed.seq)?),
                Some(to_i64(relayed.accepted_ms)?),
            ),
            Placement::Applied(None) | Placement::Unsent => (None, None),
        };
        let now_ms = to_i64(now_ms)?;
        let sql = match placement {
            Placement::Unsent => {
                "INSERT INTO transcript (channel, id, position, seq, accepted_ms, stored_ms,
                     mine, sender, sender_uri, content, replaces, in_reply_to)
                 VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT (channel, id) DO NOTHING"
            }
            Placement::Applied(_) => {
                "INSERT INTO transcript (channel, id, position, seq, accepted_ms, stored_ms,
                     mine, sender, sender_uri, content, replaces, in_reply_to)
                 VALUES (?1, ?2, (SELECT coalesce(max(position), 0) + 1 FROM transcript),
                     ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT (channel, id) DO UPDATE SET
                     position = excluded.position,
                     seq = excluded.seq,
                     accepted_ms = excluded.accepted_ms
                 WHERE position IS NULL"
            }
        };
        self.conn
            .prepare_cached(sql)
            .and_then(|mut stmt| {
                stmt.execute(params![
                    entry.channel,
                    entry.id,
                    seq,
                    accepted_ms,
                    now_ms,
                    entry.mine,
                    entry.sender,
                    entry.sender_uri,
                    entry.content,
                    entry.replaces,
                    entry.in_reply_to,
                ])
            })
            .map(drop)
            .map_err(store)
    }

    /// Up to `limit` messages on `channel`, oldest first: the latest ones,
    /// this client's unplaced messages last, or with `before`, the placed
    /// ones just before that position.
    pub(crate) fn history(
        &self,
        channel: &[u8],
        before: Option<u64>,
        limit: u32,
    ) -> Result<Vec<LogRow>> {
        const COLUMNS: &str = "id, position, seq, accepted_ms, stored_ms, mine, sender,
            sender_uri, content";
        let row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<LogRow> {
            let opt = |i| -> rusqlite::Result<Option<u64>> {
                r.get::<_, Option<i64>>(i)?.map(to_u64).transpose()
            };
            Ok(LogRow {
                id: r.get(0)?,
                position: opt(1)?,
                seq: opt(2)?,
                accepted_ms: opt(3)?,
                stored_ms: to_u64(r.get(4)?)?,
                mine: r.get(5)?,
                sender: r.get(6)?,
                sender_uri: r.get(7)?,
                content: r.get(8)?,
            })
        };
        let mut rows = match before {
            None => {
                let mut stmt = self
                    .conn
                    .prepare_cached(&format!(
                        "SELECT {COLUMNS} FROM transcript WHERE channel = ?1
                         ORDER BY position IS NULL DESC, position DESC, rowid DESC
                         LIMIT ?2"
                    ))
                    .map_err(store)?;
                stmt.query_map(params![channel, limit], row)
                    .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
            }
            Some(before) => {
                // Positions are i64 in SQLite: anything larger is "before
                // everything", which is what a caller passing u64::MAX means.
                let before = i64::try_from(before).unwrap_or(i64::MAX);
                let mut stmt = self
                    .conn
                    .prepare_cached(&format!(
                        "SELECT {COLUMNS} FROM transcript WHERE channel = ?1 AND position < ?2
                         ORDER BY position DESC LIMIT ?3"
                    ))
                    .map_err(store)?;
                stmt.query_map(params![channel, before, limit], row)
                    .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
            }
        }
        .map_err(store)?;
        rows.reverse();
        Ok(rows)
    }
}

/// A message as written to the transcript.
pub(crate) struct LogEntry<'a> {
    pub(crate) channel: &'a [u8],
    pub(crate) id: &'a [u8],
    pub(crate) sender: &'a str,
    pub(crate) sender_uri: &'a str,
    pub(crate) mine: bool,
    pub(crate) content: &'a [u8],
    pub(crate) replaces: Option<&'a [u8]>,
    pub(crate) in_reply_to: Option<&'a [u8]>,
}

/// Where a relayed frame landed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Relayed {
    /// Its sequence number on its address.
    pub(crate) seq: u64,
    /// When the relay accepted it, in milliseconds since the Unix epoch.
    pub(crate) accepted_ms: u64,
}

/// Whether a message written to the transcript has its place in the
/// conversation yet.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Placement {
    /// This client's own, encrypted but not yet seen coming back.
    Unsent,
    /// Applied in order: decrypted, or this client's own seen coming back.
    /// With where the relay put it, when it came through the relay.
    Applied(Option<Relayed>),
}

/// A message as read back from the transcript.
pub(crate) struct LogRow {
    pub(crate) id: Vec<u8>,
    pub(crate) position: Option<u64>,
    pub(crate) seq: Option<u64>,
    pub(crate) accepted_ms: Option<u64>,
    pub(crate) stored_ms: u64,
    pub(crate) mine: bool,
    pub(crate) sender: String,
    pub(crate) sender_uri: String,
    pub(crate) content: Vec<u8>,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A store from before #134 named its one group on the client row. It
    /// opens with that group in the list, and with what was in flight in it
    /// moved under its channel.
    #[test]
    fn a_store_with_one_group_moves_to_the_group_list() {
        let mut conn = Connection::open_in_memory().unwrap();
        SqliteStorageProvider::<JsonCodec, &mut Connection>::new(&mut conn)
            .run_migrations()
            .unwrap();
        let tx = conn.transaction().unwrap();
        for migration in &MIGRATIONS[..7] {
            tx.execute_batch(migration).unwrap();
        }
        tx.pragma_update(None, "user_version", 7).unwrap();
        tx.execute_batch(
            "INSERT INTO client (id, identity, signature_public_key, group_id)
                 VALUES (0, 'alice', x'01', x'c0ffee');
             INSERT INTO pending_commit (id, commit_, welcome, mailbox, until)
                 VALUES (0, x'aa', x'ab', x'ac', 9);
             INSERT INTO leaving (id, proposal, epoch) VALUES (0, x'bb', 3);
             INSERT INTO sent (wire, epoch, text, content) VALUES (x'cc', 3, '', x'cd');
             INSERT INTO deferred (key_package, rejoin) VALUES (x'dd', 1);
             INSERT INTO join_requests (signature_key, key_package, name, seen_at)
                 VALUES (x'ee', x'ef', 'bob', 1);
             INSERT INTO removed (signature_key, name, by, at) VALUES (x'f0', 'eve', 'alice', 2);
             INSERT INTO approved (signature_key, name, at) VALUES (x'f1', 'dan', 3);
             INSERT INTO admitted (key_package_ref, until) VALUES (x'f2', 99);",
        )
        .unwrap();
        tx.commit().unwrap();

        let db = Database::init(conn, None).unwrap();
        let channel = [0xc0, 0xff, 0xee];
        assert!(db.load().unwrap().is_some(), "lost who this client is");
        assert_eq!(db.groups().unwrap(), [channel.to_vec()]);
        assert_eq!(
            db.pending_commit(&channel).unwrap(),
            Some((vec![0xaa], Some((vec![0xab], vec![0xac], 9))))
        );
        assert_eq!(db.leaving(&channel).unwrap(), Some((vec![0xbb], 3)));
        assert_eq!(
            db.sent(&channel).unwrap(),
            [(vec![0xcc], 3, Some(vec![0xcd]), String::new())]
        );
        assert_eq!(
            db.take_deferred(&channel).unwrap(),
            Some((vec![0xdd], true))
        );
        assert_eq!(
            db.pending_requests(&channel).unwrap(),
            [(vec![0xee], "bob".to_owned(), 1)]
        );
        assert!(db.removal_of(&channel, &[0xf0], "eve").unwrap().is_some());
        assert!(db.is_approved(&channel, &[0xf1]).unwrap());
        assert!(db.was_admitted(&channel, &[0xf2]).unwrap());
    }
}
