//! UniFFI surface consumed by the SwiftUI client.
//!
//! UniFFI objects are shared by [`Arc`] and take `&self`, so the mutable
//! [`Member`] lives behind a mutex here rather than the core API being bent
//! to suit the bridge.

// Every exported function takes its arguments by value because that is what
// UniFFI generates bindings for; borrowing here would not compile.
#![allow(clippy::needless_pass_by_value)]

use crate::{Effect, FrameKind, Member};
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
    /// A staged commit that adds nobody (a removal) won its epoch.
    Committed,
    /// A staged commit lost its epoch to another member's commit, which has
    /// been applied instead.
    Superseded,
    /// Another member's proposal was queued for the next commit.
    Proposed,
    /// A commit removed this client.
    Removed,
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
            crate::Event::Committed => Self::Committed,
            crate::Event::Superseded => Self::Superseded,
            crate::Event::Proposed => Self::Proposed,
            crate::Event::Removed => Self::Removed,
            crate::Event::Advanced => Self::Advanced,
            crate::Event::Echo => Self::Echo,
            crate::Event::Resend { message } => Self::Resend { message },
            crate::Event::Stale => Self::Stale,
        }
    }
}

/// What the app should do after [`Client::deliver`], in order.
#[derive(Debug, uniffi::Enum)]
pub enum ClientEffect {
    /// Write `request` to the relay.
    Send {
        /// Ready to send as a binary WebSocket message.
        request: Vec<u8>,
    },
    /// A decrypted message from another member.
    Message {
        /// The plaintext.
        text: String,
    },
    /// This client joined the group.
    Joined,
    /// This client let a new member in.
    Admitted,
    /// Another member's change to the group landed first.
    Superseded,
    /// This client is no longer in the group: removed, or its leave was
    /// committed.
    Removed,
    /// This client was stranded and has asked to be added back.
    Rejoining,
    /// Someone asked to join; see [`Client::join_requests`].
    JoinRequested,
    /// Group state moved on; membership may have changed.
    Advanced,
    /// Caught up on `address`; live from here.
    Live {
        /// The address.
        address: Vec<u8>,
    },
    /// The relay's history was reset; this client follows it from its start.
    RelayReset {
        /// The address.
        address: Vec<u8>,
    },
    /// The relay no longer holds frames this client missed on `address`, so
    /// it cannot follow the group until another member re-adds it.
    Unavailable {
        /// The address.
        address: Vec<u8>,
        /// The oldest sequence number the relay holds.
        floor: u64,
        /// The next sequence number the relay will assign.
        next: u64,
    },
}

