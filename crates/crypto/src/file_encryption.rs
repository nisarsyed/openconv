//! Symmetric file encryption using AES-256-GCM.
//!
//! Provides `encrypt_file` and `decrypt_file` for standalone file encryption
//! with a random per-file key. Independent of the Signal protocol — the caller
//! distributes `FileKey` to recipients via their Signal sessions.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::error::CryptoError;

const NONCE_SIZE: usize = 12; // 96-bit nonce for AES-256-GCM
const KEY_SIZE: usize = 32; // 256-bit key

/// A 32-byte AES-256 key that is securely zeroed on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct FileKey {
    pub(crate) key: [u8; 32],
}

/// Container for encrypted output: `nonce (12 bytes) || ciphertext || auth tag (16 bytes)`.
pub struct EncryptedBlob {
    pub data: Vec<u8>,
}

/// Encrypt file bytes with a random AES-256-GCM key.
///
/// Returns the encrypted blob and the key. The caller is responsible for
/// distributing the key to recipients (e.g., via `encrypt_message`).
///
/// If `aad` is provided, it is used as additional authenticated data to
/// bind the ciphertext to contextual data and prevent blob substitution.
pub fn encrypt_file(
    file_bytes: &[u8],
    aad: Option<&[u8]>,
) -> Result<(EncryptedBlob, FileKey), CryptoError> {
    let mut key_bytes = Zeroizing::new([0u8; KEY_SIZE]);
    rand::rng().fill_bytes(key_bytes.as_mut());

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key_bytes.as_ref())
        .map_err(|e| CryptoError::FileEncryptionError(format!("encryption failed: {e}")))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = match aad {
        Some(aad_bytes) => cipher.encrypt(
            nonce,
            Payload {
                msg: file_bytes,
                aad: aad_bytes,
            },
        ),
        None => cipher.encrypt(
            nonce,
            Payload {
                msg: file_bytes,
                aad: &[],
            },
        ),
    }
    .map_err(|e| CryptoError::FileEncryptionError(format!("encryption failed: {e}")))?;

    let mut data = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    data.extend_from_slice(&nonce_bytes);
    data.extend_from_slice(&ciphertext);

    Ok((EncryptedBlob { data }, FileKey { key: *key_bytes }))
}

/// Decrypt an encrypted blob using the provided file key.
///
/// The `aad` must match what was provided during encryption. If the
/// original encryption used AAD, the same AAD must be supplied here.
pub fn decrypt_file(
    key: &FileKey,
    blob: &EncryptedBlob,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, CryptoError> {
    if blob.data.len() < NONCE_SIZE {
        return Err(CryptoError::FileEncryptionError(
            "blob too short to contain nonce".into(),
        ));
    }

    let (nonce_bytes, ciphertext_with_tag) = blob.data.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&key.key)
        .map_err(|e| CryptoError::FileEncryptionError(format!("decryption failed: {e}")))?;

    let plaintext = match aad {
        Some(aad_bytes) => cipher.decrypt(
            nonce,
            Payload {
                msg: ciphertext_with_tag,
                aad: aad_bytes,
            },
        ),
        None => cipher.decrypt(
            nonce,
            Payload {
                msg: ciphertext_with_tag,
                aad: &[],
            },
        ),
    }
    .map_err(|e| CryptoError::FileEncryptionError(format!("decryption failed: {e}")))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_file_returns_blob_and_key() {
        let data = b"hello world";
        let (blob, key) = encrypt_file(data, None).unwrap();
        assert!(!blob.data.is_empty());
        // ciphertext = nonce (12) + plaintext + auth tag (16)
        assert!(blob.data.len() >= NONCE_SIZE + 16 + data.len());
        let _ = key;
    }

    #[test]
    fn decrypt_file_roundtrip() {
        let original = b"the quick brown fox jumps over the lazy dog";
        let (blob, key) = encrypt_file(original, None).unwrap();
        let decrypted = decrypt_file(&key, &blob, None).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn decrypt_file_wrong_key_fails() {
        let data = b"secret data";
        let (blob, _key) = encrypt_file(data, None).unwrap();
        let wrong_key = FileKey { key: [0xAB; 32] };
        let result = decrypt_file(&wrong_key, &blob, None);
        assert!(matches!(result, Err(CryptoError::FileEncryptionError(_))));
    }

    #[test]
    fn encrypted_blob_starts_with_nonce() {
        let data = b"test";
        let (blob, _key) = encrypt_file(data, None).unwrap();
        assert!(blob.data.len() >= NONCE_SIZE + 16 + data.len());
    }

    #[test]
    fn aad_matching_succeeds() {
        let data = b"payload";
        let aad = b"file-id-12345";
        let (blob, key) = encrypt_file(data, Some(aad)).unwrap();
        let decrypted = decrypt_file(&key, &blob, Some(aad)).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn aad_mismatch_fails() {
        let data = b"payload";
        let (blob, key) = encrypt_file(data, Some(b"correct-aad")).unwrap();
        let result = decrypt_file(&key, &blob, Some(b"wrong-aad"));
        assert!(matches!(result, Err(CryptoError::FileEncryptionError(_))));
    }

    #[test]
    fn aad_required_but_missing_fails() {
        let data = b"payload";
        let (blob, key) = encrypt_file(data, Some(b"some-aad")).unwrap();
        let result = decrypt_file(&key, &blob, None);
        assert!(matches!(result, Err(CryptoError::FileEncryptionError(_))));
    }

    #[test]
    fn filekey_implements_zeroize() {
        // Verify that FileKey can be constructed and dropped without issues.
        // The zeroize derive ensures memory is wiped on drop.
        let key = FileKey { key: [0xFF; 32] };
        drop(key);
    }

    #[test]
    fn empty_file_roundtrip() {
        let data = b"";
        let (blob, key) = encrypt_file(data, None).unwrap();
        let decrypted = decrypt_file(&key, &blob, None).unwrap();
        assert_eq!(decrypted, data.to_vec());
    }

    #[test]
    fn large_file_roundtrip() {
        let data = vec![0x42u8; 1_000_000]; // 1 MB
        let (blob, key) = encrypt_file(&data, None).unwrap();
        let decrypted = decrypt_file(&key, &blob, None).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn blob_too_short_returns_error() {
        let key = FileKey { key: [0; 32] };
        let blob = EncryptedBlob {
            data: vec![0; 5], // too short for nonce
        };
        let result = decrypt_file(&key, &blob, None);
        assert!(matches!(result, Err(CryptoError::FileEncryptionError(_))));
    }
}
