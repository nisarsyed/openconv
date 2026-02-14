# 02 - Cryptography & Identity

## Overview

Implement the Signal protocol cryptography layer and keypair-based identity system for OpenConv. This is the privacy core of the application — all message content is encrypted client-side before transmission, and the server never has access to plaintext.

This split produces a Rust crate with clean APIs consumed by the auth (03) and messaging (06) splits.

## Original Requirements

See `planning/requirements.md` for the high-level project description.
See `planning/deep_project_interview.md` for full interview context.

## Key Decisions (from interview)

- **E2E Encryption:** Signal protocol (gold standard, battle-tested)
- **Identity model:** Keypair-based — user's public key IS their identity
- **Library:** libsignal-protocol (Rust implementation)
- **Key storage:** Client-side SQLite (encrypted)

## Scope

### Identity Keypair Management
- Identity key pair generation (Curve25519)
- Secure local storage of private keys (encrypted in SQLite using OS keychain or passphrase)
- Public key fingerprint generation (for user verification)
- Key export/import for device migration (stretch goal)

### Signal Protocol Integration
- **X3DH (Extended Triple Diffie-Hellman):**
  - Pre-key bundle generation (signed pre-key + one-time pre-keys)
  - Pre-key bundle upload API types (consumed by server)
  - Initial key agreement when starting a new conversation
  - Pre-key replenishment logic

- **Double Ratchet Algorithm:**
  - Session initialization from X3DH output
  - Message encryption (plaintext → ciphertext + metadata)
  - Message decryption (ciphertext → plaintext)
  - Ratchet state management
  - Out-of-order message handling

### Session Management
- Crypto session creation, storage, and retrieval
- Per-conversation session tracking
- Session reset/recovery mechanisms
- Multi-device considerations (design for future, implement single-device for V1)

### Encryption APIs
- `encrypt_message(session, plaintext) → EncryptedMessage`
- `decrypt_message(session, encrypted) → plaintext`
- `encrypt_file(key, file_bytes) → EncryptedBlob` (symmetric, for file sharing)
- `decrypt_file(key, encrypted_blob) → file_bytes`
- File encryption key generation and sharing (encrypted per-recipient)

### Key Storage (Client SQLite)
- Identity keys table
- Session state table (serialized Double Ratchet state)
- Pre-key storage
- All sensitive data encrypted at rest

## Outputs

A Rust crate (`openconv-crypto`) exposing:
1. Identity keypair generation and management
2. Pre-key bundle creation
3. X3DH key agreement
4. Double Ratchet message encrypt/decrypt
5. File encryption/decryption (symmetric)
6. Session persistence APIs

No UI — this is a library crate consumed by other splits.

## Dependencies

- **Depends on:** 01 (shared types crate, SQLite schema for key storage)
- **Depended on by:** 03 (keypair generation for registration), 06 (encrypt/decrypt for messaging)

## Technical Notes

- libsignal-protocol-rust is the reference implementation; evaluate if it can be used directly or if a wrapper is needed
- Signal protocol requires server-side pre-key bundle storage — define the API contract here, implement storage in 04
- Consider using the OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service) for master key protection
- Group messaging encryption (Sender Keys) can be deferred to post-V1 — use pairwise sessions for channel messages initially (simpler, less efficient)
