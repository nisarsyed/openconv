//! OpenConv relay.
//!
//! The relay routes opaque frames to whoever is subscribed to their address.
//! It never parses or decrypts payloads. It reads the envelope
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
//! Every frame is stored before anyone receives it (see [`store`]), including
//! frames to an address nobody is subscribed to: that is how a joiner who is
//! offline still gets their `Welcome`, and how a member who was away will
//! catch up (#105). Sequence numbers live in the store too, so a restarted
//! relay carries on from where it stopped rather than reusing numbers.

pub mod store;

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
use openconv_wire::{Address, Request};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use store::Store;
use tokio::sync::{Notify, mpsc};

/// Frames queued for one connection before it counts as too slow to keep up.
const BACKLOG: usize = 256;

/// Addresses one connection may subscribe to. A client needs its channel and
/// its mailbox; this is headroom, not a quota anyone should reach.
const MAX_SUBSCRIPTIONS: usize = 64;

/// A connection, as an address sees it.
#[derive(Clone)]
struct Subscriber {
    frames: mpsc::Sender<Bytes>,
    /// Tells the connection to close because it fell behind. A queue that
    /// overflowed has lost frames, and a client missing frames can no longer
    /// trust its view of the order.
    kick: Arc<Notify>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[derive(Clone)]
struct Relay {
    /// Where frames are numbered and kept.
    ///
    /// This lock is also the ordering lock. A frame is numbered, written and
    /// handed to every subscriber in one critical section, so no two can
    /// interleave. With numbering and delivery separate, concurrent frames
    /// get numbers 4 and 5, then race to the queues, and some clients
    /// receive 5 before 4 (#102). Clients then disagree about which commit
    /// won its epoch, which is the one thing this relay exists to decide.
    ///
    /// One lock for every address, where there used to be one per channel.
    /// SQLite allows a single writer, so publishes were going to serialise
    /// on the write regardless; delivery under the same lock adds only
    /// `try_send`s, which never wait. If this ever limits throughput, the
    /// fix is sharding stores by address, not splitting the lock.
    store: Arc<Mutex<Store>>,
    /// Who is subscribed to what. Taken after `store` when both are needed,
    /// and alone to add or remove subscribers.
    ///
    /// Poisoned locks panic rather than recover: a panic partway through an
    /// update means routing state can no longer be trusted.
    routes: Arc<Mutex<HashMap<Address, HashMap<usize, Subscriber>>>>,
}

impl Relay {
    fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            routes: Arc::default(),
        }
    }

    /// Add a connection to each of `addresses`, taking the lock once for the
    /// whole request.
    fn subscribe(&self, id: usize, subscriber: &Subscriber, addresses: Vec<Address>) {
        if addresses.is_empty() {
            return;
        }
        let mut routes = self.routes.lock().expect("routing state poisoned");
        for address in addresses {
            routes
                .entry(address)
                .or_default()
                .insert(id, subscriber.clone());
        }
    }

    /// Number a frame, store it, and only then deliver it.
    ///
    /// Blocks on SQLite, so call it off the async runtime. Storing first is
    /// what makes delivery a promise: a subscriber never receives a frame
    /// that a crash could lose, and a failed write is delivered to nobody.
    fn publish(&self, address: Address, payload: Vec<u8>) -> store::Result<(u64, usize)> {
        let mut store = self.store.lock().expect("store poisoned");
        let delivery = store.append(address, now_ms(), payload)?;
        let seq = delivery.seq;
        let frame = Bytes::from(delivery.encode());
        let mut routes = self.routes.lock().expect("routing state poisoned");
        let Some(subscribers) = routes.get_mut(&delivery.address) else {
            return Ok((seq, 0));
        };
        subscribers.retain(|id, sub| match sub.frames.try_send(frame.clone()) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(id, "client lagged; disconnecting");
                sub.kick.notify_one();
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        });
        let reached = subscribers.len();
        if subscribers.is_empty() {
            routes.remove(&delivery.address);
        }
        Ok((seq, reached))
    }

    fn unsubscribe_all(&self, id: usize, addresses: &HashSet<Address>) {
        let mut routes = self.routes.lock().expect("routing state poisoned");
        for address in addresses {
            let Some(subscribers) = routes.get_mut(address) else {
                continue;
            };
            subscribers.remove(&id);
            if subscribers.is_empty() {
                routes.remove(address);
            }
        }
    }
}

/// Build the relay router over `store`. Exposed so tests can serve it on an
/// ephemeral port.
pub fn router(store: Store) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(upgrade))
        .with_state(Relay::new(store))
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
                    let relay = inbound_relay.clone();
                    // Awaited before the next request is read, so one
                    // connection's requests still take effect in order.
                    match tokio::task::spawn_blocking(move || relay.publish(to, payload)).await {
                        Ok(Ok((seq, reached))) => {
                            tracing::debug!(id, seq, bytes, reached, "stored and relayed");
                        }
                        // There is no acknowledgement to fail, so closing the
                        // connection is how the publisher learns the frame
                        // went nowhere, rather than believing it was sent.
                        Ok(Err(error)) => {
                            tracing::error!(id, %error, "could not store frame; disconnecting");
                            return;
                        }
                        Err(join) if join.is_panic() => {
                            std::panic::resume_unwind(join.into_panic())
                        }
                        // Only happens while the runtime shuts down.
                        Err(_) => return,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn subscriber() -> (Subscriber, mpsc::Receiver<Bytes>) {
        let (frames, rx) = mpsc::channel(BACKLOG);
        let kick = Arc::new(Notify::new());
        (Subscriber { frames, kick }, rx)
    }

    /// Delivery is a promise that the frame is stored. A frame the store
    /// refused must reach nobody, or a crash could lose something a
    /// subscriber has already acted on.
    #[test]
    fn a_frame_that_cannot_be_stored_is_not_delivered() {
        let relay = Relay::new(Store::in_memory().unwrap());
        let room = Address::new(b"room".to_vec()).unwrap();
        let (sub, mut rx) = subscriber();
        relay.subscribe(0, &sub, vec![room.clone()]);

        assert_eq!(
            relay.publish(room.clone(), b"kept".to_vec()).unwrap(),
            (0, 1)
        );
        assert!(rx.try_recv().is_ok(), "a stored frame should be delivered");

        relay.store.lock().unwrap().fail_writes();
        assert!(relay.publish(room, b"lost".to_vec()).is_err());
        assert!(rx.try_recv().is_err(), "an unstored frame was delivered");
    }
}
