//! UniFFI surface consumed by the SwiftUI client.
//!
//! UniFFI objects are shared by [`Arc`] and take `&self`, so the mutable
//! [`Member`] lives behind a mutex here rather than the core API being bent
//! to suit the bridge.

// Every exported function takes its arguments by value because that is what
// UniFFI generates bindings for; borrowing here would not compile.
#![allow(clippy::needless_pass_by_value)]

use crate::{FrameKind, Member};
use std::sync::{Arc, Mutex};

/// Flattened error: the core error type holds openmls and codec errors that
/// have no meaning across the FFI boundary.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ClientError {
    /// Something went wrong; `message` carries the underlying description.
    #[error("{message}")]
    Failed {
        /// Human-readable description of the failure.
        message: String,
    },
}

impl From<crate::Error> for ClientError {
    fn from(e: crate::Error) -> Self {
        Self::Failed {
            message: e.to_string(),
        }
    }
}

type Result<T> = std::result::Result<T, ClientError>;

/// The two messages produced by admitting a member.
#[derive(Debug, uniffi::Record)]
pub struct Invite {
    /// Deliver to the new member.
    pub welcome: Vec<u8>,
    /// Fan out to members who were already present.
    pub commit: Vec<u8>,
}

/// A chat client bound to one identity.
#[derive(Debug, uniffi::Object)]
pub struct Client {
    inner: Mutex<Member>,
}

#[uniffi::export]
impl Client {
    /// In-memory client. Nothing survives the process.
    ///
    /// # Errors
    /// If an identity key pair cannot be generated.
    #[uniffi::constructor]
    pub fn new(identity: String) -> Result<Arc<Self>> {
        let member = Member::new(&identity)?;
        Ok(Arc::new(Self {
            inner: Mutex::new(member),
        }))
    }

    /// Client backed by an encrypted vault at `path`, restoring previous
    /// state if there is any. The data key comes from the macOS Keychain.
    ///
    /// # Errors
    /// If the data key is unavailable, the vault cannot be decrypted, or the
    /// restored state is incomplete.
    #[uniffi::constructor]
    pub fn open(path: String, identity: String) -> Result<Arc<Self>> {
        let member = Member::open(&path, &identity)?;
        Ok(Arc::new(Self {
            inner: Mutex::new(member),
        }))
    }

    /// Publishable `KeyPackage`; safe to hand to the relay.
    ///
    /// # Errors
    /// If the key package cannot be built or serialised.
    pub fn key_package(&self) -> Result<Vec<u8>> {
        Ok(self.lock().key_package()?)
    }

    /// Start a new group containing only this client.
    ///
    /// # Errors
    /// If the group cannot be created or its state cannot be saved.
    pub fn create_group(&self) -> Result<()> {
        Ok(self.lock().create_group()?)
    }

    /// Admit a member from their published `KeyPackage`.
    ///
    /// # Errors
    /// If the key package is invalid, this client is not in a group, or the
    /// resulting state cannot be saved.
    pub fn add_member(&self, key_package: Vec<u8>) -> Result<Invite> {
        let invite = self.lock().add_member(&key_package)?;
        Ok(Invite {
            welcome: invite.welcome,
            commit: invite.commit,
        })
    }

    /// Join a group from a `Welcome`.
    ///
    /// # Errors
    /// If the welcome is malformed or does not admit this client.
    pub fn join(&self, welcome: Vec<u8>) -> Result<()> {
        Ok(self.lock().join(&welcome)?)
    }

    /// Encrypt a message for the group.
    ///
    /// # Errors
    /// If this client is not in a group, or encryption fails.
    pub fn send(&self, text: String) -> Result<Vec<u8>> {
        Ok(self.lock().send(&text)?)
    }

    /// Process an incoming frame. `None` means handshake traffic that was
    /// applied to group state rather than a message to display.
    ///
    /// # Errors
    /// If the frame is malformed, or cannot be decrypted by this client.
    pub fn receive(&self, wire: Vec<u8>) -> Result<Option<String>> {
        Ok(self.lock().receive(&wire)?)
    }

    /// This client's display name.
    #[must_use]
    pub fn identity(&self) -> String {
        self.lock().identity()
    }

    /// How many members are currently in the group.
    #[must_use]
    pub fn member_count(&self) -> u32 {
        // A group cannot plausibly exceed u32::MAX; saturate rather than wrap.
        u32::try_from(self.lock().member_count()).unwrap_or(u32::MAX)
    }
}

impl Client {
    /// A poisoned mutex means a previous call panicked mid-mutation; the
    /// group state can no longer be trusted, so fail loudly.
    fn lock(&self) -> std::sync::MutexGuard<'_, Member> {
        self.inner
            .lock()
            .expect("group state poisoned by an earlier panic")
    }
}

/// Prefix a payload with its frame tag.
#[must_use]
#[uniffi::export]
pub fn encode_frame(kind: FrameKind, body: Vec<u8>) -> Vec<u8> {
    crate::encode_frame(kind, &body)
}

/// Split a relayed frame into its kind and payload.
///
/// # Errors
/// If the frame is empty or carries an unknown tag.
#[uniffi::export]
pub fn decode_frame(wire: Vec<u8>) -> Result<DecodedFrame> {
    let (kind, body) = crate::decode_frame(&wire)?;
    Ok(DecodedFrame { kind, body })
}

/// A frame split into its parts.
#[derive(Debug, uniffi::Record)]
pub struct DecodedFrame {
    /// What the frame carries.
    pub kind: FrameKind,
    /// The payload behind the tag.
    pub body: Vec<u8>,
}
