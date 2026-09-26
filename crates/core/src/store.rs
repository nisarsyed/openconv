//! On-disk persistence for client state.
//!
//! The whole MLS store is snapshotted as one encrypted blob. That is
//! deliberately simple rather than scalable: implementing openmls's
//! `StorageProvider` over SQLite means 72 trait methods, and a snapshot buys
//! correct persistence today. Revisit when group state outgrows a single
//! write per change.
//!
//! The blob holds private key material, so it is never written in the clear.
//! The data key lives in the macOS Keychain, not beside the file.

use crate::{Error, Result};
use openmls_rust_crypto::{MemoryStorage, RustCrypto};
use openmls_traits::OpenMlsProvider;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// An OpenMLS provider whose storage can be restored from a snapshot.
///
/// `OpenMlsRustCrypto` keeps its fields private and only offers `Default`,
/// so there is no way to hand it a pre-loaded store; this is the same pairing
/// with a constructor that accepts one.
#[derive(Default)]
pub struct Provider {
    crypto: RustCrypto,
    storage: MemoryStorage,
}

impl Provider {
    pub fn with_storage(storage: MemoryStorage) -> Self {
        Self { crypto: RustCrypto::default(), storage }
    }
}

impl OpenMlsProvider for Provider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = MemoryStorage;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }
    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }
    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}

/// Everything needed to reconstruct a [`crate::Member`] after a restart.
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub identity: String,
    /// Identifies which key pair in the store belongs to us.
    pub signature_public_key: Vec<u8>,
    /// Present once the member has created or joined a group.
    pub group_id: Option<Vec<u8>>,
    /// The MLS store, flattened to key/value pairs.
    ///
    /// `MemoryStorage`'s own serialize/deserialize sit behind its `test-utils`
    /// feature, and its `persistence` feature writes unencrypted JSON into the
    /// temp dir. Its `values` map is public, so we snapshot that instead and
    /// keep control of the format and the encryption.
    pub storage: Vec<(Vec<u8>, Vec<u8>)>,
}

const MAGIC: &[u8; 4] = b"OCV1";
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

/// Flatten a store into serialisable pairs.
pub fn dump(storage: &MemoryStorage) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let values = storage
        .values
        .read()
        .map_err(|_| Error::Store("storage lock poisoned".into()))?;
    Ok(values.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// Rebuild a store from pairs produced by [`dump`].
pub fn restore(pairs: Vec<(Vec<u8>, Vec<u8>)>) -> Result<MemoryStorage> {
    let storage = MemoryStorage::default();
    {
        let mut values = storage
            .values
            .write()
            .map_err(|_| Error::Store("storage lock poisoned".into()))?;
        values.extend(pairs);
    }
    Ok(storage)
}

/// An encrypted file holding one client's state.
pub struct Vault {
    path: PathBuf,
    key: [u8; KEY_LEN],
}

impl Vault {
    /// Open the vault at `path`, fetching (or creating) its data key in the
    /// Keychain under `account`.
    pub fn open(path: impl Into<PathBuf>, account: &str) -> Result<Self> {
        Ok(Self { path: path.into(), key: data_key(account)? })
    }

    /// Open a vault with an explicit data key, bypassing the Keychain.
    ///
    /// Used by tests, which must not touch the real Keychain, and the hook a
    /// passphrase-derived key would use.
    pub fn with_key(path: impl Into<PathBuf>, key: [u8; KEY_LEN]) -> Self {
        Self { path: path.into(), key }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Encrypt and write a snapshot, replacing any previous one.
    pub fn save(&self, snapshot: &Snapshot) -> Result<()> {
        use chacha20poly1305::{
            AeadCore, KeyInit, XChaCha20Poly1305,
            aead::{Aead, OsRng},
        };

        let plaintext = serde_json::to_vec(snapshot).map_err(|e| Error::Store(e.to_string()))?;
        let cipher = XChaCha20Poly1305::new((&self.key).into());
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_ref())
            .map_err(|e| Error::Store(format!("encrypt: {e}")))?;

        let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + ciphertext.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Store(e.to_string()))?;
        }
        // Write then rename, so an interrupted save cannot truncate good state.
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, &out).map_err(|e| Error::Store(e.to_string()))?;
        restrict(&tmp)?;
        std::fs::rename(&tmp, &self.path).map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    /// Read and decrypt the snapshot, or `None` if the file does not exist.
    pub fn load(&self) -> Result<Option<Snapshot>> {
        use chacha20poly1305::{KeyInit, XChaCha20Poly1305, aead::Aead};

        let raw = match std::fs::read(&self.path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::Store(e.to_string())),
        };
        if raw.len() < MAGIC.len() + NONCE_LEN || &raw[..MAGIC.len()] != MAGIC {
            return Err(Error::Store("not an openconv vault".into()));
        }

        let nonce = &raw[MAGIC.len()..MAGIC.len() + NONCE_LEN];
        let ciphertext = &raw[MAGIC.len() + NONCE_LEN..];
        let cipher = XChaCha20Poly1305::new((&self.key).into());
        // A failure here means a wrong key or a tampered file. Surface it
        // rather than silently starting fresh, which would look like data loss.
        let plaintext = cipher
            .decrypt(nonce.into(), ciphertext)
            .map_err(|_| Error::Store("could not decrypt vault (wrong key or tampered)".into()))?;

        serde_json::from_slice(&plaintext)
            .map(Some)
            .map_err(|e| Error::Store(e.to_string()))
    }
}

/// Owner-only permissions on the state file.
#[cfg(unix)]
fn restrict(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| Error::Store(e.to_string()))
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<()> {
    Ok(())
}

const KEYCHAIN_SERVICE: &str = "com.openconv.vault";

/// Development override: keep the data key in a file beside the vault instead
/// of the Keychain.
///
/// An unsigned binary gets a new code identity on every rebuild, and the
/// Keychain then shows an ACL prompt when it reads an item an earlier build
/// created. Launched headlessly that prompt is invisible and the process
/// hangs forever, which is how this was found. Real installs are code-signed
/// and use the Keychain; dev and test runs set OPENCONV_DATA_DIR and get a
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
        Ok(dir) if !dir.is_empty() => {
            file_key(&Path::new(&dir).join(format!(".{account}.key")))
        }
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
                std::fs::create_dir_all(parent).map_err(|e| Error::Store(e.to_string()))?;
            }
            std::fs::write(path, key).map_err(|e| Error::Store(e.to_string()))?;
            restrict(path)?;
            Ok(key)
        }
        Err(e) => Err(Error::Store(e.to_string())),
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
    Err(Error::Store("encrypted storage is only implemented on macOS".into()))
}

fn getrandom(buf: &mut [u8]) -> Result<()> {
    use rand::RngCore;
    rand::rngs::OsRng
        .try_fill_bytes(buf)
        .map_err(|e| Error::Store(format!("rng: {e}")))
}