impl From<Effect> for ClientEffect {
    fn from(e: Effect) -> Self {
        match e {
            Effect::Send(request) => Self::Send { request },
            Effect::Message(text) => Self::Message { text },
            Effect::Joined => Self::Joined,
            Effect::Admitted => Self::Admitted,
            Effect::Superseded => Self::Superseded,
            Effect::Removed => Self::Removed,
            Effect::Rejoining => Self::Rejoining,
            Effect::JoinRequested => Self::JoinRequested,
            Effect::Advanced => Self::Advanced,
            Effect::Live { address } => Self::Live { address },
            Effect::RelayReset { address } => Self::RelayReset { address },
            Effect::Unavailable {
                address,
                floor,
                next,
            } => Self::Unavailable {
                address,
                floor,
                next,
            },
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

    /// Requests to send on connecting to the relay: catch up on every
    /// address this client reads, then go live. Send them in order.
    ///
    /// # Errors
    /// If saved state cannot be read.
    pub fn connect(&self) -> Result<Vec<Vec<u8>>> {
        Ok(self.lock().connect()?)
    }

    /// Ask to join the group on `channel`. Send [`Client::connect`]'s
    /// requests afterwards; they carry the offer.
    ///
    /// # Errors
    /// If the offer cannot be made or recorded.
    pub fn offer(&self, channel: Vec<u8>) -> Result<()> {
        Ok(self.lock().offer(&channel)?)
    }

    /// Requests that keep this client's request to join one a member can
    /// still act on. Call every few minutes while waiting; usually empty.
    ///
    /// # Errors
    /// If a new request cannot be made or recorded.
    pub fn refresh_offers(&self) -> Result<Vec<Vec<u8>>> {
        Ok(self.lock().refresh_offers()?)
    }

    /// Whether this client asked to join and is still waiting to be let in.
    ///
    /// # Errors
    /// If saved state cannot be read.
    pub fn waiting_to_join(&self) -> Result<bool> {
        Ok(self.lock().waiting_to_join()?)
    }

    /// Everyone in the group, this client included.
    #[must_use]
    pub fn members(&self) -> Vec<GroupMember> {
        self.lock()
            .members()
            .into_iter()
            .map(|r| GroupMember {
                index: r.index,
                name: r.name,
                me: r.me,
            })
            .collect()
    }

    /// Remove the member at `index`, returning the request to send.
    ///
    /// # Errors
    /// If there is no group, another change is in flight, or `index` is not
    /// another member's.
    pub fn remove_member(&self, index: u32) -> Result<Vec<u8>> {
        let mut member = self.lock();
        let commit = member.remove_member(index)?;
        let channel = member.channel().ok_or(crate::Error::NoGroup)?;
        Ok(crate::encode_publish(&channel, FrameKind::Commit, &commit)?)
    }

    /// Ask to leave the group, returning the request to send.
    ///
    /// # Errors
    /// If there is no group.
    pub fn leave(&self) -> Result<Vec<u8>> {
        let mut member = self.lock();
        let proposal = member.leave()?;
        let channel = member.channel().ok_or(crate::Error::NoGroup)?;
        Ok(crate::encode_publish(
            &channel,
            FrameKind::Proposal,
            &proposal,
        )?)
    }

    /// Requests to join waiting for an answer, oldest first.
    ///
    /// # Errors
    /// If saved state cannot be read.
    pub fn join_requests(&self) -> Result<Vec<PendingJoin>> {
        Ok(self
            .lock()
            .join_requests()?
            .into_iter()
            .map(|r| PendingJoin {
                id: r.id,
                name: r.name,
                seen_at: r.seen_at,
                expired: r.expired,
                removed_before: r.removed_before.map(|p| EarlierRemoval {
                    same_identity: p.same_identity,
                    name: p.name,
                    by: p.by,
                    at: p.at,
                }),
            })
            .collect())
    }

    /// Let in whoever made request `id`: the person, not just this request,
    /// so an expired one is answered when their next request arrives.
    ///
    /// # Errors
    /// If `id` is not a pending request, or the add cannot be made.
    pub fn accept(&self, id: Vec<u8>) -> Result<Accepted> {
        Ok(match self.lock().accept(&id)? {
            crate::Acceptance::Sending(requests) => Accepted::Sending { requests },
            crate::Acceptance::Queued => Accepted::Queued,
            crate::Acceptance::WhenTheyNextAsk => Accepted::WhenTheyNextAsk,
        })
    }

    /// Turn down request `id`, and later ones from the same identity.
    ///
    /// # Errors
    /// If `id` is not a pending request.
    pub fn decline(&self, id: Vec<u8>) -> Result<()> {
        Ok(self.lock().decline(&id)?)
    }

    /// Whether this client has asked to leave and is still a member.
    #[must_use]
    pub fn is_leaving(&self) -> bool {
        self.lock().is_leaving()
    }

    /// Process anything the relay sent, returning what to do about it.
    ///
    /// # Errors
    /// If the message is malformed, frames were missed, or a frame could not
    /// be applied.
    pub fn deliver(&self, wire: Vec<u8>) -> Result<Vec<ClientEffect>> {
        Ok(self
            .lock()
            .deliver(&wire)?
            .into_iter()
            .map(ClientEffect::from)
            .collect())
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

/// A request to join, waiting for an answer.
#[derive(Debug, uniffi::Record)]
pub struct PendingJoin {
    /// What [`Client::accept`] and [`Client::decline`] take.
    pub id: Vec<u8>,
    /// The name it gives: self-declared, so unverified.
    pub name: String,
    /// When this client first saw it, in seconds since the Unix epoch.
    pub seen_at: u64,
    /// Whether the latest request has run out; accepting still works.
    pub expired: bool,
    /// An earlier removal of this identity, or of someone with this name.
    pub removed_before: Option<EarlierRemoval>,
}

/// What accepting a request did.
#[derive(Debug, uniffi::Enum)]
pub enum Accepted {
    /// The add is staged: send these.
    Sending {
        /// Ready to send to the relay.
        requests: Vec<Vec<u8>>,
    },
    /// The add will be made as soon as this client has caught up, or its
    /// change in flight resolves.
    Queued,
    /// Their request has run out; they are let in when their next arrives.
    WhenTheyNextAsk,
}

/// A removal shown beside a request to join.
#[derive(Debug, uniffi::Record)]
pub struct EarlierRemoval {
    /// True if it was this very identity; false if only the name matches.
    pub same_identity: bool,
    /// Who was removed.
    pub name: String,
    /// Who removed them: their own name if they left.
    pub by: String,
    /// When, in seconds since the Unix epoch.
    pub at: u64,
}

/// A member of the group, as the app lists it.
#[derive(Debug, uniffi::Record)]
pub struct GroupMember {
    /// What [`Client::remove_member`] takes.
    pub index: u32,
    /// The display name its credential carries.
    pub name: String,
    /// Whether this is the client asking.
    pub me: bool,
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
