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
//!
//! # One writer
//!
//! Numbering, storing and delivering all happen on one thread, the writer.
//! Connections hand it their publishes and wait to hear where each landed.
//! Being the only thing that numbers or delivers is what keeps the order
//! agreed (#102). The writer takes every publish waiting at that moment and
//! stores the lot in one transaction, so frames that arrive together share
//! one flush to disk (#117). A flush per frame capped the whole relay at
//! about 250 frames a second with a true flush, however many clients were
//! publishing; sharing it lets throughput grow with load.

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
use tokio::sync::{Notify, mpsc, oneshot};

/// Frames queued for one connection before it counts as too slow to keep up.
const BACKLOG: usize = 256;

/// Addresses one connection may subscribe to. A client needs its channel and
/// its mailbox; this is headroom, not a quota anyone should reach.
const MAX_SUBSCRIPTIONS: usize = 64;

/// Publishes waiting for the writer. Bounded, so a flood makes connections
/// wait their turn instead of growing memory without limit.
const WRITE_QUEUE: usize = 1024;

/// The most frames committed in one transaction. Bounds a transaction's size,
/// and how long the last frame in a batch waits behind the rest.
const MAX_BATCH: usize = 256;

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

/// Who is subscribed to what.
///
/// Poisoned locks panic rather than recover: a panic partway through an
/// update means routing state can no longer be trusted.
type Routes = Arc<Mutex<HashMap<Address, HashMap<usize, Subscriber>>>>;

/// Why a publish did not land.
#[derive(Debug, Clone, thiserror::Error)]
enum PublishError {
    /// The store refused the batch this frame was in.
    #[error(transparent)]
    Store(Arc<store::Error>),
    /// The writer has stopped, so nothing can be stored.
    #[error("the relay's writer has stopped")]
    WriterStopped,
}

/// Where a published frame landed: its sequence number, and how many
/// subscribers it reached.
type Landed = Result<(u64, usize), PublishError>;

/// A publish waiting for the writer.
struct Publish {
    address: Address,
    payload: Vec<u8>,
    landed: oneshot::Sender<Landed>,
}

#[derive(Clone)]
struct Relay {
    /// The writer's queue. See the module docs for why there is one writer.
    writes: mpsc::Sender<Publish>,
    /// Changed by connections as they subscribe and leave; read by the writer
    /// as it delivers.
    routes: Routes,
}

