//! Master key management for openconv-crypto.
//!
//! Provides two-tier key management: a 32-byte master key (from OS keychain or
//! user passphrase via Argon2id), derived into a database encryption key via
//! HKDF-SHA256 for SQLCipher.

use crate::error::CryptoError;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

const KEYCHAIN_SERVICE: &str = "com.openconv.crypto";
const KEYCHAIN_ACCOUNT: &str = "master_key";
const DB_KEY_INFO: &[u8] = b"openconv-db-encryption-v1";

/// A 32-byte master key, securely wiped from memory on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey {
    key: [u8; 32],
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MasterKey")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl MasterKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }
}

/// Hex-encoded database encryption key formatted for SQLCipher's `PRAGMA key`.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DbEncryptionKey {
    hex: String,
}

impl std::fmt::Debug for DbEncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbEncryptionKey")
            .field("hex", &"[REDACTED]")
            .finish()
    }
}

impl DbEncryptionKey {
    /// Returns the full `x'...'` string for use in PRAGMA statements.
    pub fn as_pragma_value(&self) -> &str {
        &self.hex
    }
}

/// Whether a database file is encrypted.
#[derive(Debug, PartialEq)]
pub enum EncryptionStatus {
    Unencrypted,
    Encrypted,
}

/// Retrieve or generate a master key via the OS keychain.
///
/// On first run, generates 32 random bytes and stores them in the keychain.
/// On subsequent runs, retrieves the stored key.
pub fn init_master_key_from_keychain() -> Result<MasterKey, CryptoError> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|_| CryptoError::KeychainUnavailable)?;

    match entry.get_password() {
        Ok(mut hex_string) => {
            let mut bytes = hex_decode(&hex_string)
                .ok_or_else(|| CryptoError::KeychainError("malformed master key in keychain".into()))?;
            hex_string.zeroize();

            if bytes.len() != 32 {
                bytes.zeroize();
                return Err(CryptoError::KeychainError("malformed master key in keychain".into()));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            bytes.zeroize();
            Ok(MasterKey { key })
        }
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::rng(), &mut key);
            let mut hex_string = hex_encode(&key);
            entry
                .set_password(&hex_string)
                .map_err(CryptoError::from)?;
            hex_string.zeroize();
            Ok(MasterKey { key })
        }
        Err(e) => Err(CryptoError::from(e)),
    }
}

/// Derive a master key from a user passphrase and salt via Argon2id.
pub fn init_master_key_from_passphrase(
    passphrase: &str,
    salt: &[u8],
) -> Result<MasterKey, CryptoError> {
    if salt.len() < 16 {
        return Err(CryptoError::InvalidKey("salt too short".into()));
    }

    let params = argon2::Params::new(65536, 3, 4, Some(32))
        .map_err(|e| CryptoError::InvalidKey(e.to_string()))?;
    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let mut output = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut output)
        .map_err(|e| CryptoError::InvalidKey(e.to_string()))?;

    Ok(MasterKey { key: output })
}

/// Generate a random 16-byte salt for passphrase derivation.
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut salt);
    salt
}

