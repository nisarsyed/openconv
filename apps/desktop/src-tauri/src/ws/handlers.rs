use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use openconv_shared::api::ws::{ClientMessage, ServerMessage};
use tauri::AppHandle;
use tokio_tungstenite::tungstenite::Message;

use super::events;
use super::state::{WsConnectionState, WsState};

/// Read incoming WebSocket messages and dispatch them.
///
/// Runs until the WebSocket stream ends, errors, or `shutdown_notify` is signalled.
/// On `Ready`, transitions state to `Authenticated` and re-subscribes to tracked channels.
/// On `Pong`, updates `last_pong` timestamp.
/// All other messages are dispatched via `events::dispatch_server_message`.
pub async fn recv_loop(
    app: AppHandle,
    ws_state: WsState,
    mut read_stream: impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
    shutdown_notify: Arc<tokio::sync::Notify>,
) {
    loop {
        tokio::select! {
            _ = shutdown_notify.notified() => {
                tracing::debug!("recv_loop: shutdown signal received");
                break;
            }
            msg = read_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_text_message(&app, &ws_state, &text).await;
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("recv_loop: server sent close frame");
                        break;
                    }
                    Some(Ok(_)) => {
                        // Ignore binary, ping, pong frames
                    }
                    Some(Err(e)) => {
                        tracing::warn!("recv_loop: WebSocket error: {e}");
                        break;
                    }
                    None => {
                        tracing::info!("recv_loop: WebSocket stream ended");
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_text_message(app: &AppHandle, ws_state: &WsState, text: &str) {
    let server_msg: ServerMessage = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(e) => {
            tracing::warn!("recv_loop: failed to deserialize ServerMessage: {e}");
            return;
        }
    };

    match &server_msg {
        ServerMessage::Ready { user_id, guild_ids } => {
            tracing::info!(
                "recv_loop: Ready - user_id={user_id}, guilds={}",
                guild_ids.len()
            );
            {
                let mut state = ws_state.connection_state.write().await;
                *state = WsConnectionState::Authenticated;
                events::emit_state(app, &state);
            }

            // Re-subscribe to all tracked channels now that we're authenticated
            let channels = ws_state.subscribed_channels.read().await;
            let tx_guard = ws_state.outgoing_tx.read().await;
            if let Some(tx) = tx_guard.as_ref() {
                for channel_id in channels.iter() {
                    let _ = tx.send(ClientMessage::Subscribe {
                        channel_id: *channel_id,
                    });
                }
            }
        }
        ServerMessage::Pong { ts: _ } => {
            let mut last_pong = ws_state.last_pong.write().await;
            *last_pong = Instant::now();
        }
        _ => {
            events::dispatch_server_message(app, &server_msg);
        }
    }
}

/// Calculate reconnection backoff delay in milliseconds.
/// Formula: min(500ms * 2^attempt + jitter, 60_000ms)
pub fn backoff_delay_ms(attempt: u32) -> u64 {
    let base = 500u64.saturating_mul(1u64 << attempt.min(20));
    let jitter = rand::random::<u64>() % 500;
    base.saturating_add(jitter).min(60_000)
}

/// Derive a WebSocket URL from an HTTP base URL.
/// `https://` becomes `wss://`, `http://` becomes `ws://`.
/// Appends `/ws?ticket={ticket}` to the path.
pub fn derive_ws_url(api_base_url: &str, ticket: &str) -> String {
    let ws_base = if api_base_url.starts_with("https://") {
        api_base_url.replacen("https://", "wss://", 1)
    } else if api_base_url.starts_with("http://") {
        api_base_url.replacen("http://", "ws://", 1)
    } else {
        format!("ws://{api_base_url}")
    };

    let ws_base = ws_base.trim_end_matches('/');
    format!("{ws_base}/ws?ticket={ticket}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- backoff_delay_ms tests ---

    #[test]
    fn backoff_attempt_0_is_in_range() {
        for _ in 0..100 {
            let delay = backoff_delay_ms(0);
            assert!(delay >= 500, "delay {delay} < 500");
            assert!(delay < 1000, "delay {delay} >= 1000");
        }
    }

    #[test]
    fn backoff_attempt_1_is_in_range() {
        for _ in 0..100 {
            let delay = backoff_delay_ms(1);
            assert!(delay >= 1000, "delay {delay} < 1000");
            assert!(delay < 1500, "delay {delay} >= 1500");
        }
    }

    #[test]
    fn backoff_attempt_2_is_in_range() {
        for _ in 0..100 {
            let delay = backoff_delay_ms(2);
            assert!(delay >= 2000, "delay {delay} < 2000");
            assert!(delay < 2500, "delay {delay} >= 2500");
        }
    }

    #[test]
    fn backoff_high_attempt_capped_at_60s() {
        for attempt in 6..25 {
            for _ in 0..20 {
                let delay = backoff_delay_ms(attempt);
                assert!(delay <= 60_000, "attempt {attempt}: delay {delay} > 60000");
            }
        }
    }

    #[test]
    fn backoff_very_high_attempt_does_not_overflow() {
        let delay = backoff_delay_ms(u32::MAX);
        assert!(delay <= 60_000);
    }

    // --- derive_ws_url tests ---

    #[test]
    fn derive_ws_url_converts_https_to_wss() {
        let url = derive_ws_url("https://api.example.com", "ticket123");
        assert_eq!(url, "wss://api.example.com/ws?ticket=ticket123");
    }

    #[test]
    fn derive_ws_url_converts_http_to_ws() {
        let url = derive_ws_url("http://localhost:3000", "abc");
        assert_eq!(url, "ws://localhost:3000/ws?ticket=abc");
    }

    #[test]
    fn derive_ws_url_strips_trailing_slash() {
        let url = derive_ws_url("https://api.example.com/", "t1");
        assert_eq!(url, "wss://api.example.com/ws?ticket=t1");
    }

    #[test]
    fn derive_ws_url_handles_plain_host() {
        let url = derive_ws_url("localhost:3000", "t1");
        assert_eq!(url, "ws://localhost:3000/ws?ticket=t1");
    }
}
