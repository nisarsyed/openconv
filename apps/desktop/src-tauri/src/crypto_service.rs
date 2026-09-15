#![allow(dead_code)]

use std::sync::Mutex;

use libsignal_protocol::{DeviceId, ProtocolAddress};
use openconv_crypto::error::CryptoError;
use openconv_crypto::file_encryption::{self, EncryptedBlob, FileKey};
use openconv_crypto::message::{self, MessageType};
use openconv_crypto::session;
use openconv_crypto::storage::CryptoStore;
use reqwest::Client;
use rusqlite::Connection;
use zeroize::Zeroizing;

use crate::auth_service::AppError;

/// A per-device encrypted payload for one recipient.
#[derive(Debug, Clone)]
pub struct RecipientPayload {
    pub user_id: String,
    pub device_id: u32,
    pub ciphertext: Vec<u8>,
    pub message_type: String,
}

/// Identifies a specific device belonging to a user.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub user_id: String,
    pub device_id: u32,
}

/// Classification of decryption failures for the caller to handle.
#[derive(Debug)]
pub enum DecryptError {
    /// No Signal session exists for this sender/device.
    SessionNotFound { address: String },
    /// Session was corrupted; auto-recovery deleted it. Caller should
    /// re-fetch the sender's pre-key bundle and retry.
    SessionCorrupted { address: String },
    /// Ciphertext is permanently unrecoverable (tampered, wrong key, etc.).
    DecryptionFailed { detail: String },
    /// Other errors (storage, serialization).
    Other(String),
}

