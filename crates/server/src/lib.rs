//! OpenConv relay.
//!
//! The relay fans out opaque frames to every client on the channel. It never
//! parses, decrypts, or stores payloads: as far as this process is concerned
//! the traffic is uninterpreted bytes. Keeping it that way is the entire
//! privacy argument, so resist adding anything that inspects a frame.
//!
//! Its one semantic job is ordering. Every accepted frame gets the next
//! sequence number, and every client sees frames in that same order. MLS
//! needs a single winner when two members commit at the same epoch, and this
//! is what decides it — without the relay ever knowing which frames are
//! commits.
//!
//! Frames are echoed back to their sender too, so a sender learns where its
//! own frame landed in the order. Clients recognise their own frames by
//! content, which keeps sender identity off the wire.

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::broadcast;

/// Frames in flight before a slow client starts missing traffic.
const BACKLOG: usize = 256;

#[derive(Clone)]
struct Relay {
    /// Every connection subscribes and receives every frame, sender included.
    tx: broadcast::Sender<Vec<u8>>,
    /// The ordering authority for the channel.
    ///
    /// A mutex rather than an atomic counter, because numbering a frame and
    /// broadcasting it have to happen together. With a counter the two steps
    /// can interleave: two concurrent frames get numbers 4 and 5, then the
    /// sends race and every client receives 5 before 4. Clients then disagree
    /// about which commit won its epoch, which is the one thing this relay
    /// exists to decide.
    seq: Arc<Mutex<u64>>,
}

/// Width of the sequence prefix the relay prepends to each frame.
const SEQ_LEN: usize = 8;

/// Prepend the sequence number. Deliberately does not look past it.
fn stamp(seq: u64, frame: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(SEQ_LEN + frame.len());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(frame);
    out
}

/// Build the relay router. Exposed so tests can serve it on an ephemeral port.
pub fn router() -> Router {
    let (tx, _) = broadcast::channel(BACKLOG);
    let relay = Relay {
        tx,
        seq: Arc::new(Mutex::new(0)),
    };
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(upgrade))
        .with_state(relay)
}

async fn upgrade(ws: WebSocketUpgrade, State(relay): State<Relay>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle(socket, relay))
}

async fn handle(socket: WebSocket, relay: Relay) {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut rx = relay.tx.subscribe();
    let (mut sink, mut stream) = socket.split();
    tracing::info!(id, "client connected");

    // Outbound: every frame on the channel, in sequence order, including this
    // client's own. A sender needs its own frames back to learn where they
    // landed relative to everyone else's.
    let mut outbound = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(bytes) => {
                    if sink.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                // A client too slow to keep up has missed frames and can no
                // longer trust its view of the order. Drop it rather than
                // let it silently diverge; the sequence gap would otherwise
                // surface as an undecryptable message much later.
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(id, missed, "client lagged; disconnecting");
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Inbound: assign a sequence number and fan out, unparsed.
    let tx = relay.tx.clone();
    let seq = relay.seq.clone();
    let mut inbound = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            match msg {
                Message::Binary(bytes) => {
                    // Number and broadcast under one lock so delivery order
                    // matches numbering. `send` only pushes into the channel
                    // and does not await, so this never holds across a yield.
                    let n = {
                        let mut seq = seq.lock().expect("relay sequence poisoned");
                        let n = *seq;
                        *seq += 1;
                        let _ = tx.send(stamp(n, &bytes));
                        n
                    };
                    tracing::debug!(id, seq = n, bytes = bytes.len(), "relayed");
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Either direction ending tears down the connection.
    tokio::select! {
        _ = &mut outbound => inbound.abort(),
        _ = &mut inbound => outbound.abort(),
    }
    tracing::info!(id, "client disconnected");
}
