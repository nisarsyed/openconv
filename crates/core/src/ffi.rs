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
            crate::Event::Message(received) => Self::Message {
                text: received.text(),
            },
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

/// What the app should do after [`Client::deliver`], in order. Each one
/// about a group names its `channel`.
#[derive(Debug, uniffi::Enum)]
pub enum ClientEffect {
    /// Write `request` to the relay.
    Send {
        /// Ready to send as a binary WebSocket message.
        request: Vec<u8>,
    },
    /// A decrypted message from another member.
    Message {
        /// The group it was sent to.
        channel: Vec<u8>,
        /// The text to show.
        text: String,
        /// Who sent it: their self-declared name.
        sender: String,
        /// Its MIMI message id, stable across resends.
        id: Vec<u8>,
    },
    /// This client joined the group.
    Joined {
        /// The group.
        channel: Vec<u8>,
    },
    /// This client let a new member in.
    Admitted {
        /// The group.
        channel: Vec<u8>,
    },
    /// Another member's change to the group landed first.
    Superseded {
        /// The group.
        channel: Vec<u8>,
    },
    /// This client is no longer in the group: removed, or its leave was
    /// committed.
    Removed {
        /// The group.
        channel: Vec<u8>,
    },
    /// This client was stranded and has asked to be added back.
    Rejoining {
        /// The group.
        channel: Vec<u8>,
    },
    /// Someone asked to join; see [`Client::join_requests`].
    JoinRequested {
        /// The group.
        channel: Vec<u8>,
    },
    /// Group state moved on: membership, or the name, may have changed.
    Advanced {
        /// The group.
        channel: Vec<u8>,
    },
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
            Effect::Message { channel, message } => Self::Message {
                channel,
                text: message.text(),
                sender: message.sender.clone(),
                id: message.id.to_vec(),
            },
            Effect::Joined { channel } => Self::Joined { channel },
            Effect::Admitted { channel } => Self::Admitted { channel },
            Effect::Superseded { channel } => Self::Superseded { channel },
            Effect::Removed { channel } => Self::Removed { channel },
            Effect::Rejoining { channel } => Self::Rejoining { channel },
            Effect::JoinRequested { channel } => Self::JoinRequested { channel },
            Effect::Advanced { channel } => Self::Advanced { channel },
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