impl std::fmt::Display for DecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotFound { address } => write!(f, "session not found: {address}"),
            Self::SessionCorrupted { address } => write!(f, "session corrupted: {address}"),
            Self::DecryptionFailed { detail } => write!(f, "decryption failed: {detail}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DecryptError {}

/// Tauri managed state wrapper for CryptoService.
pub struct CryptoState {
    pub crypto_service: CryptoService,
}

// Signal device ids are assigned by the server (see the `devices` table) and
// travel with every device listing and every incoming message, so there is no
// client-side UUID → u32 mapping to maintain. A client-local mapping would be
// wrong: the id is half of the `ProtocolAddress`, so two peers inventing their
// own numbering would address the same physical device differently.

/// High-level wrapper around the `openconv_crypto` crate.
///
/// All methods are synchronous. Callers in async contexts MUST use
/// `tokio::task::spawn_blocking()` to avoid deadlocking the tokio runtime
/// (the crypto crate uses `futures::executor::block_on()` internally).
pub struct CryptoService {
    crypto_conn: Mutex<Connection>,
    http_client: Client,
    api_base_url: String,
}

impl CryptoService {
    /// Create a new CryptoService.
    ///
    /// Opens the crypto SQLCipher database, applies the encryption key
    /// derived from the OS keychain master key, and runs migrations.
    pub fn new(crypto_db_path: std::path::PathBuf, api_base_url: String) -> Result<Self, AppError> {
        use openconv_crypto::master_key;

        let conn = Connection::open(&crypto_db_path)
            .map_err(|e| AppError::new(format!("failed to open crypto DB: {e}")))?;

        let mk = master_key::init_master_key_from_keychain()
            .map_err(|e| AppError::new(format!("failed to init master key: {e}")))?;
        let db_key = master_key::derive_db_encryption_key(&mk)
            .map_err(|e| AppError::new(format!("failed to derive DB key: {e}")))?;
        master_key::apply_encryption_key(&conn, &db_key)
            .map_err(|e| AppError::new(format!("failed to apply encryption: {e}")))?;

        CryptoStore::new(&conn)
            .run_migrations()
            .map_err(|e| AppError::new(format!("failed to run crypto migrations: {e}")))?;

        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| AppError::new(format!("failed to create HTTP client: {e}")))?;

        Ok(Self {
            crypto_conn: Mutex::new(conn),
            api_base_url,
            http_client,
        })
    }

    /// Create a CryptoService for testing with an in-memory database.
    #[cfg(test)]
    pub fn new_for_testing() -> Self {
        use openconv_crypto::identity;

        let conn = Connection::open_in_memory().unwrap();
        CryptoStore::new(&conn).run_migrations().unwrap();
        identity::generate_identity(&conn).unwrap();
        Self {
            crypto_conn: Mutex::new(conn),
            api_base_url: String::new(),
            http_client: Client::new(),
        }
    }

    fn lock_crypto(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.crypto_conn
            .lock()
            .map_err(|e| AppError::new(format!("crypto DB lock poisoned: {e}")))
    }

    /// Establish a Signal session with a remote device if one does not already exist.
    ///
    /// Checks the local session store. If no session is found, uses the provided
    /// `bundle_json` to establish one via `create_outgoing_session`.
    ///
    /// Returns an error if no session exists and no bundle is provided. The
    /// caller (messaging pipeline) is responsible for pre-fetching bundles
    /// from the server before entering `spawn_blocking`.
    pub fn ensure_session(
        &self,
        user_id: &str,
        device_id: u32,
        bundle_json: Option<&[u8]>,
    ) -> Result<(), AppError> {
        let conn = self.lock_crypto()?;

        // Sessions are keyed by (address, device_id) — matching the primary key
        // of `crypto_sessions`. Checking the address alone would report a
        // session for *any* of the user's devices, so the first device to
        // establish one would suppress session creation for all the others and
        // every subsequent encrypt to them would fail with SessionNotFound.
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM crypto_sessions WHERE address = ?1 AND device_id = ?2",
                rusqlite::params![user_id, device_id],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if exists {
            return Ok(());
        }

        match bundle_json {
            Some(bundle) => {
                session::create_outgoing_session(&conn, bundle, device_id)?;
                Ok(())
            }
            None => Err(AppError::new(format!(
                "no session for {user_id}:{device_id} and no bundle provided"
            ))),
        }
    }

    /// Encrypt plaintext for all devices of all members in a channel.
    ///
    /// Returns a `Vec<RecipientPayload>` — one entry per member-device pair.
    /// Sessions must be pre-established via `ensure_session` for all devices.
    pub fn encrypt_for_channel(
        &self,
        member_devices: &[DeviceInfo],
        plaintext: &[u8],
    ) -> Result<Vec<RecipientPayload>, AppError> {
        self.encrypt_for_devices(member_devices, plaintext)
    }

    /// Encrypt plaintext for all devices of a single DM recipient.
    pub fn encrypt_for_dm(
        &self,
        recipient_devices: &[DeviceInfo],
        plaintext: &[u8],
    ) -> Result<Vec<RecipientPayload>, AppError> {
        self.encrypt_for_devices(recipient_devices, plaintext)
    }

    fn encrypt_for_devices(
        &self,
        devices: &[DeviceInfo],
        plaintext: &[u8],
    ) -> Result<Vec<RecipientPayload>, AppError> {
        let conn = self.lock_crypto()?;
        let mut payloads = Vec::with_capacity(devices.len());

        for device in devices {
            let device_id_u8: u8 = device
                .device_id
                .try_into()
                .map_err(|_| AppError::new(format!("device id {} exceeds u8", device.device_id)))?;
            let device_id = DeviceId::new(device_id_u8)
                .map_err(|e| AppError::new(format!("invalid device id: {e}")))?;
            let address = ProtocolAddress::new(device.user_id.clone(), device_id);

            let encrypted = message::encrypt_message(&conn, &address, plaintext)?;
            payloads.push(RecipientPayload {
                user_id: device.user_id.clone(),
                device_id: device.device_id,
                ciphertext: encrypted.ciphertext,
                message_type: encrypted.message_type.as_nonce_tag().to_string(),
            });
        }

        Ok(payloads)
    }

    /// Decrypt an incoming message from a specific sender device.
    ///
    /// Returns the decrypted plaintext bytes on success, or a `DecryptError`
    /// classifying the failure mode.
    pub fn decrypt_message(
        &self,
        sender_id: &str,
        device_id: u32,
        ciphertext: &[u8],
        message_type_str: &str,
    ) -> Result<Vec<u8>, DecryptError> {
        let msg_type = MessageType::from_nonce_tag(message_type_str).ok_or_else(|| {
            DecryptError::Other(format!("unknown message type: {message_type_str}"))
        })?;

        let conn = self
            .lock_crypto()
            .map_err(|e| DecryptError::Other(e.message))?;

        let device_id_u8: u8 = device_id
            .try_into()
            .map_err(|_| DecryptError::Other(format!("device id {device_id} exceeds u8")))?;
        let device_id = DeviceId::new(device_id_u8)
            .map_err(|e| DecryptError::Other(format!("invalid device id: {e}")))?;
        let address = ProtocolAddress::new(sender_id.to_string(), device_id);

        message::decrypt_message(&conn, &address, ciphertext, msg_type).map_err(|e| match e {
            CryptoError::SessionNotFound { address } => DecryptError::SessionNotFound { address },
            CryptoError::SessionCorrupted { address, .. } => {
                DecryptError::SessionCorrupted { address }
            }
            CryptoError::DecryptionFailed(detail) => DecryptError::DecryptionFailed { detail },
            other => DecryptError::Other(other.to_string()),
        })
    }

    /// Encrypt file bytes with AES-256-GCM.
    ///
    /// Returns `(encrypted_blob_bytes, file_key_bytes)`. The file key is a
    /// 32-byte symmetric key wrapped in `Zeroizing` to ensure it is wiped
    /// from memory on drop. Distribute it to recipients via
    /// `encrypt_key_for_recipients`.
    pub fn encrypt_file(
        &self,
        file_bytes: &[u8],
    ) -> Result<(Vec<u8>, Zeroizing<Vec<u8>>), AppError> {
        let (blob, key) = file_encryption::encrypt_file(file_bytes, None)?;
        Ok((blob.data, Zeroizing::new(key.as_bytes().to_vec())))
    }

    /// Decrypt an AES-256-GCM encrypted file blob.
    pub fn decrypt_file(
        &self,
        encrypted_bytes: &[u8],
        key_bytes: &[u8],
    ) -> Result<Vec<u8>, AppError> {
        let key = FileKey::from_bytes(key_bytes)?;
        let blob = EncryptedBlob {
            data: encrypted_bytes.to_vec(),
        };
        Ok(file_encryption::decrypt_file(&key, &blob, None)?)
    }

    /// Encrypt a symmetric file key for each device of the channel members.
    ///
    /// The 32-byte `file_key` is treated as a plaintext message and encrypted
    /// per-device using each member's Signal session.
    pub fn encrypt_key_for_recipients(
        &self,
        file_key: &[u8],
        member_devices: &[DeviceInfo],
    ) -> Result<Vec<RecipientPayload>, AppError> {
        self.encrypt_for_devices(member_devices, file_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openconv_crypto::prekeys;

    fn test_service() -> CryptoService {
        CryptoService::new_for_testing()
    }

    fn generate_bundle(service: &CryptoService, user_id: &str) -> Vec<u8> {
        let conn = service.lock_crypto().unwrap();
        let bundle = prekeys::generate_pre_key_bundle(&conn, user_id).unwrap();
        serde_json::to_vec(&bundle).unwrap()
    }

    /// Create Alice and Bob services with a session from Alice to Bob.
    fn setup_alice_bob() -> (CryptoService, CryptoService) {
        let alice = test_service();
        let bob = test_service();

        let bob_bundle = generate_bundle(&bob, "bob-user-id");
        alice
            .ensure_session("bob-user-id", 1, Some(&bob_bundle))
            .unwrap();

        (alice, bob)
    }

    // --- multi-device ---

    /// Regression test for the session-existence check.
    ///
    /// `crypto_sessions` is keyed by `(address, device_id)`. A check that
    /// matched on address alone reported "session exists" for *any* of a user's
    /// devices, so the first device to establish one suppressed session
    /// creation for every other device of that user.
    #[test]
    fn ensure_session_creates_a_separate_session_per_device() {
        let alice = test_service();
        let bob_d1 = test_service();
        let bob_d2 = test_service();

        let bundle_d1 = generate_bundle(&bob_d1, "bob-user-id");
        let bundle_d2 = generate_bundle(&bob_d2, "bob-user-id");

        alice
            .ensure_session("bob-user-id", 1, Some(&bundle_d1))
            .unwrap();
        alice
            .ensure_session("bob-user-id", 2, Some(&bundle_d2))
            .unwrap();

        let conn = alice.lock_crypto().unwrap();
        let device_ids: Vec<u32> = conn
            .prepare("SELECT device_id FROM crypto_sessions WHERE address = 'bob-user-id' ORDER BY device_id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(
            device_ids,
            vec![1, 2],
            "each device must get its own session"
        );
    }

    /// A second device of the same user must receive its own ciphertext, and
    /// must be able to decrypt it. Before the fix this produced one recipient
    /// instead of two, and encrypting to device 2 failed with SessionNotFound.
    #[test]
    fn encrypt_for_channel_reaches_every_device_of_a_user() {
        let alice = test_service();
        let bob_d1 = test_service();
        let bob_d2 = test_service();

        for (service, signal_device_id) in [(&bob_d1, 1u32), (&bob_d2, 2u32)] {
            let bundle = generate_bundle(service, "bob-user-id");
            alice
                .ensure_session("bob-user-id", signal_device_id, Some(&bundle))
                .unwrap();
        }

        let devices = vec![
            DeviceInfo {
                user_id: "bob-user-id".into(),
                device_id: 1,
            },
            DeviceInfo {
                user_id: "bob-user-id".into(),
                device_id: 2,
            },
        ];

        let payloads = alice.encrypt_for_channel(&devices, b"hello both").unwrap();
        assert_eq!(payloads.len(), 2, "one ciphertext per device");
        assert_ne!(
            payloads[0].ciphertext, payloads[1].ciphertext,
            "each device must get its own ciphertext, not a shared one"
        );

        // Each device decrypts only its own payload.
        for (service, payload) in [(&bob_d1, &payloads[0]), (&bob_d2, &payloads[1])] {
            let plaintext = service
                .decrypt_message(
                    "alice-user-id",
                    1,
                    &payload.ciphertext,
                    &payload.message_type,
                )
                .unwrap();
            assert_eq!(plaintext, b"hello both");
        }
    }

    // --- ensure_session ---

    #[test]
    fn ensure_session_creates_session_from_bundle() {
        let alice = test_service();
        let bob = test_service();

        let bob_bundle = generate_bundle(&bob, "bob-user-id");
        alice
            .ensure_session("bob-user-id", 1, Some(&bob_bundle))
            .unwrap();

        let conn = alice.lock_crypto().unwrap();
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions WHERE address = 'bob-user-id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn ensure_session_noop_if_session_exists() {
        let (alice, _bob) = setup_alice_bob();
        // Calling again without a bundle should succeed (session already exists)
        alice.ensure_session("bob-user-id", 1, None).unwrap();
    }

    #[test]
    fn ensure_session_errors_without_bundle_or_session() {
        let alice = test_service();
        let result = alice.ensure_session("unknown-user", 1, None);
        assert!(result.is_err());
    }

    // --- encrypt_for_channel ---

    #[test]
    fn encrypt_for_channel_encrypts_for_all_devices_of_all_members() {
        let alice = test_service();
        let bob = test_service();
        let charlie = test_service();

        let bob_bundle = generate_bundle(&bob, "bob-user-id");
        let charlie_bundle = generate_bundle(&charlie, "charlie-user-id");

        alice
            .ensure_session("bob-user-id", 1, Some(&bob_bundle))
            .unwrap();
        alice
            .ensure_session("charlie-user-id", 1, Some(&charlie_bundle))
            .unwrap();

        let devices = vec![
            DeviceInfo {
                user_id: "bob-user-id".into(),
                device_id: 1,
            },
            DeviceInfo {
                user_id: "charlie-user-id".into(),
                device_id: 1,
            },
        ];

        let payloads = alice
            .encrypt_for_channel(&devices, b"hello channel")
            .unwrap();
        assert_eq!(payloads.len(), 2);
        assert!(!payloads[0].ciphertext.is_empty());
        assert!(!payloads[1].ciphertext.is_empty());
    }

    #[test]
    fn encrypt_for_channel_returns_correct_user_and_device_ids() {
        let (alice, _bob) = setup_alice_bob();

        let devices = vec![DeviceInfo {
            user_id: "bob-user-id".into(),
            device_id: 1,
        }];
        let payloads = alice.encrypt_for_channel(&devices, b"test").unwrap();
        assert_eq!(payloads[0].user_id, "bob-user-id");
        assert_eq!(payloads[0].device_id, 1);
        assert!(payloads[0].message_type == "prekey" || payloads[0].message_type == "signal");
    }

    #[test]
    fn encrypt_for_channel_returns_error_if_no_session() {
        let alice = test_service();
        let devices = vec![DeviceInfo {
            user_id: "no-session-user".into(),
            device_id: 1,
        }];
        let result = alice.encrypt_for_channel(&devices, b"hello");
        assert!(result.is_err());
    }

    // --- decrypt_message ---

    #[test]
    fn decrypt_message_decrypts_ciphertext_from_known_sender() {
        let (alice, bob) = setup_alice_bob();

        let devices = vec![DeviceInfo {
            user_id: "bob-user-id".into(),
            device_id: 1,
        }];
        let payloads = alice
            .encrypt_for_channel(&devices, b"secret message")
            .unwrap();

        let plaintext = bob
            .decrypt_message(
                "alice-user-id",
                1,
                &payloads[0].ciphertext,
                &payloads[0].message_type,
            )
            .unwrap();
        assert_eq!(plaintext, b"secret message");
    }

    #[test]
    fn decrypt_message_returns_decryption_failed_for_garbage() {
        let bob = test_service();
        let result = bob.decrypt_message("unknown-sender", 1, &[0u8; 64], "signal");
        assert!(matches!(result, Err(DecryptError::DecryptionFailed { .. })));
    }

    #[test]
    fn decrypt_message_returns_error_for_unknown_message_type() {
        let bob = test_service();
        let result = bob.decrypt_message("sender", 1, &[0u8; 32], "invalid_type");
        assert!(matches!(result, Err(DecryptError::Other(_))));
    }

    #[test]
    fn decrypt_message_returns_session_corrupted_after_corruption() {
        let (alice, bob) = setup_alice_bob();

        // Alice encrypts a first message to Bob
        let devices = vec![DeviceInfo {
            user_id: "bob-user-id".into(),
            device_id: 1,
        }];
        let first = alice
            .encrypt_for_channel(&devices, b"first message")
            .unwrap();

        // Bob decrypts it (establishes session on Bob's side)
        bob.decrypt_message(
            "alice-user-id",
            1,
            &first[0].ciphertext,
            &first[0].message_type,
        )
        .unwrap();

        // Corrupt Bob's session data
        {
            let conn = bob.lock_crypto().unwrap();
            conn.execute(
                "UPDATE crypto_sessions SET session_data = X'DEADBEEF' WHERE address = 'alice-user-id'",
                [],
            )
            .unwrap();
        }

        // Alice sends another message
        let second = alice
            .encrypt_for_channel(&devices, b"second message")
            .unwrap();

        // Bob tries to decrypt with corrupted session
        let result = bob.decrypt_message(
            "alice-user-id",
            1,
            &second[0].ciphertext,
            &second[0].message_type,
        );
        assert!(matches!(result, Err(DecryptError::SessionCorrupted { .. })));
    }

    // --- encrypt_for_dm ---

    #[test]
    fn encrypt_for_dm_encrypts_for_all_devices_of_one_user() {
        let (alice, _bob) = setup_alice_bob();

        let devices = vec![DeviceInfo {
            user_id: "bob-user-id".into(),
            device_id: 1,
        }];
        let payloads = alice.encrypt_for_dm(&devices, b"dm message").unwrap();
        assert_eq!(payloads.len(), 1);
        assert!(!payloads[0].ciphertext.is_empty());
    }

    // --- file encryption ---

    #[test]
    fn encrypt_file_returns_blob_and_key() {
        let svc = test_service();
        let (blob, key) = svc.encrypt_file(b"file contents").unwrap();
        assert!(!blob.is_empty());
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn decrypt_file_roundtrip() {
        let svc = test_service();
        let original = b"file contents for roundtrip";
        let (blob, key) = svc.encrypt_file(original).unwrap();
        let decrypted = svc.decrypt_file(&blob, &key).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn encrypt_key_for_recipients_returns_per_device_payloads() {
        let (alice, _bob) = setup_alice_bob();

        let file_key = [0x42u8; 32];
        let devices = vec![DeviceInfo {
            user_id: "bob-user-id".into(),
            device_id: 1,
        }];
        let payloads = alice
            .encrypt_key_for_recipients(&file_key, &devices)
            .unwrap();
        assert_eq!(payloads.len(), 1);
        assert!(!payloads[0].ciphertext.is_empty());
        assert_eq!(payloads[0].user_id, "bob-user-id");
    }

    // --- spawn_blocking safety ---

    #[tokio::test]
    async fn crypto_operations_safe_inside_spawn_blocking() {
        let alice = test_service();
        let bob = test_service();

        let bob_bundle = generate_bundle(&bob, "bob-user-id");

        let payloads = tokio::task::spawn_blocking(move || {
            alice
                .ensure_session("bob-user-id", 1, Some(&bob_bundle))
                .unwrap();

            let devices = vec![DeviceInfo {
                user_id: "bob-user-id".into(),
                device_id: 1,
            }];
            alice
                .encrypt_for_channel(&devices, b"spawn_blocking test")
                .unwrap()
        })
        .await
        .unwrap();

        assert!(!payloads.is_empty());
        assert!(!payloads[0].ciphertext.is_empty());
    }
}