/// Derive a database encryption key from a master key via HKDF-SHA256.
pub fn derive_db_encryption_key(master_key: &MasterKey) -> Result<DbEncryptionKey, CryptoError> {
    let hk = Hkdf::<Sha256>::new(None, master_key.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(DB_KEY_INFO, &mut okm)
        .map_err(|e| CryptoError::InvalidKey(e.to_string()))?;

    let mut hex_str = hex_encode(&okm);
    okm.zeroize();

    let result = DbEncryptionKey {
        hex: format!("x'{hex_str}'"),
    };
    hex_str.zeroize();

    Ok(result)
}

/// Apply a SQLCipher encryption key to a database connection.
///
/// Uses `execute_batch` with the hex literal embedded directly in SQL, because
/// SQLCipher's `x'...'` syntax is a SQL literal that cannot be bound as a parameter.
pub fn apply_encryption_key(
    conn: &rusqlite::Connection,
    db_key: &DbEncryptionKey,
) -> Result<(), CryptoError> {
    // NOTE: We use execute_batch instead of pragma_update because SQLCipher's
    // x'...' hex key syntax is a SQL literal. If passed as a bound parameter,
    // SQLCipher treats it as a passphrase and applies PBKDF2, producing a
    // different key than intended.
    conn.execute_batch(&format!("PRAGMA key = \"{}\";", db_key.as_pragma_value()))?;

    let cipher_version: String = conn
        .pragma_query_value(None, "cipher_version", |row| row.get(0))
        .map_err(|_| CryptoError::StorageError("SQLCipher not available".into()))?;

    if cipher_version.is_empty() {
        return Err(CryptoError::StorageError("SQLCipher not available".into()));
    }

    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    Ok(())
}

/// Detect whether a database is encrypted or unencrypted.
pub fn detect_encryption_status(
    conn: &rusqlite::Connection,
) -> Result<EncryptionStatus, CryptoError> {
    match conn.execute_batch("SELECT count(*) FROM sqlite_master") {
        Ok(()) => Ok(EncryptionStatus::Unencrypted),
        Err(rusqlite::Error::SqliteFailure(err, _)) if err.extended_code == 26 => {
            Ok(EncryptionStatus::Encrypted)
        }
        Err(e) => Err(CryptoError::StorageError(e.to_string())),
    }
}

// NOTE: These hex helpers are not constant-time. They operate on key material
// but timing side-channels are not a concern for local keychain storage.

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passphrase_key(passphrase: &str, salt: &[u8]) -> MasterKey {
        init_master_key_from_passphrase(passphrase, salt).unwrap()
    }

    // --- Passphrase Path ---

    #[test]
    fn test_init_master_key_from_passphrase_produces_32_byte_key() {
        let salt = [0u8; 16];
        let mk = passphrase_key("test-passphrase", &salt);
        assert_eq!(mk.as_bytes().len(), 32);
    }

    #[test]
    fn test_same_passphrase_same_salt_produces_same_key() {
        let salt = [1u8; 16];
        let mk1 = passphrase_key("same-pass", &salt);
        let mk2 = passphrase_key("same-pass", &salt);
        assert_eq!(mk1.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn test_different_passphrase_same_salt_produces_different_key() {
        let salt = [2u8; 16];
        let mk1 = passphrase_key("password1", &salt);
        let mk2 = passphrase_key("password2", &salt);
        assert_ne!(mk1.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn test_same_passphrase_different_salt_produces_different_key() {
        let mk1 = passphrase_key("same-pass", &[3u8; 16]);
        let mk2 = passphrase_key("same-pass", &[4u8; 16]);
        assert_ne!(mk1.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn test_salt_too_short_returns_error() {
        let result = init_master_key_from_passphrase("pass", &[0u8; 8]);
        assert!(result.is_err());
    }

    // --- DB Key Derivation ---

    #[test]
    fn test_derive_db_encryption_key_format() {
        let mk = passphrase_key("test", &[5u8; 16]);
        let db_key = derive_db_encryption_key(&mk).unwrap();
        let val = db_key.as_pragma_value();
        assert!(val.starts_with("x'"));
        assert!(val.ends_with("'"));
    }

    #[test]
    fn test_derive_db_encryption_key_length() {
        let mk = passphrase_key("test", &[6u8; 16]);
        let db_key = derive_db_encryption_key(&mk).unwrap();
        let val = db_key.as_pragma_value();
        // x'<64 hex chars>' = 2 + 64 + 1 = 67
        assert_eq!(val.len(), 67);
    }

    #[test]
    fn test_derive_db_encryption_key_deterministic() {
        let mk1 = passphrase_key("det-test", &[7u8; 16]);
        let mk2 = passphrase_key("det-test", &[7u8; 16]);
        let k1 = derive_db_encryption_key(&mk1).unwrap();
        let k2 = derive_db_encryption_key(&mk2).unwrap();
        assert_eq!(k1.as_pragma_value(), k2.as_pragma_value());
    }

    #[test]
    fn test_different_master_keys_produce_different_db_keys() {
        let mk1 = passphrase_key("key-a", &[8u8; 16]);
        let mk2 = passphrase_key("key-b", &[8u8; 16]);
        let k1 = derive_db_encryption_key(&mk1).unwrap();
        let k2 = derive_db_encryption_key(&mk2).unwrap();
        assert_ne!(k1.as_pragma_value(), k2.as_pragma_value());
    }

    // --- Applying to Connection ---

    #[test]
    fn test_apply_encryption_key_succeeds_on_in_memory_db() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mk = passphrase_key("mem-test", &[9u8; 16]);
        let db_key = derive_db_encryption_key(&mk).unwrap();
        apply_encryption_key(&conn, &db_key).unwrap();

        let version: String = conn
            .pragma_query_value(None, "cipher_version", |row| row.get(0))
            .unwrap();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_apply_encryption_key_enables_read_write() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mk = passphrase_key("rw-test", &[10u8; 16]);
        let db_key = derive_db_encryption_key(&mk).unwrap();
        apply_encryption_key(&conn, &db_key).unwrap();

        conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY, val TEXT)")
            .unwrap();
        conn.execute("INSERT INTO test (val) VALUES (?1)", ["hello"])
            .unwrap();
        let val: String = conn
            .query_row("SELECT val FROM test WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(val, "hello");
    }

    #[test]
    fn test_wrong_key_fails_to_read() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Write with key A
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            let mk = passphrase_key("key-a", &[11u8; 16]);
            let db_key = derive_db_encryption_key(&mk).unwrap();
            apply_encryption_key(&conn, &db_key).unwrap();
            conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY)")
                .unwrap();
        }

        // Try to read with key B
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            let mk = passphrase_key("key-b", &[11u8; 16]);
            let db_key = derive_db_encryption_key(&mk).unwrap();
            conn.execute_batch(&format!("PRAGMA key = \"{}\";", db_key.as_pragma_value()))
                .unwrap();
            let result = conn.execute_batch("SELECT count(*) FROM sqlite_master");
            assert!(result.is_err());
        }
    }

    // --- Encryption Detection ---

    #[test]
    fn test_detect_encryption_status_unencrypted() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let status = detect_encryption_status(&conn).unwrap();
        assert_eq!(status, EncryptionStatus::Unencrypted);
    }

    #[test]
    fn test_detect_encryption_status_encrypted() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("enc.db");

        // Create encrypted DB
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            let mk = passphrase_key("enc-test", &[12u8; 16]);
            let db_key = derive_db_encryption_key(&mk).unwrap();
            apply_encryption_key(&conn, &db_key).unwrap();
            conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY)")
                .unwrap();
        }

        // Open without key and detect
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            let status = detect_encryption_status(&conn).unwrap();
            assert_eq!(status, EncryptionStatus::Encrypted);
        }
    }

    // --- MasterKey Debug is redacted ---

    #[test]
    fn test_master_key_debug_is_redacted() {
        let mk = passphrase_key("debug-test", &[14u8; 16]);
        let debug = format!("{mk:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains(&format!("{:02x}", mk.as_bytes()[0])));
    }

    #[test]
    fn test_db_encryption_key_debug_is_redacted() {
        let mk = passphrase_key("debug-test", &[15u8; 16]);
        let db_key = derive_db_encryption_key(&mk).unwrap();
        let debug = format!("{db_key:?}");
        assert!(debug.contains("REDACTED"));
    }

    // --- Salt Generation ---

    #[test]
    fn test_generate_salt_is_16_bytes() {
        let salt = generate_salt();
        assert_eq!(salt.len(), 16);
    }

    #[test]
    fn test_generate_salt_is_random() {
        let s1 = generate_salt();
        let s2 = generate_salt();
        assert_ne!(s1, s2);
    }
}
