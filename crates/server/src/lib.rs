//! OpenConv relay.
//!
//! The relay fans out opaque frames to every other client on the channel.
//! It never parses, decrypts, or stores payloads: as far as this process is
//! concerned the traffic is uninterpreted bytes. Keeping it that way is the
//! entire privacy argument, so resist adding anything that inspects a frame.

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
use tokio::sync::broadcast;

/// Frames in flight before a slow client starts missing traffic.
const BACKLOG: usize = 256;

#[derive(Clone)]
struct Relay {
    /// Every connection subscribes; senders skip their own frames by id.
    tx: broadcast::Sender<(usize, Vec<u8>)>,
}

/// Build the relay router. Exposed so tests can serve it on an ephemeral port.
pub fn router() -> Router {
    let (tx, _) = broadcast::channel(BACKLOG);
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(upgrade))
        .with_state(Relay { tx })
}

async fn upgrade(ws: WebSocketUpgrade, State(relay): State<Relay>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle(socket, relay))
}

async fn handle(socket: WebSocket, relay: Relay) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut rx = relay.tx.subscribe();
    let (mut sink, mut stream) = socket.split();
    tracing::info!(id, "client connected");

    // Outbound: everyone else's frames.
    let mut outbound = tokio::spawn(async move {
        while let Ok((from, bytes)) = rx.recv().await {
            if from == id {
                continue; // don't echo a sender its own frame
            }
            if sink.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });

    // Inbound: fan out, unparsed.
    let tx = relay.tx.clone();
    let mut inbound = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            match msg {
                Message::Binary(bytes) => {
                    tracing::debug!(id, bytes = bytes.len(), "relaying");
                    let _ = tx.send((id, bytes.to_vec()));
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
