//! Error types for the openconv-crypto crate.

use thiserror::Error;

/// Errors that can occur during cryptographic operations.
#[derive(Debug, Error)]
pub enum CryptoError {
    /// The provided key material is invalid (wrong length, malformed, etc.).
    #[error("invalid key: {0}")]
    InvalidKey(String),

    /// Decryption failed (wrong key, tampered ciphertext, etc.).
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    /// No session exists for the given address.
    #[error("session not found for address: {address}")]
    SessionNotFound { address: String },

    /// Session state is corrupted and needs recovery.
    #[error("session corrupted for address {address}: {detail}")]
    SessionCorrupted { address: String, detail: String },

    /// No identity keypair has been generated yet.
    #[error("identity not initialized")]
    IdentityNotInitialized,

    /// All one-time pre-keys have been consumed.
    #[error("pre-keys exhausted")]
    PreKeyExhausted,

    /// Database storage error.
    #[error("storage error: {0}")]
    StorageError(String),

    /// OS keychain operation failed.
    #[error("keychain error: {0}")]
    KeychainError(String),

    /// No credential found in OS keychain for the requested entry.
    #[error("keychain entry not found")]
    KeychainEntryNotFound,

    /// OS keychain is not available on this platform — triggers passphrase fallback.
    #[error("keychain unavailable")]
    KeychainUnavailable,

    /// A passphrase is required but was not provided.
    #[error("passphrase required")]
    PassphraseRequired,

    /// Serialization or deserialization error.
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// Error from the Signal protocol layer.
    #[error("signal protocol error: {0}")]
    SignalProtocolError(String),

    /// File encryption/decryption error.
    #[error("file encryption error: {0}")]
    FileEncryptionError(String),
}

impl From<rusqlite::Error> for CryptoError {
    fn from(err: rusqlite::Error) -> Self {
        CryptoError::StorageError(err.to_string())
    }
}

impl From<serde_json::Error> for CryptoError {
    fn from(err: serde_json::Error) -> Self {
        CryptoError::SerializationError(err.to_string())
    }
}

impl From<CryptoError> for openconv_shared::error::OpenConvError {
    fn from(err: CryptoError) -> Self {
        openconv_shared::error::OpenConvError::Crypto(err.to_string())
    }
}

impl From<keyring::Error> for CryptoError {
    fn from(err: keyring::Error) -> Self {
        match err {
            keyring::Error::NoEntry => CryptoError::KeychainEntryNotFound,
            keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
                CryptoError::KeychainUnavailable
            }
            other => CryptoError::KeychainError(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_human_readable() {
        let err = CryptoError::InvalidKey("bad key data".into());
        assert!(err.to_string().contains("bad key data"));

        let err = CryptoError::SessionNotFound {
            address: "abc-123".into(),
        };
        assert!(err.to_string().contains("abc-123"));

        let err = CryptoError::IdentityNotInitialized;
        let msg = err.to_string();
        assert!(!msg.is_empty());

        let err = CryptoError::PreKeyExhausted;
        let msg = err.to_string();
        assert!(!msg.is_empty());

        let err = CryptoError::KeychainUnavailable;
        let msg = err.to_string();
        assert!(!msg.is_empty());

        let err = CryptoError::SessionCorrupted {
            address: "addr".into(),
            detail: "corrupt".into(),
        };
        assert!(err.to_string().contains("addr"));
    }

    #[test]
    fn from_rusqlite_error_converts_to_storage_error() {
        let rusqlite_err = rusqlite::Error::QueryReturnedNoRows;
        let crypto_err: CryptoError = rusqlite_err.into();
        match crypto_err {
            CryptoError::StorageError(_) => {}
            other => panic!("expected StorageError, got: {other:?}"),
        }
    }

    #[test]
    fn from_serde_json_error_converts_to_serialization_error() {
        let json_err: serde_json::Error = serde_json::from_str::<String>("not json").unwrap_err();
        let crypto_err: CryptoError = json_err.into();
        match crypto_err {
            CryptoError::SerializationError(_) => {}
            other => panic!("expected SerializationError, got: {other:?}"),
        }
    }

    #[test]
    fn from_keyring_no_entry_converts_to_entry_not_found() {
        let keyring_err = keyring::Error::NoEntry;
        let crypto_err: CryptoError = keyring_err.into();
        match crypto_err {
            CryptoError::KeychainEntryNotFound => {}
            other => panic!("expected KeychainEntryNotFound, got: {other:?}"),
        }
    }

    #[test]
    fn from_keyring_platform_failure_converts_to_unavailable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let keyring_err = keyring::Error::PlatformFailure(Box::new(io_err));
        let crypto_err: CryptoError = keyring_err.into();
        match crypto_err {
            CryptoError::KeychainUnavailable => {}
            other => panic!("expected KeychainUnavailable, got: {other:?}"),
        }
    }

    #[test]
    fn from_crypto_error_for_openconv_error() {
        let crypto_err = CryptoError::InvalidKey("test".into());
        let shared_err: openconv_shared::error::OpenConvError = crypto_err.into();
        match shared_err {
            openconv_shared::error::OpenConvError::Crypto(_) => {}
            other => panic!("expected Crypto variant, got: {other:?}"),
        }
    }

    #[test]
    fn all_variants_impl_error() {
        let errors: Vec<Box<dyn std::error::Error>> = vec![
            Box::new(CryptoError::InvalidKey("k".into())),
            Box::new(CryptoError::DecryptionFailed("d".into())),
            Box::new(CryptoError::SessionNotFound {
                address: "a".into(),
            }),
            Box::new(CryptoError::SessionCorrupted {
                address: "a".into(),
                detail: "d".into(),
            }),
            Box::new(CryptoError::IdentityNotInitialized),
            Box::new(CryptoError::PreKeyExhausted),
            Box::new(CryptoError::StorageError("s".into())),
            Box::new(CryptoError::KeychainError("k".into())),
            Box::new(CryptoError::KeychainEntryNotFound),
            Box::new(CryptoError::KeychainUnavailable),
            Box::new(CryptoError::PassphraseRequired),
            Box::new(CryptoError::SerializationError("s".into())),
            Box::new(CryptoError::SignalProtocolError("s".into())),
            Box::new(CryptoError::FileEncryptionError("f".into())),
        ];
        for e in &errors {
            let _ = e.to_string();
        }
    }
}
