//! openconv-crypto — Signal protocol implementation for OpenConv.
//!
//! Provides identity keypair generation, X3DH key agreement, Double Ratchet
//! message encryption, AES-256-GCM file encryption, and secure key storage
//! backed by encrypted SQLite (SQLCipher).

pub mod error;
pub mod master_key;
pub mod storage;
pub mod identity;
pub mod prekeys;
pub mod session;
pub mod message;
pub mod file_encryption;
pub mod fingerprint;
