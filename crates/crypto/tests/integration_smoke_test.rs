//! Full roundtrip integration smoke test for openconv-crypto.
//!
//! Exercises the complete Signal protocol flow between two parties:
//! identity generation, pre-key exchange, session creation, message
//! encryption/decryption, bidirectional communication, and state verification.

use libsignal_protocol::{DeviceId, ProtocolAddress};
use openconv_crypto::error::CryptoError;
use openconv_crypto::file_encryption;
use openconv_crypto::fingerprint;
use openconv_crypto::identity;
use openconv_crypto::message::{self, MessageType};
use openconv_crypto::prekeys;
use openconv_crypto::session;
use openconv_crypto::storage::migrations::run_crypto_migrations;

/// Create an in-memory SQLCipher database with migrations applied.
fn init_test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
    )
    .unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn.pragma_update(None, "busy_timeout", 5000).unwrap();
    run_crypto_migrations(&conn).unwrap();
    conn
}

#[test]
fn full_roundtrip_alice_bob() {
    // -- Setup: two independent databases --
    let alice_conn = init_test_db();
    let bob_conn = init_test_db();

    // -- Step 1: Generate identities --
    let alice_identity = identity::generate_identity(&alice_conn).unwrap();
    let _bob_identity = identity::generate_identity(&bob_conn).unwrap();

    // Verify we can retrieve them
    assert_eq!(
        identity::get_identity(&alice_conn).unwrap().serialize(),
        alice_identity.serialize()
    );

    // -- Step 2: Bob creates pre-key bundle --
    let bob_bundle = prekeys::generate_pre_key_bundle(&bob_conn, "bob-uuid").unwrap();
    // One-time pre-keys are generated for server upload but not included in the
    // session bundle (PQXDH uses Kyber last-resort key instead).
    let _bob_otpks = prekeys::generate_one_time_pre_keys(&bob_conn, 10).unwrap();

    // -- Step 3: Alice creates outgoing session to Bob --
    let bundle_json = serde_json::to_vec(&bob_bundle).unwrap();
    let bob_address = session::create_outgoing_session(&alice_conn, &bundle_json).unwrap();
    let alice_address = ProtocolAddress::new(
        "alice-uuid".to_string(),
        DeviceId::new(1).expect("valid"),
    );

    // -- Step 4: Alice encrypts "hello" --
    let encrypted = message::encrypt_message(&alice_conn, &bob_address, b"hello").unwrap();
    assert_eq!(encrypted.message_type, MessageType::PreKey);
    assert!(!encrypted.ciphertext.is_empty());

    // -- Step 5: Bob decrypts Alice's message --
    let plaintext = message::decrypt_message(
        &bob_conn,
        &alice_address,
        &encrypted.ciphertext,
        encrypted.message_type,
    )
    .unwrap();
    assert_eq!(plaintext, b"hello");

    // -- Step 6: Bob replies "hi back" --
    let reply = message::encrypt_message(&bob_conn, &alice_address, b"hi back").unwrap();
    // Bob's session was established as responder — his messages are Signal type
    assert_eq!(reply.message_type, MessageType::Signal);

    // -- Step 7: Alice decrypts Bob's reply --
    let reply_plain = message::decrypt_message(
        &alice_conn,
        &bob_address,
        &reply.ciphertext,
        reply.message_type,
    )
    .unwrap();
    assert_eq!(reply_plain, b"hi back");

    // -- Step 8: Verify session state --
    let alice_session_count: u32 = alice_conn
        .query_row(
            "SELECT COUNT(*) FROM crypto_sessions WHERE address = ?1",
            [bob_address.name()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(alice_session_count, 1);

    let bob_session_count: u32 = bob_conn
        .query_row(
            "SELECT COUNT(*) FROM crypto_sessions WHERE address = ?1",
            [alice_address.name()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bob_session_count, 1);

    // -- Step 9: Verify continued messaging transitions to Signal type --
    // After round-trip, Alice's messages should be Signal type
    let msg2 = message::encrypt_message(&alice_conn, &bob_address, b"second message").unwrap();
    assert_eq!(msg2.message_type, MessageType::Signal);

    let plain2 = message::decrypt_message(
        &bob_conn,
        &alice_address,
        &msg2.ciphertext,
        msg2.message_type,
    )
    .unwrap();
    assert_eq!(plain2, b"second message");

    // -- Step 10: Multiple sequential messages --
    for i in 0..5 {
        let payload = format!("message {i}");
        let enc = message::encrypt_message(&alice_conn, &bob_address, payload.as_bytes()).unwrap();
        assert_eq!(enc.message_type, MessageType::Signal);

        let dec = message::decrypt_message(
            &bob_conn,
            &alice_address,
            &enc.ciphertext,
            enc.message_type,
        )
        .unwrap();
        assert_eq!(dec, payload.as_bytes());
    }
}

#[test]
fn file_encryption_roundtrip() {
    let plaintext = b"confidential document contents";
    let aad = b"file-id-12345";

    let (blob, key) = file_encryption::encrypt_file(plaintext, Some(aad)).unwrap();
    let decrypted = file_encryption::decrypt_file(&key, &blob, Some(aad)).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn fingerprint_generation_and_comparison() {
    let alice_pair = libsignal_protocol::IdentityKeyPair::generate(&mut rand::rng());
    let bob_pair = libsignal_protocol::IdentityKeyPair::generate(&mut rand::rng());

    let alice_key = alice_pair.identity_key().serialize();
    let bob_key = bob_pair.identity_key().serialize();

    let fp_alice = fingerprint::generate_fingerprint(
        &alice_key,
        "alice-uuid",
        &bob_key,
        "bob-uuid",
    )
    .unwrap();

    let fp_bob = fingerprint::generate_fingerprint(
        &bob_key,
        "bob-uuid",
        &alice_key,
        "alice-uuid",
    )
    .unwrap();

    // Symmetric display
    assert_eq!(fp_alice.display, fp_bob.display);

    // QR comparison succeeds
    assert!(fingerprint::compare_fingerprints(&fp_alice, &fp_bob.scannable).unwrap());
    assert!(fingerprint::compare_fingerprints(&fp_bob, &fp_alice.scannable).unwrap());
}

#[test]
fn identity_not_initialized_error() {
    let conn = init_test_db();

    let result = identity::get_identity(&conn);
    assert!(matches!(result, Err(CryptoError::IdentityNotInitialized)));
}

#[test]
fn public_key_string_export() {
    let conn = init_test_db();
    identity::generate_identity(&conn).unwrap();

    let pubkey_str = identity::get_public_key_string(&conn).unwrap();
    // Should be valid base64
    assert!(!pubkey_str.is_empty());
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &pubkey_str).unwrap();
}
