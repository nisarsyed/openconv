//! Safety number (fingerprint) generation for out-of-band identity verification.
//!
//! Users compare numeric fingerprints visually or scan QR codes to verify they
//! are communicating with the intended party and not a man-in-the-middle.

use crate::error::CryptoError;
use libsignal_protocol::{
    Fingerprint as LibsignalFingerprint, IdentityKey, ScannableFingerprint,
};

/// Fingerprint version used by Signal protocol.
const FINGERPRINT_VERSION: u32 = 2;

/// Number of hash iterations for fingerprint generation (Signal standard).
const FINGERPRINT_ITERATIONS: u32 = 5200;

/// A safety number fingerprint containing both human-readable and machine-scannable
/// representations for out-of-band identity verification.
#[derive(Debug, Clone)]
pub struct Fingerprint {
    /// Human-readable numeric fingerprint: 60 digits formatted as 12 groups of 5,
    /// separated by spaces (e.g., "12345 67890 12345 67890 ...").
    pub display: String,

    /// Protobuf-serialized scannable fingerprint bytes, suitable for encoding
    /// as a QR code. The other party scans this and calls `compare_fingerprints`.
    pub scannable: Vec<u8>,
}

/// Generate a safety number fingerprint for verifying identity between two parties.
///
/// Both parties generate their own fingerprint using the same two identity keys
/// (just with local/remote swapped). The resulting display string is symmetric --
/// both parties see the same 60-digit number.
pub fn generate_fingerprint(
    local_identity: &[u8],
    local_address: &str,
    remote_identity: &[u8],
    remote_address: &str,
) -> Result<Fingerprint, CryptoError> {
    let local_key = IdentityKey::decode(local_identity)
        .map_err(|e| CryptoError::InvalidKey(format!("local identity key: {e}")))?;
    let remote_key = IdentityKey::decode(remote_identity)
        .map_err(|e| CryptoError::InvalidKey(format!("remote identity key: {e}")))?;

    let libsignal_fp = LibsignalFingerprint::new(
        FINGERPRINT_VERSION,
        FINGERPRINT_ITERATIONS,
        local_address.as_bytes(),
        &local_key,
        remote_address.as_bytes(),
        &remote_key,
    )
    .map_err(|e| CryptoError::FingerprintError(e.to_string()))?;

    let raw = libsignal_fp
        .display_string()
        .map_err(|e| CryptoError::FingerprintError(e.to_string()))?;

    let formatted = raw
        .as_bytes()
        .chunks(5)
        .map(|chunk| std::str::from_utf8(chunk).expect("display_string output is always ASCII digits"))
        .collect::<Vec<_>>()
        .join(" ");

    let scannable = libsignal_fp
        .scannable
        .serialize()
        .map_err(|e| CryptoError::FingerprintError(e.to_string()))?;

    Ok(Fingerprint {
        display: formatted,
        scannable,
    })
}