impl Relay {
    /// Start the writer over `store`.
    fn start(store: Store) -> std::io::Result<Self> {
        let (writes, queue) = mpsc::channel(WRITE_QUEUE);
        let routes = Routes::default();
        let writer_routes = routes.clone();
        // A thread rather than a blocking task: it lives as long as the relay,
        // and it ends by itself once every connection and the router have
        // dropped their senders.
        std::thread::Builder::new()
            .name("relay-writer".into())
            .spawn(move || write_loop(store, queue, &writer_routes))?;
        Ok(Self { writes, routes })
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

    /// Hand a frame to the writer, and wait until it is stored and delivered.
    async fn publish(&self, address: Address, payload: Vec<u8>) -> Landed {
        let (landed, reply) = oneshot::channel();
        let publish = Publish {
            address,
            payload,
            landed,
        };
        self.writes
            .send(publish)
            .await
            .map_err(|_| PublishError::WriterStopped)?;
        reply.await.map_err(|_| PublishError::WriterStopped)?
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

/// The writer: take whatever publishes are waiting, commit them together,
/// and repeat until every sender has gone.
fn write_loop(mut store: Store, mut queue: mpsc::Receiver<Publish>, routes: &Routes) {
    let mut batch = Vec::with_capacity(MAX_BATCH);
    while let Some(first) = queue.blocking_recv() {
        batch.push(first);
        while batch.len() < MAX_BATCH {
            let Ok(next) = queue.try_recv() else { break };
            batch.push(next);
        }
        commit(&mut store, routes, &mut batch);
    }
}

/// Number and store `batch` in one transaction, and only then deliver it, in
/// sequence order, and tell each publisher where its frame landed.
///
/// Storing first is what makes delivery a promise: a subscriber never
/// receives a frame that a crash could lose. If the store refuses the batch,
/// none of it is delivered and every publisher in it is told so.
fn commit(store: &mut Store, routes: &Routes, batch: &mut Vec<Publish>) {
    let (frames, replies): (Vec<_>, Vec<_>) = batch
        .drain(..)
        .map(|p| ((p.address, p.payload), p.landed))
        .unzip();
    let deliveries = match store.append_batch(frames, now_ms()) {
        Ok(deliveries) => deliveries,
        Err(error) => {
            let error = Arc::new(error);
            for landed in replies {
                // A publisher that has gone away no longer needs telling.
                let _ = landed.send(Err(PublishError::Store(error.clone())));
            }
            return;
        }
    };
    let frames: Vec<_> = deliveries
        .into_iter()
        .map(|d| {
            let frame = Bytes::from(d.encode());
            (d.address, d.seq, frame)
        })
        .collect();
    let mut routes = routes.lock().expect("routing state poisoned");
    for ((address, seq, frame), landed) in frames.into_iter().zip(replies) {
        let reached = deliver(&mut routes, &address, &frame);
        let _ = landed.send(Ok((seq, reached)));
    }
}

/// Hand a frame to everyone subscribed to `address`, dropping any connection
/// too far behind to take it. Returns how many it reached.
fn deliver(
    routes: &mut HashMap<Address, HashMap<usize, Subscriber>>,
    address: &Address,
    frame: &Bytes,
) -> usize {
    let Some(subscribers) = routes.get_mut(address) else {
        return 0;
    };
    // `try_send` never waits, so a slow client cannot stall the writer.
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
        routes.remove(address);
    }
    reached
}

/// Build the relay router over `store`, starting the thread that writes to
/// it. Exposed so tests can serve it on an ephemeral port.
///
/// # Errors
/// If the writer thread cannot be started.
pub fn router(store: Store) -> std::io::Result<Router> {
    Ok(Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(upgrade))
        .with_state(Relay::start(store)?))
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
                    // Awaited before the next request is read, so one
                    // connection's requests still take effect in order.
                    match inbound_relay.publish(to, payload).await {
                        Ok((seq, reached)) => {
                            tracing::debug!(id, seq, bytes, reached, "stored and relayed");
                        }
                        // There is no acknowledgement to fail, so closing the
                        // connection is how the publisher learns the frame
                        // went nowhere, rather than believing it was sent.
                        Err(error) => {
                            tracing::error!(id, %error, "could not store frame; disconnecting");
                            return;
                        }
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

    fn addr(bytes: &[u8]) -> Address {
        Address::new(bytes.to_vec()).unwrap()
    }

    /// Subscribe a fake connection to `address`, returning its queue.
    fn subscriber(routes: &Routes, id: usize, address: &Address) -> mpsc::Receiver<Bytes> {
        let (frames, rx) = mpsc::channel(BACKLOG);
        let kick = Arc::new(Notify::new());
        routes
            .lock()
            .unwrap()
            .entry(address.clone())
            .or_default()
            .insert(id, Subscriber { frames, kick });
        rx
    }

    /// A batch as the writer would take it off its queue.
    fn batch(frames: &[(&Address, &[u8])]) -> (Vec<Publish>, Vec<oneshot::Receiver<Landed>>) {
        frames
            .iter()
            .map(|(address, payload)| {
                let (landed, reply) = oneshot::channel();
                let publish = Publish {
                    address: (*address).clone(),
                    payload: payload.to_vec(),
                    landed,
                };
                (publish, reply)
            })
            .unzip()
    }

    /// A committed batch reaches subscribers in sequence order, and each
    /// publisher learns its own frame's number.
    #[test]
    fn a_batch_is_delivered_in_order_once_stored() {
        let mut store = Store::in_memory().unwrap();
        let routes = Routes::default();
        let (room, other) = (addr(b"room"), addr(b"other"));
        let mut rx = subscriber(&routes, 0, &room);
        let (mut publishes, replies) = batch(&[(&room, b"a"), (&other, b"x"), (&room, b"b")]);

        commit(&mut store, &routes, &mut publishes);

        let landed: Vec<_> = replies
            .into_iter()
            .map(|mut r| r.try_recv().unwrap().unwrap())
            .collect();
        assert_eq!(landed, [(0, 1), (0, 0), (1, 1)]);
        let seen: Vec<(u64, Vec<u8>)> = std::iter::from_fn(|| rx.try_recv().ok())
            .map(|f| openconv_wire::Delivery::decode(&f).unwrap())
            .map(|d| (d.seq, d.payload))
            .collect();
        assert_eq!(seen, [(0, b"a".to_vec()), (1, b"b".to_vec())]);
    }

    /// Delivery is a promise that the frame is stored. A batch the store
    /// refused must reach nobody, and every publisher in it must hear so,
    /// or a crash could lose something a subscriber has already acted on.
    #[test]
    fn a_batch_that_cannot_be_stored_is_delivered_to_nobody() {
        let mut store = Store::in_memory().unwrap();
        store.fail_writes();
        let routes = Routes::default();
        let room = addr(b"room");
        let mut rx = subscriber(&routes, 0, &room);
        let (mut publishes, replies) = batch(&[(&room, b"a"), (&room, b"b"), (&room, b"c")]);

        commit(&mut store, &routes, &mut publishes);

        for mut reply in replies {
            assert!(matches!(
                reply.try_recv().unwrap(),
                Err(PublishError::Store(_))
            ));
        }
        assert!(rx.try_recv().is_err(), "an unstored frame was delivered");
    }
}
