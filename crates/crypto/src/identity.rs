//! Identity keypair management for openconv-crypto.
//!
//! Generates Curve25519 identity keypairs via libsignal, persists them in
//! encrypted SQLite, and provides public key export and challenge signing
//! for the registration/login flows.

use base64::Engine;
use libsignal_protocol::IdentityKeyPair;
use rand::Rng;
use rusqlite::Connection;

use crate::error::CryptoError;
use crate::storage::{with_transaction, CryptoStore};

/// Generate a new Curve25519 identity keypair and store it in the database.
///
/// Also generates and stores a registration ID in the 14-bit range (1..=16380)
/// as required by the libsignal protocol specification.
///
/// Returns an error if an identity already exists (to prevent accidental
/// overwrite, which would break existing sessions).
pub fn generate_identity(conn: &Connection) -> Result<IdentityKeyPair, CryptoError> {
    let keypair = IdentityKeyPair::generate(&mut rand::rng());
    let reg_id: u32 = rand::rng().random_range(1..=16380);

    with_transaction(conn, |store| {
        // Check if identity already exists
        match store.get_identity_keypair() {
            Ok(_) => return Err(CryptoError::StorageError("identity already exists".into())),
            Err(CryptoError::IdentityNotInitialized) => {} // expected, proceed
            Err(e) => return Err(e),
        }

        store.store_identity_keypair(
            keypair.public_key().serialize().as_ref(),
            &keypair.private_key().serialize(),
        )?;

        store.store_config("registration_id", &reg_id.to_be_bytes())?;

        Ok(())
    })?;

    Ok(keypair)
}

/// Retrieve the local identity keypair from the database.
///
/// Returns `CryptoError::IdentityNotInitialized` if no identity has been
/// generated yet.
pub fn get_identity(conn: &Connection) -> Result<IdentityKeyPair, CryptoError> {
    let store = CryptoStore::new(conn);
    let (pub_bytes, priv_bytes) = store.get_identity_keypair()?;

    let public = libsignal_protocol::IdentityKey::decode(&pub_bytes)?;
    let private = libsignal_protocol::PrivateKey::deserialize(&priv_bytes)
        .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;
    Ok(IdentityKeyPair::new(public, private))
}

/// Get the local identity public key as a base64-encoded string.
///
/// This is the value used for `RegisterRequest.public_key` and
/// `LoginChallengeRequest.public_key` in the shared API types.
pub fn get_public_key_string(conn: &Connection) -> Result<String, CryptoError> {
    let keypair = get_identity(conn)?;
    let pub_bytes = keypair.public_key().serialize();
    Ok(base64::engine::general_purpose::STANDARD.encode(pub_bytes))
}

/// Sign a challenge byte slice using the local identity private key.
///
/// Used for the login challenge-response flow. The caller base64-encodes
/// the returned signature bytes for `LoginVerifyRequest.signature`.
pub fn sign_challenge(conn: &Connection, challenge: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let keypair = get_identity(conn)?;
    let signature = keypair
        .private_key()
        .calculate_signature(challenge, &mut rand::rng())
        .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;
    Ok(Vec::from(signature.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::init_test_db;

    #[test]
    fn generate_identity_creates_keypair_and_stores_in_db() {
        let conn = init_test_db();
        let keypair = generate_identity(&conn).unwrap();

        // Verify keypair public key is 33 bytes (Curve25519 compressed point)
        assert_eq!(keypair.public_key().serialize().len(), 33);

        // Verify it's stored in the database
        let row_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_identity_keys WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(row_count, 1);
    }

    #[test]
    fn generate_identity_stores_registration_id_in_14_bit_range() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let store = CryptoStore::new(&conn);
        let reg_bytes = store.get_config("registration_id").unwrap().unwrap();
        let reg_id = u32::from_be_bytes([reg_bytes[0], reg_bytes[1], reg_bytes[2], reg_bytes[3]]);
        assert!(reg_id >= 1 && reg_id <= 16380);
    }

    #[test]
    fn generate_identity_called_twice_returns_error() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let result = generate_identity(&conn);
        assert!(result.is_err());
    }

    #[test]
    fn get_identity_returns_stored_keypair() {
        let conn = init_test_db();
        let original = generate_identity(&conn).unwrap();

        let loaded = get_identity(&conn).unwrap();
        assert_eq!(
            original.public_key().serialize(),
            loaded.public_key().serialize()
        );
        assert_eq!(
            original.private_key().serialize(),
            loaded.private_key().serialize()
        );
    }

    #[test]
    fn get_identity_returns_identity_not_initialized_on_empty_db() {
        let conn = init_test_db();
        let result = get_identity(&conn);
        assert!(matches!(result, Err(CryptoError::IdentityNotInitialized)));
    }

    #[test]
    fn get_public_key_string_returns_valid_base64() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let b64 = get_public_key_string(&conn).unwrap();
        let decoded = base64::engine::general_purpose::STANDARD.decode(&b64);
        assert!(decoded.is_ok());
    }

    #[test]
    fn get_public_key_string_round_trips_with_raw_bytes() {
        let conn = init_test_db();
        let keypair = generate_identity(&conn).unwrap();

        let b64 = get_public_key_string(&conn).unwrap();
        let decoded = base64::engine::general_purpose::STANDARD.decode(&b64).unwrap();
        assert_eq!(decoded.as_slice(), keypair.public_key().serialize().as_ref());
    }

    #[test]
    fn sign_challenge_returns_verifiable_signature() {
        let conn = init_test_db();
        let keypair = generate_identity(&conn).unwrap();

        let challenge = b"test challenge bytes";
        let signature = sign_challenge(&conn, challenge).unwrap();

        let valid = keypair
            .public_key()
            .verify_signature(challenge, &signature);
        assert!(valid);
    }

    #[test]
    fn sign_challenge_with_different_challenges_produces_different_signatures() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let sig1 = sign_challenge(&conn, b"challenge one").unwrap();
        let sig2 = sign_challenge(&conn, b"challenge two").unwrap();
        assert_ne!(sig1, sig2);
    }
}