/// Compare a locally-generated fingerprint against scanned QR code data from
/// the other party.
///
/// Returns `true` if the fingerprints match (identities are verified), `false` otherwise.
pub fn compare_fingerprints(
    local_fingerprint: &Fingerprint,
    scanned_data: &[u8],
) -> Result<bool, CryptoError> {
    let local_scannable = ScannableFingerprint::deserialize(&local_fingerprint.scannable)
        .map_err(|e| CryptoError::FingerprintError(e.to_string()))?;

    local_scannable
        .compare(scanned_data)
        .map_err(|e| CryptoError::FingerprintError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use libsignal_protocol::IdentityKeyPair;

    fn generate_test_identity() -> IdentityKeyPair {
        IdentityKeyPair::generate(&mut rand::rng())
    }

    #[test]
    fn generate_fingerprint_returns_60_digit_numeric_display_string() {
        let alice = generate_test_identity();
        let bob = generate_test_identity();

        let fp = generate_fingerprint(
            &alice.identity_key().serialize(),
            "alice-uuid",
            &bob.identity_key().serialize(),
            "bob-uuid",
        )
        .unwrap();

        let digits_only: String = fp.display.chars().filter(|c| c.is_ascii_digit()).collect();
        assert_eq!(digits_only.len(), 60);
    }

    #[test]
    fn generate_fingerprint_display_is_formatted_as_12_groups_of_5_digits() {
        let alice = generate_test_identity();
        let bob = generate_test_identity();

        let fp = generate_fingerprint(
            &alice.identity_key().serialize(),
            "alice-uuid",
            &bob.identity_key().serialize(),
            "bob-uuid",
        )
        .unwrap();

        let groups: Vec<&str> = fp.display.split(' ').collect();
        assert_eq!(groups.len(), 12);
        for group in &groups {
            assert_eq!(group.len(), 5);
            assert!(group.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn generate_fingerprint_returns_non_empty_scannable_bytes() {
        let alice = generate_test_identity();
        let bob = generate_test_identity();

        let fp = generate_fingerprint(
            &alice.identity_key().serialize(),
            "alice-uuid",
            &bob.identity_key().serialize(),
            "bob-uuid",
        )
        .unwrap();

        assert!(!fp.scannable.is_empty());
    }

    #[test]
    fn same_inputs_produce_same_fingerprint() {
        let alice = generate_test_identity();
        let bob = generate_test_identity();

        let alice_bytes = alice.identity_key().serialize();
        let bob_bytes = bob.identity_key().serialize();

        let fp1 = generate_fingerprint(&alice_bytes, "alice", &bob_bytes, "bob").unwrap();
        let fp2 = generate_fingerprint(&alice_bytes, "alice", &bob_bytes, "bob").unwrap();

        assert_eq!(fp1.display, fp2.display);
        assert_eq!(fp1.scannable, fp2.scannable);
    }

    #[test]
    fn different_identity_keys_produce_different_fingerprints() {
        let alice = generate_test_identity();
        let bob1 = generate_test_identity();
        let bob2 = generate_test_identity();

        let alice_bytes = alice.identity_key().serialize();

        let fp1 = generate_fingerprint(
            &alice_bytes,
            "alice",
            &bob1.identity_key().serialize(),
            "bob",
        )
        .unwrap();

        let fp2 = generate_fingerprint(
            &alice_bytes,
            "alice",
            &bob2.identity_key().serialize(),
            "bob",
        )
        .unwrap();

        assert_ne!(fp1.display, fp2.display);
    }

    #[test]
    fn fingerprint_is_symmetric() {
        let alice = generate_test_identity();
        let bob = generate_test_identity();

        let alice_bytes = alice.identity_key().serialize();
        let bob_bytes = bob.identity_key().serialize();

        let fp_alice =
            generate_fingerprint(&alice_bytes, "alice", &bob_bytes, "bob").unwrap();
        let fp_bob =
            generate_fingerprint(&bob_bytes, "bob", &alice_bytes, "alice").unwrap();

        assert_eq!(fp_alice.display, fp_bob.display);
    }

    #[test]
    fn compare_fingerprints_returns_true_for_matching_qr_data() {
        let alice = generate_test_identity();
        let bob = generate_test_identity();

        let alice_bytes = alice.identity_key().serialize();
        let bob_bytes = bob.identity_key().serialize();

        let fp_alice =
            generate_fingerprint(&alice_bytes, "alice", &bob_bytes, "bob").unwrap();
        let fp_bob =
            generate_fingerprint(&bob_bytes, "bob", &alice_bytes, "alice").unwrap();

        let result = compare_fingerprints(&fp_alice, &fp_bob.scannable).unwrap();
        assert!(result);
    }

    #[test]
    fn compare_fingerprints_returns_false_for_non_matching_qr_data() {
        let alice = generate_test_identity();
        let bob = generate_test_identity();
        let charlie = generate_test_identity();

        let alice_bytes = alice.identity_key().serialize();
        let bob_bytes = bob.identity_key().serialize();
        let charlie_bytes = charlie.identity_key().serialize();

        let fp_alice_bob =
            generate_fingerprint(&alice_bytes, "alice", &bob_bytes, "bob").unwrap();
        let fp_alice_charlie =
            generate_fingerprint(&alice_bytes, "alice", &charlie_bytes, "charlie").unwrap();

        let result = compare_fingerprints(&fp_alice_bob, &fp_alice_charlie.scannable).unwrap();
        assert!(!result);
    }
}