    /// Start a new group containing only this client, returning its channel.
    ///
    /// # Errors
    /// If the group cannot be created or its state cannot be saved.
    pub fn create_group(&self) -> Result<Vec<u8>> {
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

    /// The channels of the groups this client is in, in the order it joined
    /// them.
    ///
    /// # Errors
    /// If saved state cannot be read.
    pub fn channels(&self) -> Result<Vec<Vec<u8>>> {
        Ok(self.lock().channels()?)
    }

    /// The conversation's name, if a member has given it one. Show its
    /// members' names when it has none.
    ///
    /// # Errors
    /// If this client is not in that group.
    pub fn name(&self, channel: Vec<u8>) -> Result<Option<String>> {
        Ok(self.lock().name(&channel)?)
    }

    /// Name the conversation, or clear its name with an empty `name`,
    /// returning the request to send. The name changes once the relay's
    /// order confirms it: watch for [`ClientEffect::Advanced`] or
    /// [`ClientEffect::Superseded`].
    ///
    /// # Errors
    /// If the name is too long, this client is not in that group, another
    /// change is in flight there, or the group predates names.
    pub fn set_name(&self, channel: Vec<u8>, name: String) -> Result<Vec<u8>> {
        let commit = self.lock().set_name(&channel, &name)?;
        Ok(crate::encode_publish(&channel, FrameKind::Commit, &commit)?)
    }

    /// Stage admitting a member from their published `KeyPackage`, returning
    /// the commit to send.
    ///
    /// The add is not applied until the relay's ordering confirms it; watch
    /// for [`ClientEvent::Admitted`] or [`ClientEvent::Superseded`].
    ///
    /// # Errors
    /// If the key package is invalid, this client is not in that group, or
    /// a change is already in flight there.
    pub fn propose_add(&self, channel: Vec<u8>, key_package: Vec<u8>) -> Result<Vec<u8>> {
        Ok(self.lock().propose_add(&channel, &key_package)?)
    }

    /// Join a group from a `Welcome`, returning its channel.
    ///
    /// # Errors
    /// If the welcome is malformed, does not admit this client, or is to a
    /// group it is already in.
    pub fn join(&self, welcome: Vec<u8>) -> Result<Vec<u8>> {
        Ok(self.lock().join(&welcome)?)
    }

    /// Encrypt a message for the group on `channel`.
    ///
    /// # Errors
    /// If this client is not in that group, or encryption fails.
    pub fn send(&self, channel: Vec<u8>, text: String) -> Result<Vec<u8>> {
        Ok(self.lock().send(&channel, &text)?)
    }

    /// Process an incoming frame for the group on `channel`.
    ///
    /// # Errors
    /// If the frame is malformed, or cannot be decrypted by this client.
    pub fn receive(&self, channel: Vec<u8>, wire: Vec<u8>) -> Result<ClientEvent> {
        Ok(self.lock().receive(&channel, &wire)?.into())
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

    /// The channels this client asked to join and is still waiting to be
    /// let into.
    ///
    /// # Errors
    /// If saved state cannot be read.
    pub fn waiting_to_join(&self) -> Result<Vec<Vec<u8>>> {
        Ok(self.lock().waiting_to_join()?)
    }

    /// Everyone in the group on `channel`, this client included.
    #[must_use]
    pub fn members(&self, channel: Vec<u8>) -> Vec<GroupMember> {
        self.lock()
            .members(&channel)
            .into_iter()
            .map(|r| GroupMember {
                index: r.index,
                name: r.name,
                me: r.me,
            })
            .collect()
    }

    /// Remove the member at `index` from the group on `channel`, returning
    /// the request to send.
    ///
    /// # Errors
    /// If this client is not in that group, another change is in flight
    /// there, or `index` is not another member's.
    pub fn remove_member(&self, channel: Vec<u8>, index: u32) -> Result<Vec<u8>> {
        let commit = self.lock().remove_member(&channel, index)?;
        Ok(crate::encode_publish(&channel, FrameKind::Commit, &commit)?)
    }

    /// Ask to leave the group on `channel`, returning the request to send.
    ///
    /// # Errors
    /// If this client is not in that group.
    pub fn leave(&self, channel: Vec<u8>) -> Result<Vec<u8>> {
        let proposal = self.lock().leave(&channel)?;
        Ok(crate::encode_publish(
            &channel,
            FrameKind::Proposal,
            &proposal,
        )?)
    }

    /// Requests to join the group on `channel` waiting for an answer, oldest
    /// first.
    ///
    /// # Errors
    /// If saved state cannot be read.
    pub fn join_requests(&self, channel: Vec<u8>) -> Result<Vec<PendingJoin>> {
        Ok(self
            .lock()
            .join_requests(&channel)?
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
    /// If `id` is not a request pending in that group, or the add cannot be
    /// made.
    pub fn accept(&self, channel: Vec<u8>, id: Vec<u8>) -> Result<Accepted> {
        Ok(match self.lock().accept(&channel, &id)? {
            crate::Acceptance::Sending(requests) => Accepted::Sending { requests },
            crate::Acceptance::Queued => Accepted::Queued,
            crate::Acceptance::WhenTheyNextAsk => Accepted::WhenTheyNextAsk,
        })
    }

    /// Turn down request `id` to join the group on `channel`, and later ones
    /// there from the same identity.
    ///
    /// # Errors
    /// If `id` is not a request pending in that group.
    pub fn decline(&self, channel: Vec<u8>, id: Vec<u8>) -> Result<()> {
        Ok(self.lock().decline(&channel, &id)?)
    }

    /// Whether this client has asked to leave the group on `channel` and is
    /// still a member.
    #[must_use]
    pub fn is_leaving(&self, channel: Vec<u8>) -> bool {
        self.lock().is_leaving(&channel)
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

    /// The stored transcript of `channel`, oldest first: up to `limit` of
    /// the latest messages, or with `before`, of those just before that
    /// position. Page back by passing the smallest position you have, or
    /// `u64::MAX` if nothing you have has one yet.
    ///
    /// # Errors
    /// If the store cannot be read.
    pub fn history(
        &self,
        channel: Vec<u8>,
        before: Option<u64>,
        limit: u32,
    ) -> Result<Vec<TranscriptMessage>> {
        Ok(self
            .lock()
            .history(&channel, before, limit)?
            .into_iter()
            .map(|e| TranscriptMessage {
                text: e.text(),
                id: e.id.to_vec(),
                position: e.position,
                seq: e.seq,
                accepted_ms: e.accepted_ms,
                stored_ms: e.stored_ms,
                mine: e.mine,
                sender: e.sender,
                sender_uri: e.sender_uri,
                replaces: e.content.replaces.map(|id| id.to_vec()),
                in_reply_to: e.content.in_reply_to.map(|id| id.to_vec()),
            })
            .collect())
    }

    /// This client's display name.
    #[must_use]
    pub fn identity(&self) -> String {
        self.lock().identity()
    }

    /// How many members are currently in the group on `channel`.
    #[must_use]
    pub fn member_count(&self, channel: Vec<u8>) -> u32 {
        // A group cannot plausibly exceed u32::MAX; saturate rather than wrap.
        u32::try_from(self.lock().member_count(&channel)).unwrap_or(u32::MAX)
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

/// A message in the stored transcript.
#[derive(uniffi::Record)]
pub struct TranscriptMessage {
    /// Its MIMI message id, stable across resends.
    pub id: Vec<u8>,
    /// Its place in the conversation, increasing in the relay's order: what
    /// [`Client::history`] pages by. Absent for this client's own message
    /// until the relay has it back; those come last.
    pub position: Option<u64>,
    /// The relay's sequence number for it. Shown, not sorted by: a reset
    /// relay numbers from the start again.
    pub seq: Option<u64>,
    /// When the relay accepted it, in milliseconds since the Unix epoch.
    pub accepted_ms: Option<u64>,
    /// When this client stored it, by its own clock, in milliseconds.
    pub stored_ms: u64,
    /// Whether this client sent it.
    pub mine: bool,
    /// Who sent it: their self-declared name.
    pub sender: String,
    /// Their provisional URI, from the key MLS authenticated it with.
    pub sender_uri: String,
    /// The text to show.
    pub text: String,
    /// For an edit or delete: the id of the message it replaces.
    pub replaces: Option<Vec<u8>>,
    /// For a reply or reaction: the id of the message it answers.
    pub in_reply_to: Option<Vec<u8>>,
}

/// Leaves out the text, as the core's message types do.
impl std::fmt::Debug for TranscriptMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranscriptMessage")
            .field("position", &self.position)
            .field("sender", &self.sender)
            .field("mine", &self.mine)
            .finish_non_exhaustive()
    }
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
