use base64::Engine;
use openconv_shared::error::OpenConvError;

/// Parse a base64-encoded public key string into a libsignal PublicKey.
/// Expects 33 bytes after base64 decoding (Curve25519 compressed point).
pub fn parse_public_key(base64_key: &str) -> Result<libsignal_protocol::PublicKey, OpenConvError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_key)
        .map_err(|e| OpenConvError::Validation(format!("invalid base64 public key: {e}")))?;

    if bytes.len() != 33 {
        return Err(OpenConvError::Validation(format!(
            "expected 33-byte public key, got {} bytes",
            bytes.len()
        )));
    }

    libsignal_protocol::PublicKey::deserialize(&bytes)
        .map_err(|e| OpenConvError::Validation(format!("invalid public key: {e}")))
}

/// Verify a signature against a challenge using a libsignal PublicKey.
/// The signature was produced by `private_key.calculate_signature()` on the client.
pub fn verify_challenge_signature(
    public_key: &libsignal_protocol::PublicKey,
    challenge: &[u8],
    signature: &[u8],
) -> bool {
    public_key.verify_signature(challenge, signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libsignal_protocol::IdentityKeyPair;

    #[test]
    fn parse_33_byte_curve25519_public_key() {
        let keypair = IdentityKeyPair::generate(&mut rand::rng());
        let bytes = keypair.public_key().serialize();
        assert_eq!(bytes.len(), 33);
        let parsed = libsignal_protocol::PublicKey::deserialize(&bytes);
        assert!(parsed.is_ok());
    }

    #[test]
    fn verify_signature_from_crypto_crate() {
        let keypair = IdentityKeyPair::generate(&mut rand::rng());
        let challenge = b"test challenge";
        let signature = keypair
            .private_key()
            .calculate_signature(challenge, &mut rand::rng())
            .unwrap();
        let valid = keypair.public_key().verify_signature(challenge, &signature);
        assert!(valid);
    }

    #[test]
    fn reject_signature_from_different_keypair() {
        let keypair1 = IdentityKeyPair::generate(&mut rand::rng());
        let keypair2 = IdentityKeyPair::generate(&mut rand::rng());
        let challenge = b"test challenge";
        let signature = keypair1
            .private_key()
            .calculate_signature(challenge, &mut rand::rng())
            .unwrap();
        let valid = keypair2
            .public_key()
            .verify_signature(challenge, &signature);
        assert!(!valid);
    }

    #[test]
    fn reject_32_byte_key_input() {
        let bad_bytes = [0u8; 32];
        let result = libsignal_protocol::PublicKey::deserialize(&bad_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn reject_invalid_base64_public_key() {
        let result = parse_public_key("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn parse_public_key_roundtrip() {
        let keypair = IdentityKeyPair::generate(&mut rand::rng());
        let bytes = keypair.public_key().serialize();
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        let parsed = parse_public_key(&b64).unwrap();
        assert_eq!(parsed.serialize(), keypair.public_key().serialize());
    }

    #[test]
    fn verify_challenge_signature_helper() {
        let keypair = IdentityKeyPair::generate(&mut rand::rng());
        let challenge = b"hello world";
        let signature = keypair
            .private_key()
            .calculate_signature(challenge, &mut rand::rng())
            .unwrap();
        assert!(verify_challenge_signature(
            keypair.public_key(),
            challenge,
            &signature
        ));
    }
}
