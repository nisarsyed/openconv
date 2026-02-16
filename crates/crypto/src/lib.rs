//! openconv-crypto -- Signal protocol implementation for OpenConv.
//!
//! Provides identity keypair generation (Curve25519), X3DH key agreement,
//! Double Ratchet message encryption, AES-256-GCM file encryption, and
//! secure key storage backed by encrypted SQLite (SQLCipher).
//!
//! ## Architecture
//!
//! - **Sync public API**: All public functions are synchronous. Callers in async
//!   contexts (e.g., Tauri commands) should use `spawn_blocking`.
//! - **Caller-provided connection**: Functions accept `&rusqlite::Connection`.
//!   The desktop app manages the connection lifecycle and passes it in.
//! - **libsignal internally**: Uses the reference Signal protocol implementation
//!   for X3DH and Double Ratchet. Async trait implementations bridge to sync
//!   SQLite via `futures::executor::block_on`.
//!
//! ## Modules
//!
//! - [`error`] -- `CryptoError` enum
//! - [`master_key`] -- OS keychain and passphrase-based key management
//! - [`storage`] -- SQLite storage layer and libsignal store trait implementations
//! - [`identity`] -- Identity keypair generation and management
//! - [`prekeys`] -- Pre-key bundle and one-time pre-key management
//! - [`session`] -- Signal session creation and recovery
//! - [`message`] -- Message encryption and decryption
//! - [`file_encryption`] -- AES-256-GCM symmetric file encryption
//! - [`fingerprint`] -- Safety number generation and verification

pub mod error;
pub mod master_key;
pub mod storage;
pub mod identity;
pub mod prekeys;
pub mod session;
pub mod message;
pub mod file_encryption;
pub mod fingerprint;

#[cfg(test)]
mod tests {
    #[test]
    fn all_public_modules_accessible() {
        use crate::error::CryptoError;
        use crate::master_key::{MasterKey, DbEncryptionKey};
        use crate::identity;
        use crate::prekeys;
        use crate::session;
        use crate::message::{EncryptedMessage, MessageType};
        use crate::file_encryption::{FileKey, EncryptedBlob};
        use crate::fingerprint::Fingerprint;

        // Verify types are accessible via size_of (compile-time check)
        let _ = (
            std::mem::size_of::<CryptoError>(),
            std::mem::size_of::<MasterKey>(),
            std::mem::size_of::<DbEncryptionKey>(),
        );
        let _ = (
            identity::generate_identity as fn(&_) -> _,
            prekeys::generate_pre_key_bundle as fn(&_, &_) -> _,
        );
        let _ = session::create_outgoing_session as fn(&_, &_) -> _;
        let _ = std::mem::size_of::<EncryptedMessage>();
        let _ = std::mem::size_of::<MessageType>();
        let _ = std::mem::size_of::<FileKey>();
        let _ = std::mem::size_of::<EncryptedBlob>();
        let _ = std::mem::size_of::<Fingerprint>();

        // Verify CryptoStore is accessible
        let _ = std::mem::size_of::<crate::storage::CryptoStore>();
    }
}
