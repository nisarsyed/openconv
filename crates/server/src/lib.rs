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
//!
//! The writer also serves catch-up (`Resume`, see `openconv-wire`). Because
//! it is the only thing that delivers, a page it reads from the store and the
//! live subscription it registers after the last page cannot have a frame
//! slip between them: nothing is published while the writer is busy serving.

pub mod config;
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
use openconv_wire::{Address, Inbound, Request};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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

/// Stored frames sent per catch-up page. Well under [`BACKLOG`], so a page
/// plus its notice fits a connection's queue with room for live traffic.
const PAGE_SIZE: usize = 128;

/// How long a catch-up in progress holds retention back after its last
/// page. A client asks for the next page as soon as it has applied one, so
/// this only has to outlast a slow round trip; a client that stops asking
/// stops pinning storage once it passes.
const PIN_GRACE: Duration = Duration::from_secs(120);

/// How long frames are kept, and how much each address may hold (#106).
///
/// Only members already in a group who were offline are served by retention:
/// a new member cannot read frames from before it joined however long they
/// are kept. So `window` answers one question, how long someone may be away
/// and still catch up, and lowering it costs nothing else. `channel_cap`
/// bounds disk use, but on a busy address it can bite before the window
/// does, and then it is what limits the time away.
///
/// Retention bounds history, not abuse. Until identity (milestone 3), anyone
/// who can reach the relay can publish to any address, including made-up
/// ones, so the per-address cap alone does not bound disk. `relay_cap` does:
/// over it, the oldest frames anywhere go first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// Frames older than this are deleted.
    pub window: Duration,
    /// An address over this many payload bytes loses its oldest frames.
    pub channel_cap: u64,
    /// The store over this many payload bytes, across every address, loses
    /// its oldest frames wherever they are.
    pub relay_cap: u64,
}

impl Default for Retention {
    /// Thirty days, which covers a holiday or a broken laptop; 64 MiB per
    /// address, which holds a month of a busy group chat; and 4 GiB in all.
    fn default() -> Self {
        Self {
            window: Duration::from_secs(30 * 24 * 60 * 60),
            channel_cap: 64 * 1024 * 1024,
            relay_cap: 4 * 1024 * 1024 * 1024,
        }
    }
}

/// Everything an operator can set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// How long frames are kept, and how much.
    pub retention: Retention,
    /// The largest WebSocket message a client may send. A larger one closes
    /// the connection. MLS messages run from a few hundred bytes to a few
    /// KiB, so 1 MiB is generous, and it stops a single frame from filling a
    /// channel's cap and pushing out all of its history.
    pub max_frame: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            retention: Retention::default(),
            max_frame: 1024 * 1024,
        }
    }
}

impl Retention {
    /// How often expired frames are swept: often enough that nothing
    /// outlives the window by much, and at most once a minute.
    fn sweep_every(self) -> Duration {
        (self.window / 10).clamp(Duration::from_millis(50), Duration::from_secs(60))
    }
}

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

/// Why the writer could not do what a connection asked.
#[derive(Debug, Clone, thiserror::Error)]
enum WriteError {
    /// The store failed: it refused the batch a frame was in, or could not
    /// be read for catch-up.
    #[error(transparent)]
    Store(Arc<store::Error>),
    /// The connection's queue was full, so a page could not be handed over.
    #[error("connection too far behind to take a catch-up page")]
    Lagged,
    /// The writer has stopped, so nothing can be stored.
    #[error("the relay's writer has stopped")]
    WriterStopped,
}

/// Where a published frame landed: its sequence number, and how many
/// subscribers it reached.
type Landed = Result<(u64, usize), WriteError>;

/// Work for the writer, in the order connections asked for it.
enum Job {
    Publish(Publish),
    Resume(Resume),
    /// Delete frames past the retention window. Queued by a timer, so
    /// eviction is ordered with everything else the writer does.
    Sweep,
}

/// A publish waiting for the writer.
struct Publish {
    address: Address,
    payload: Vec<u8>,
    landed: oneshot::Sender<Landed>,
}

/// A connection asking to catch up on `address` from `from`.
struct Resume {
    id: usize,
    subscriber: Subscriber,
    address: Address,
    from: u64,
    served: oneshot::Sender<Result<(), WriteError>>,
}

