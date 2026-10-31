//! OpenConv relay.
//!
//! The relay routes opaque frames to whoever is subscribed to their address.
//! It never parses, decrypts, or stores payloads. It reads the envelope
//! (`openconv-wire`) because routing needs an address, and nothing past it.
//! Keeping it that way is the privacy argument, so resist adding anything
//! that inspects a payload.
//!
//! An address is opaque too. A channel's MLS group id and a joiner's
//! `KeyPackageRef` look the same here: the relay knows that some connections
//! want some byte strings, and not what either one is.
//!
//! Its one semantic job is ordering. Every frame published to an address
//! gets that address's next sequence number, and every subscriber sees the
//! address's frames in that order. MLS needs a single winner when two members
//! commit at the same epoch, and this is what decides it — without the relay
//! knowing which frames are commits.
//!
//! Publishers subscribed to the address get their own frames back, so a
//! sender learns where its frame landed in the order. Clients recognise their
//! own frames by content, which keeps sender identity off the wire.
//!
//! Nothing is stored. A frame published to an address with no subscribers is
//! dropped. An address is forgotten when its last subscriber leaves, and its
//! sequence starts again from zero if anyone subscribes later. Continuity
//! across that is the job of durable storage (#104).

use axum::{
    Router,
    body::Bytes,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use openconv_wire::{Address, Delivery, Request};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Notify, mpsc};

/// Frames queued for one connection before it counts as too slow to keep up.
const BACKLOG: usize = 256;

/// Addresses one connection may subscribe to. A client needs its channel and
/// its mailbox; this is headroom, not a quota anyone should reach.
const MAX_SUBSCRIPTIONS: usize = 64;

/// A connection, as a channel sees it.
#[derive(Clone)]
struct Subscriber {
    frames: mpsc::Sender<Bytes>,
    /// Tells the connection to close because it fell behind. A queue that
    /// overflowed has lost frames, and a client missing frames can no longer
    /// trust its view of the order.
    kick: Arc<Notify>,
}

/// One address's ordering state and audience.
#[derive(Default)]
struct Channel {
    /// The next sequence number to hand out.
    next_seq: u64,
    subscribers: HashMap<usize, Subscriber>,
}

impl Channel {
    /// Number a frame and hand it to every subscriber.
    ///
    /// One call, under the channel's lock, so numbering and delivery cannot
    /// interleave. With the two separate, concurrent frames get numbers 4
    /// and 5, then race to the queues, and some clients receive 5 before 4.
    /// Clients then disagree about which commit won its epoch, which is the
    /// one thing this relay exists to decide. `try_send` never waits, so the
    /// lock is never held across a yield.
    fn publish(&mut self, address: &Address, payload: Vec<u8>) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let frame = Bytes::from(
            Delivery {
                address: address.clone(),
                seq,
                accepted_ms: now_ms(),
                payload,
            }
            .encode(),
        );
        self.subscribers
            .retain(|id, sub| match sub.frames.try_send(frame.clone()) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(id, "client lagged; disconnecting");
                    sub.kick.notify_one();
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            });
        seq
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[derive(Clone, Default)]
struct Relay {
    /// Every address someone is subscribed to.
    ///
    /// The map's lock is held only to find or create a channel, and to add
    /// or remove subscribers. Numbering happens under each channel's own
    /// lock, so traffic on one channel never waits for another.
    ///
    /// Poisoned locks panic rather than recover: a panic partway through an
    /// update means routing state can no longer be trusted.
    channels: Arc<Mutex<HashMap<Address, Arc<Mutex<Channel>>>>>,
}

impl Relay {
    /// Add a connection to each of `addresses`, taking the map's lock once
    /// for the whole request.
    fn subscribe(&self, id: usize, subscriber: &Subscriber, addresses: Vec<Address>) {
        if addresses.is_empty() {
            return;
        }
        let mut channels = self.channels.lock().expect("relay state poisoned");
        for address in addresses {
            channels
                .entry(address)
                .or_default()
                .lock()
                .expect("channel state poisoned")
                .subscribers
                .insert(id, subscriber.clone());
        }
    }

