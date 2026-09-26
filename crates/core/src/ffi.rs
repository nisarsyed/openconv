//! UniFFI surface consumed by the SwiftUI client.
//!
//! UniFFI objects are shared by `Arc` and take `&self`, so the mutable
//! [`Member`] lives behind a mutex here rather than the core API being bent
//! to suit the bridge.

use crate::{FrameKind, Member};
use std::sync::{Arc, Mutex};

/// Flattened error: the core error type holds openmls and codec errors that
/// have no meaning across the FFI boundary.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ClientError {
    #[error("{message}")]
    Failed { message: String },
}

impl From<crate::Error> for ClientError {
    fn from(e: crate::Error) -> Self {
        Self::Failed { message: e.to_string() }
    }
}

type Result<T> = std::result::Result<T, ClientError>;

/// The two messages produced by admitting a member.
#[derive(uniffi::Record)]
pub struct Invite {
    /// Deliver to the new member.
    pub welcome: Vec<u8>,
    /// Fan out to members who were already present.
    pub commit: Vec<u8>,
}

/// A chat client bound to one identity.
#[derive(uniffi::Object)]
pub struct Client {
    inner: Mutex<Member>,
}

#[uniffi::export]
impl Client {
    #[uniffi::constructor]
    pub fn new(identity: String) -> Result<Arc<Self>> {
        let member = Member::new(&identity)?;
        Ok(Arc::new(Self { inner: Mutex::new(member) }))
    }

    /// Publishable KeyPackage; safe to hand to the relay.
    pub fn key_package(&self) -> Result<Vec<u8>> {
        Ok(self.lock().key_package()?)
    }

    /// Start a new group containing only this client.
    pub fn create_group(&self) -> Result<()> {
        Ok(self.lock().create_group()?)
    }

    /// Admit a member from their published KeyPackage.
    pub fn add_member(&self, key_package: Vec<u8>) -> Result<Invite> {
        let invite = self.lock().add_member(&key_package)?;
        Ok(Invite { welcome: invite.welcome, commit: invite.commit })
    }

    /// Join a group from a Welcome.
    pub fn join(&self, welcome: Vec<u8>) -> Result<()> {
        Ok(self.lock().join(&welcome)?)
    }

    /// Encrypt a message for the group.
    pub fn send(&self, text: String) -> Result<Vec<u8>> {
        Ok(self.lock().send(&text)?)
    }

    /// Process an incoming frame. `None` means handshake traffic that was
    /// applied to group state rather than a message to display.
    pub fn receive(&self, wire: Vec<u8>) -> Result<Option<String>> {
        Ok(self.lock().receive(&wire)?)
    }

    pub fn identity(&self) -> String {
        self.lock().identity()
    }

    pub fn member_count(&self) -> u32 {
        self.lock().member_count() as u32
    }
}

impl Client {
    /// A poisoned mutex means a previous call panicked mid-mutation; the
    /// group state can no longer be trusted, so fail loudly.
    fn lock(&self) -> std::sync::MutexGuard<'_, Member> {
        self.inner.lock().expect("group state poisoned by an earlier panic")
    }
}

/// Prefix a payload with its frame tag.
#[uniffi::export]
pub fn encode_frame(kind: FrameKind, body: Vec<u8>) -> Vec<u8> {
    crate::encode_frame(kind, &body)
}

/// Split a relayed frame into its kind and payload.
#[uniffi::export]
pub fn decode_frame(wire: Vec<u8>) -> Result<DecodedFrame> {
    let (kind, body) = crate::decode_frame(&wire)?;
    Ok(DecodedFrame { kind, body })
}

/// A frame split into its parts.
#[derive(uniffi::Record)]
pub struct DecodedFrame {
    pub kind: FrameKind,
    pub body: Vec<u8>,
}