#[derive(Clone)]
struct Relay {
    /// The writer's queue. See the module docs for why there is one writer.
    writes: mpsc::Sender<Job>,
    /// See [`Config::max_frame`].
    max_frame: usize,
    /// Changed by connections as they subscribe and leave; read by the writer
    /// as it delivers.
    routes: Routes,
}

impl Relay {
    /// Start the writer over `store`, and the timer that sweeps it.
    fn start(store: Store, config: Config) -> std::io::Result<Self> {
        let retention = config.retention;
        let (writes, queue) = mpsc::channel(WRITE_QUEUE);
        let routes = Routes::default();
        let state = Writer {
            store,
            routes: routes.clone(),
            retention,
            pins: HashMap::new(),
        };
        // Threads rather than blocking tasks: they live as long as the
        // relay, and end by themselves once every connection and the router
        // have dropped their senders. The timer holds only a weak handle, so
        // it cannot keep the writer alive.
        std::thread::Builder::new()
            .name("relay-writer".into())
            .spawn(move || write_loop(state, queue))?;
        let timer = writes.downgrade();
        std::thread::Builder::new()
            .name("relay-sweeper".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(retention.sweep_every());
                    let Some(writes) = timer.upgrade() else { break };
                    if writes.blocking_send(Job::Sweep).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            writes,
            max_frame: config.max_frame,
            routes,
        })
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
            .send(Job::Publish(publish))
            .await
            .map_err(|_| WriteError::WriterStopped)?;
        reply.await.map_err(|_| WriteError::WriterStopped)?
    }

    /// Ask the writer for a page of catch-up, and wait until it is queued
    /// for this connection.
    async fn resume(
        &self,
        id: usize,
        subscriber: &Subscriber,
        address: Address,
        from: u64,
    ) -> Result<(), WriteError> {
        let (served, reply) = oneshot::channel();
        let resume = Resume {
            id,
            subscriber: subscriber.clone(),
            address,
            from,
            served,
        };
        self.writes
            .send(Job::Resume(resume))
            .await
            .map_err(|_| WriteError::WriterStopped)?;
        reply.await.map_err(|_| WriteError::WriterStopped)?
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

/// The writer thread's state.
struct Writer {
    store: Store,
    routes: Routes,
    retention: Retention,
    /// Catch-ups in progress, by connection and address: where the next page
    /// starts, and when the last one was sent. Retention keeps everything
    /// from there, so a client cannot be stranded between pages.
    pins: HashMap<(usize, Address), (u64, Instant)>,
}

impl Writer {
    /// Tell the store what to hold back, dropping catch-ups that have gone
    /// quiet for longer than [`PIN_GRACE`].
    fn apply_pins(&mut self) {
        self.pins.retain(|_, (_, at)| at.elapsed() < PIN_GRACE);
        let mut lowest: HashMap<&Address, u64> = HashMap::new();
        for ((_, address), (seq, _)) in &self.pins {
            let pin = lowest.entry(address).or_insert(*seq);
            *pin = (*pin).min(*seq);
        }
        let pins: Vec<_> = lowest.into_iter().map(|(a, s)| (a.clone(), s)).collect();
        if let Err(error) = self.store.set_pins(&pins) {
            tracing::warn!(%error, "could not set retention pins");
        }
    }

    /// Delete everything past the retention window.
    fn sweep(&mut self) {
        self.apply_pins();
        let window = u64::try_from(self.retention.window.as_millis()).unwrap_or(u64::MAX);
        match self.store.expire(now_ms().saturating_sub(window)) {
            Ok(0) => {}
            Ok(count) => tracing::info!(count, "expired frames past the retention window"),
            Err(error) => tracing::warn!(%error, "could not expire frames"),
        }
    }

    /// Bring each of `addresses` back under the size cap if it went over,
    /// and the whole store under the relay-wide cap, trimming to nine tenths
    /// of each so a busy relay is not trimmed every batch.
    fn enforce_cap(&mut self, addresses: &HashSet<Address>) {
        let cap = self.retention.channel_cap;
        let mut pinned = false;
        for address in addresses {
            if self.store.bytes(address).is_ok_and(|bytes| bytes <= cap) {
                continue;
            }
            if !pinned {
                self.apply_pins();
                pinned = true;
            }
            match self.store.trim(address, cap / 10 * 9) {
                Ok(count) => tracing::info!(count, "trimmed an address over its size cap"),
                Err(error) => tracing::warn!(%error, "could not trim an address"),
            }
        }
        let relay_cap = self.retention.relay_cap;
        if self.store.total_bytes() > relay_cap {
            if !pinned {
                self.apply_pins();
            }
            match self.store.trim_total(relay_cap / 10 * 9) {
                Ok(count) => {
                    tracing::warn!(count, "relay over its total cap; dropped the oldest frames");
                }
                Err(error) => tracing::warn!(%error, "could not trim the relay"),
            }
        }
    }
}

/// The writer: take whatever publishes are waiting, commit them together,
/// serve resumes and sweeps in turn, and repeat until every sender has gone.
///
/// Jobs are handled in the order they were queued. A run of publishes
/// becomes one batch; a job behind them waits for that batch to commit, so a
/// resume sees them in the store.
fn write_loop(mut writer: Writer, mut queue: mpsc::Receiver<Job>) {
    let mut batch = Vec::with_capacity(MAX_BATCH);
    let mut held = None;
    while let Some(job) = held.take().or_else(|| queue.blocking_recv()) {
        match job {
            Job::Resume(resume) => serve(&mut writer, resume),
            Job::Sweep => writer.sweep(),
            Job::Publish(first) => {
                batch.push(first);
                while batch.len() < MAX_BATCH {
                    match queue.try_recv() {
                        Ok(Job::Publish(next)) => batch.push(next),
                        Ok(other) => {
                            held = Some(other);
                            break;
                        }
                        Err(_) => break,
                    }
                }
                commit(&mut writer, &mut batch);
            }
        }
    }
}

/// Send one page of catch-up to a connection, and subscribe it if that page
/// was the last. See the wire crate's module docs for the exchange.
fn serve(writer: &mut Writer, resume: Resume) {
    let result = page(writer, &resume);
    if result.is_err() {
        resume.subscriber.kick.notify_one();
    }
    let _ = resume.served.send(result);
}

fn page(writer: &mut Writer, resume: &Resume) -> Result<(), WriteError> {
    let Resume {
        id,
        subscriber,
        address,
        from,
        ..
    } = resume;
    let store = &writer.store;
    let read = |e| WriteError::Store(Arc::new(e));
    let next = store.next_seq(address).map_err(read)?;
    let floor = store.floor(address).map_err(read)?;
    let push = |inbound: Inbound| {
        subscriber
            .frames
            .try_send(Bytes::from(inbound.encode()))
            .map_err(|_| WriteError::Lagged)
    };
    let pin = (*id, address.clone());
    if *from < floor || *from > next {
        writer.pins.remove(&pin);
        return push(Inbound::Unavailable {
            address: address.clone(),
            floor,
            next,
        });
    }

    let frames = store.since(address, *from, PAGE_SIZE).map_err(read)?;
    let reached = frames.last().map_or(*from, |d| d.seq + 1);
    for frame in frames {
        push(Inbound::Delivery(frame))?;
    }
    let live = reached == next;
    if live {
        // Registered here, between reading the last page and the next job,
        // so the first live frame this connection gets is `next`.
        writer
            .routes
            .lock()
            .expect("routing state poisoned")
            .entry(address.clone())
            .or_default()
            .insert(*id, subscriber.clone());
        writer.pins.remove(&pin);
    } else {
        writer.pins.insert(pin, (reached, Instant::now()));
    }
    push(Inbound::Page {
        address: address.clone(),
        next: reached,
        live,
    })
}

/// Number and store `batch` in one transaction, and only then deliver it, in
/// sequence order, and tell each publisher where its frame landed. Then keep
/// the addresses it touched under their size cap.
///
/// Storing first is what makes delivery a promise: a subscriber never
/// receives a frame that a crash could lose. If the store refuses the batch,
/// none of it is delivered and every publisher in it is told so.
fn commit(writer: &mut Writer, batch: &mut Vec<Publish>) {
    let (frames, replies): (Vec<_>, Vec<_>) = batch
        .drain(..)
        .map(|p| ((p.address, p.payload), p.landed))
        .unzip();
    let deliveries = match writer.store.append_batch(frames, now_ms()) {
        Ok(deliveries) => deliveries,
        Err(error) => {
            let error = Arc::new(error);
            for landed in replies {
                // A publisher that has gone away no longer needs telling.
                let _ = landed.send(Err(WriteError::Store(error.clone())));
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
    let touched: HashSet<Address> = frames.iter().map(|(a, _, _)| a.clone()).collect();
    {
        let mut routes = writer.routes.lock().expect("routing state poisoned");
        for ((address, seq, frame), landed) in frames.into_iter().zip(replies) {
            let reached = deliver(&mut routes, &address, &frame);
            let _ = landed.send(Ok((seq, reached)));
        }
    }
    writer.enforce_cap(&touched);
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

/// Record `addresses` as subscribed by a connection, returning those that are
/// new, or `None` if that would take it past [`MAX_SUBSCRIPTIONS`].
fn admit(subscribed: &Mutex<HashSet<Address>>, addresses: Vec<Address>) -> Option<Vec<Address>> {
    let mut subscribed = subscribed.lock().expect("subscription set poisoned");
    let mut added = Vec::new();
    for address in addresses {
        if subscribed.len() == MAX_SUBSCRIPTIONS && !subscribed.contains(&address) {
            return None;
        }
        if subscribed.insert(address.clone()) {
            added.push(address);
        }
    }
    Some(added)
}

/// Build the relay router over `store`, configured by `config`,
/// and start the threads that write to it and sweep it. Exposed so tests can
/// serve it on an ephemeral port.
///
/// # Errors
/// If the writer or sweeper thread cannot be started.
pub fn router(store: Store, config: Config) -> std::io::Result<Router> {
    Ok(Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(upgrade))
        .with_state(Relay::start(store, config)?))
}

async fn upgrade(ws: WebSocketUpgrade, State(relay): State<Relay>) -> impl IntoResponse {
    // A larger message ends the connection: the read fails, and `handle`
    // treats a failed read like any other way a client goes away.
    ws.max_message_size(relay.max_frame)
        .max_frame_size(relay.max_frame)
        .on_upgrade(|socket| handle(socket, relay))
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
                    let Some(added) = admit(&inbound_subscribed, addresses) else {
                        tracing::warn!(id, "too many subscriptions; disconnecting");
                        return;
                    };
                    inbound_relay.subscribe(id, &me, added);
                }
                Ok(Request::Resume { address, from }) => {
                    // Counted against the limit now, so cleanup knows about
                    // it even if the connection ends before going live.
                    if admit(&inbound_subscribed, vec![address.clone()]).is_none() {
                        tracing::warn!(id, "too many subscriptions; disconnecting");
                        return;
                    }
                    if let Err(error) = inbound_relay.resume(id, &me, address, from).await {
                        tracing::warn!(id, %error, "could not serve catch-up; disconnecting");
                        return;
                    }
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

    /// The writer's state over `store`, with default retention.
    fn writer(store: Store) -> Writer {
        Writer {
            store,
            routes: Routes::default(),
            retention: Retention::default(),
            pins: HashMap::new(),
        }
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
        let mut w = writer(Store::in_memory().unwrap());
        let (room, other) = (addr(b"room"), addr(b"other"));
        let mut rx = subscriber(&w.routes, 0, &room);
        let (mut publishes, replies) = batch(&[(&room, b"a"), (&other, b"x"), (&room, b"b")]);

        commit(&mut w, &mut publishes);

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

    /// A resume as a connection would send it, with the reply to wait on.
    fn resume(
        address: &Address,
        from: u64,
    ) -> (
        Resume,
        mpsc::Receiver<Bytes>,
        oneshot::Receiver<Result<(), WriteError>>,
    ) {
        let (frames, rx) = mpsc::channel(BACKLOG);
        let (served, reply) = oneshot::channel();
        let subscriber = Subscriber {
            frames,
            kick: Arc::new(Notify::new()),
        };
        let resume = Resume {
            id: 7,
            subscriber,
            address: address.clone(),
            from,
            served,
        };
        (resume, rx, reply)
    }

    /// Everything queued for a connection, decoded.
    fn drain(rx: &mut mpsc::Receiver<Bytes>) -> Vec<Inbound> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .map(|b| Inbound::decode(&b).unwrap())
            .collect()
    }

    fn seqs_and_notice(inbound: &[Inbound]) -> (Vec<u64>, Option<&Inbound>) {
        let seqs = inbound
            .iter()
            .filter_map(|i| match i {
                Inbound::Delivery(d) => Some(d.seq),
                _ => None,
            })
            .collect();
        (seqs, inbound.last())
    }

    /// Catch-up comes a page at a time, and only the last page subscribes
    /// the connection, so it never receives one unbounded burst.
    #[test]
    fn catch_up_comes_in_pages_and_ends_live() {
        let mut store = Store::in_memory().unwrap();
        let room = addr(b"room");
        for i in 0..300u32 {
            store
                .append(room.clone(), 0, i.to_be_bytes().to_vec())
                .unwrap();
        }
        let mut w = writer(store);

        let mut from = 0;
        let mut pages = 0;
        loop {
            // 300 frames take three pages. Bounded, so a relay that never
            // goes live fails this test rather than hanging it.
            assert!(pages < 10, "still not live after {pages} pages");
            let (job, mut rx, mut reply) = resume(&room, from);
            serve(&mut w, job);
            reply.try_recv().unwrap().unwrap();
            let inbound = drain(&mut rx);
            let (seqs, notice) = seqs_and_notice(&inbound);
            assert!(seqs.len() <= PAGE_SIZE, "a page was {} frames", seqs.len());
            assert_eq!(seqs, (from..from + seqs.len() as u64).collect::<Vec<_>>());
            pages += 1;
            let Some(Inbound::Page { next, live, .. }) = notice else {
                panic!("a page must end with a Page notice, got {notice:?}");
            };
            let subscribed = w.routes.lock().unwrap().contains_key(&room);
            assert_eq!(subscribed, *live, "live only once the last page is sent");
            from = *next;
            if *live {
                break;
            }
        }
        assert_eq!((from, pages), (300, 3));
    }

    /// A cursor the relay cannot serve gets a notice rather than whatever
    /// history happens to be there.
    #[test]
    fn cursors_outside_the_store_are_unavailable() {
        let mut store = Store::in_memory().unwrap();
        let room = addr(b"room");
        for i in 0..5u8 {
            store.append(room.clone(), 0, vec![i]).unwrap();
        }
        store.forget_before(&room, 2);
        let mut w = writer(store);

        for from in [0, 1, 6] {
            let (job, mut rx, mut reply) = resume(&room, from);
            serve(&mut w, job);
            reply.try_recv().unwrap().unwrap();
            assert_eq!(
                drain(&mut rx),
                [Inbound::Unavailable {
                    address: room.clone(),
                    floor: 2,
                    next: 5
                }],
                "from {from}"
            );
        }
        assert!(
            w.routes.lock().unwrap().is_empty(),
            "an unavailable cursor subscribed"
        );

        // At the floor is fine.
        let (job, mut rx, _reply) = resume(&room, 2);
        serve(&mut w, job);
        assert_eq!(seqs_and_notice(&drain(&mut rx)).0, [2, 3, 4]);
    }

    /// 300 frames on `room`, all accepted long ago, behind a writer.
    fn old_frames(room: &Address) -> Writer {
        let mut store = Store::in_memory().unwrap();
        for i in 0..300u32 {
            store
                .append(room.clone(), 0, i.to_be_bytes().to_vec())
                .unwrap();
        }
        writer(store)
    }

    /// A sweep that runs between two pages of a client's catch-up must leave
    /// the next page there. Otherwise the client is stranded mid-catch-up by
    /// nothing more than timing.
    #[test]
    fn eviction_cannot_strand_a_catch_up_in_progress() {
        let room = addr(b"room");
        let mut w = old_frames(&room);
        let (job, mut rx, _reply) = resume(&room, 0);
        serve(&mut w, job);
        let Some(Inbound::Page {
            next: 128,
            live: false,
            ..
        }) = drain(&mut rx).pop()
        else {
            panic!("expected a first page ending at 128");
        };

        w.sweep();
        assert_eq!(
            w.store.floor(&room).unwrap(),
            128,
            "the sweep took the next page"
        );
        let (job, mut rx, _reply) = resume(&room, 128);
        serve(&mut w, job);
        let (seqs, _) = seqs_and_notice(&drain(&mut rx));
        assert_eq!(seqs.first(), Some(&128), "the next page was not served");
    }

    /// A sweep only takes what is past the window.
    #[test]
    fn a_sweep_keeps_frames_inside_the_window() {
        let mut store = Store::in_memory().unwrap();
        let room = addr(b"room");
        store.append(room.clone(), 0, b"old".to_vec()).unwrap();
        store
            .append(room.clone(), now_ms(), b"new".to_vec())
            .unwrap();
        let mut w = writer(store);
        w.sweep();
        let kept: Vec<_> = w
            .store
            .since(&room, 0, 10)
            .unwrap()
            .into_iter()
            .map(|d| d.payload)
            .collect();
        assert_eq!(kept, [b"new".to_vec()]);
    }

    /// A client that stops asking for pages stops holding storage back once
    /// the grace period passes, and is told what it missed if it returns.
    #[test]
    fn a_catch_up_that_goes_quiet_stops_pinning() {
        let room = addr(b"room");
        let mut w = old_frames(&room);
        let (job, mut rx, _reply) = resume(&room, 0);
        serve(&mut w, job);
        drain(&mut rx);
        for (_, at) in w.pins.values_mut() {
            *at -= PIN_GRACE + Duration::from_secs(1);
        }

        w.sweep();
        assert!(w.pins.is_empty());
        let (job, mut rx, _reply) = resume(&room, 128);
        serve(&mut w, job);
        assert!(matches!(
            drain(&mut rx).as_slice(),
            [Inbound::Unavailable { .. }]
        ));
    }

    /// An address that goes over its cap is trimmed after the batch that
    /// took it over, down to nine tenths of the cap, oldest first.
    #[test]
    fn an_address_over_its_cap_is_trimmed_after_a_batch() {
        let mut w = writer(Store::in_memory().unwrap());
        w.retention.channel_cap = 1000;
        let room = addr(b"room");
        for _ in 0..15 {
            let payload = [0u8; 100];
            let (mut publishes, _replies) = batch(&[(&room, &payload)]);
            commit(&mut w, &mut publishes);
            assert!(
                w.store.bytes(&room).unwrap() <= 1000,
                "over the cap after a batch"
            );
        }
        assert!(w.store.floor(&room).unwrap() > 0, "nothing was trimmed");
        assert_eq!(w.store.next_seq(&room).unwrap(), 15);
    }

    /// Frames sprayed across many addresses, each well under its own cap,
    /// still cannot take the relay past its total cap: that is the bound on
    /// disk while anyone can publish anywhere.
    #[test]
    fn a_spray_across_addresses_stays_under_the_relay_cap() {
        let mut w = writer(Store::in_memory().unwrap());
        w.retention.relay_cap = 1000;
        for i in 0..50u32 {
            let address = addr(format!("made-up-{i}").as_bytes());
            let payload = [0u8; 100];
            let (mut publishes, _replies) = batch(&[(&address, &payload)]);
            commit(&mut w, &mut publishes);
            assert!(
                w.store.total_bytes() <= 1000,
                "over the relay cap after {i}"
            );
        }
        let newest = addr(b"made-up-49");
        assert_eq!(
            w.store.bytes(&newest).unwrap(),
            100,
            "the newest frame went first"
        );
    }

    /// Delivery is a promise that the frame is stored. A batch the store
    /// refused must reach nobody, and every publisher in it must hear so,
    /// or a crash could lose something a subscriber has already acted on.
    #[test]
    fn a_batch_that_cannot_be_stored_is_delivered_to_nobody() {
        let store = Store::in_memory().unwrap();
        store.fail_writes();
        let mut w = writer(store);
        let room = addr(b"room");
        let mut rx = subscriber(&w.routes, 0, &room);
        let (mut publishes, replies) = batch(&[(&room, b"a"), (&room, b"b"), (&room, b"c")]);

        commit(&mut w, &mut publishes);

        for mut reply in replies {
            assert!(matches!(
                reply.try_recv().unwrap(),
                Err(WriteError::Store(_))
            ));
        }
        assert!(rx.try_recv().is_err(), "an unstored frame was delivered");
    }
}