    fn publish(&self, address: &Address, payload: Vec<u8>) -> Option<u64> {
        // Clone the handle and let go of the map before numbering, so the
        // map's lock does not serialise every channel's traffic.
        let channel = self
            .channels
            .lock()
            .expect("relay state poisoned")
            .get(address)
            .cloned()?;
        let seq = channel
            .lock()
            .expect("channel state poisoned")
            .publish(address, payload);
        Some(seq)
    }

    fn unsubscribe_all(&self, id: usize, addresses: &HashSet<Address>) {
        let mut channels = self.channels.lock().expect("relay state poisoned");
        for address in addresses {
            let Some(channel) = channels.get(address) else {
                continue;
            };
            let empty = {
                let mut channel = channel.lock().expect("channel state poisoned");
                channel.subscribers.remove(&id);
                channel.subscribers.is_empty()
            };
            if empty {
                channels.remove(address);
            }
        }
    }
}

/// Build the relay router. Exposed so tests can serve it on an ephemeral port.
pub fn router() -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(upgrade))
        .with_state(Relay::default())
}

async fn upgrade(ws: WebSocketUpgrade, State(relay): State<Relay>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle(socket, relay))
}

async fn handle(socket: WebSocket, relay: Relay) {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = mpsc::channel(BACKLOG);
    let me = Subscriber {
        frames: tx,
        kick: Arc::new(Notify::new()),
    };
    // Shared with the inbound task so cleanup still knows what to unsubscribe
    // when the outbound side is the one that ends the connection.
    let subscribed = Arc::new(Mutex::new(HashSet::new()));
    let (mut sink, mut stream) = socket.split();
    tracing::info!(id, "client connected");

    // Outbound: everything published to this connection's addresses, in
    // each address's sequence order, including this client's own frames.
    let kick = me.kick.clone();
    let mut outbound = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Checked first: a lagged client must not keep draining a
                // queue that has already lost frames.
                biased;
                () = kick.notified() => break,
                frame = rx.recv() => {
                    let Some(frame) = frame else { break };
                    if sink.send(Message::Binary(frame)).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Inbound: subscriptions and publishes. A malformed request ends the
    // connection; a client sending rubbish is broken or hostile, and neither
    // is helped by the relay guessing what it meant.
    let inbound_relay = relay.clone();
    let inbound_subscribed = subscribed.clone();
    let mut inbound = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let bytes = match msg {
                Message::Binary(bytes) => bytes,
                Message::Close(_) => break,
                _ => continue,
            };
            match Request::decode(&bytes) {
                Ok(Request::Subscribe(addresses)) => {
                    let mut subscribed = inbound_subscribed
                        .lock()
                        .expect("subscription set poisoned");
                    let mut added = Vec::new();
                    for address in addresses {
                        if subscribed.len() == MAX_SUBSCRIPTIONS && !subscribed.contains(&address) {
                            tracing::warn!(id, "too many subscriptions; disconnecting");
                            return;
                        }
                        if subscribed.insert(address.clone()) {
                            added.push(address);
                        }
                    }
                    inbound_relay.subscribe(id, &me, added);
                }
                Ok(Request::Publish { to, payload }) => {
                    let bytes = payload.len();
                    if let Some(seq) = inbound_relay.publish(&to, payload) {
                        tracing::debug!(id, seq, bytes, "relayed");
                    } else {
                        tracing::debug!(id, bytes, "no subscribers; dropped");
                    }
                }
                Err(error) => {
                    tracing::warn!(id, %error, "malformed request; disconnecting");
                    return;
                }
            }
        }
    });

    // Either direction ending tears down the connection.
    let outbound_ended = tokio::select! {
        _ = &mut outbound => true,
        _ = &mut inbound => false,
    };
    // Wait for the other side to actually stop. `abort` only takes effect at
    // the task's next yield, so without this an inbound task still running
    // could subscribe after the set below is taken, and that subscription
    // would never be removed.
    let other = if outbound_ended { inbound } else { outbound };
    other.abort();
    let _ = other.await;
    let subscribed = std::mem::take(&mut *subscribed.lock().expect("subscription set poisoned"));
    relay.unsubscribe_all(id, &subscribed);
    tracing::info!(id, "client disconnected");
}
