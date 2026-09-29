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

/// What processing a relayed frame produced.
#[derive(Debug, uniffi::Enum)]
pub enum ClientEvent {
    /// A decrypted application message.
    Message {
        /// The plaintext.
        text: String,
    },
    /// A staged add won its epoch; send this `Welcome` to the new member.
    Admitted {
        /// Deliver to the member being added.
        welcome: Vec<u8>,
        /// Publish it to this address: the new member's mailbox.
        mailbox: Vec<u8>,
    },
    /// A staged add lost its epoch to another member's commit, which has been
    /// applied instead.
    AddSuperseded,
    /// Group state moved forward; nothing to display.
    Advanced,
    /// This client's own frame, echoed back by the relay.
    Echo,
    /// This client's own message landed behind a commit and every receiver
    /// discarded it. Send `message` as an application frame in its place.
    Resend {
        /// The same text, encrypted in the current epoch.
        message: Vec<u8>,
    },
    /// Another member's message from an epoch already left. Its sender
    /// resends it, so there is nothing to display and nothing lost.
    Stale,
}

impl From<crate::Event> for ClientEvent {
    fn from(e: crate::Event) -> Self {
        match e {
            crate::Event::Message(text) => Self::Message { text },
            crate::Event::Admitted { welcome, mailbox } => Self::Admitted { welcome, mailbox },
            crate::Event::AddSuperseded => Self::AddSuperseded,
            crate::Event::Advanced => Self::Advanced,
            crate::Event::Echo => Self::Echo,
            crate::Event::Resend { message } => Self::Resend { message },
            crate::Event::Stale => Self::Stale,
        }
    }
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

    /// The address a `Welcome` for `key_package` will be sent to. Subscribe
    /// to it before publishing the `KeyPackage`.
    ///
    /// # Errors
    /// If `key_package` is not a valid `KeyPackage`.
    pub fn mailbox(&self, key_package: Vec<u8>) -> Result<Vec<u8>> {
        Ok(self.lock().mailbox(&key_package)?)
    }

    /// The channel this client's group lives on, once it is in one.
    #[must_use]
    pub fn channel(&self) -> Option<Vec<u8>> {
        self.lock().channel()
    }

    /// Stage admitting a member from their published `KeyPackage`, returning
    /// the commit to send.
    ///
    /// The add is not applied until the relay's ordering confirms it; watch
    /// for [`ClientEvent::Admitted`] or [`ClientEvent::AddSuperseded`].
    ///
    /// # Errors
    /// If the key package is invalid, this client is not in a group, or an
    /// add is already in flight.
    pub fn propose_add(&self, key_package: Vec<u8>) -> Result<Vec<u8>> {
        Ok(self.lock().propose_add(&key_package)?)
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

    /// Process an incoming frame.
    ///
    /// # Errors
    /// If the frame is malformed, or cannot be decrypted by this client.
    pub fn receive(&self, wire: Vec<u8>) -> Result<ClientEvent> {
        Ok(self.lock().receive(&wire)?.into())
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

/// A request publishing `kind` and `body` to everyone subscribed to `to`.
///
/// # Errors
/// If `to` is not a valid address.
#[uniffi::export]
pub fn encode_publish(to: Vec<u8>, kind: FrameKind, body: Vec<u8>) -> Result<Vec<u8>> {
    Ok(crate::encode_publish(&to, kind, &body)?)
}

/// A request to receive whatever is published to `addresses`.
///
/// # Errors
/// If any address is invalid, or there are more than 255.
#[uniffi::export]
pub fn encode_subscribe(addresses: Vec<Vec<u8>>) -> Result<Vec<u8>> {
    Ok(crate::encode_subscribe(&addresses)?)
}

/// Split a delivery from the relay into its parts.
///
/// # Errors
/// If the envelope is malformed, or the frame inside it is.
#[uniffi::export]
pub fn decode_delivery(wire: Vec<u8>) -> Result<DecodedFrame> {
    let env = crate::decode_delivery(&wire)?;
    Ok(DecodedFrame {
        address: env.address,
        seq: env.seq,
        accepted_ms: env.accepted_ms,
        kind: env.kind,
        body: env.body,
    })
}

/// A channel id as shareable text.
#[must_use]
#[uniffi::export]
pub fn format_channel_id(channel: Vec<u8>) -> String {
    crate::format_channel_id(&channel)
}

/// Parse a channel id from shared text.
///
/// # Errors
/// If the text is not a channel id.
#[uniffi::export]
pub fn parse_channel_id(text: String) -> Result<Vec<u8>> {
    Ok(crate::parse_channel_id(&text)?)
}

/// A delivery split into its parts.
#[derive(Debug, uniffi::Record)]
pub struct DecodedFrame {
    /// Where it was published: a channel, or this client's mailbox.
    pub address: Vec<u8>,
    /// The relay's position for this frame on that address.
    pub seq: u64,
    /// When the relay accepted it, in milliseconds since the Unix epoch.
    pub accepted_ms: u64,
    /// What the frame carries.
    pub kind: FrameKind,
    /// The payload behind the tag.
    pub body: Vec<u8>,
}
