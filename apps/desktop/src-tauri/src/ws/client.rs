use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use openconv_shared::api::ws::ClientMessage;
use openconv_shared::ids::ChannelId;
use tauri::{AppHandle, State};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::auth_service::{self, AppError};

use super::events;
use super::handlers;
use super::state::{WsConnectionState, WsState};

const MAX_RETRIES: u32 = 20;
const HEARTBEAT_INTERVAL_SECS: u64 = 30;
const PONG_TIMEOUT_SECS: u64 = 90;

/// Obtain a WebSocket ticket from the server.
async fn obtain_ws_ticket(
    http_client: &reqwest::Client,
    api_base_url: &str,
    access_token: &str,
) -> Result<String, AppError> {
    let resp = http_client
        .post(format!(
            "{}/api/ws/ticket",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(access_token)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::new("authentication required"));
    }

    if !resp.status().is_success() {
        return Err(AppError::new(format!(
            "failed to obtain WS ticket (HTTP {})",
            resp.status()
        )));
    }

    #[derive(serde::Deserialize)]
    struct TicketResponse {
        ticket: String,
    }

    let data: TicketResponse = resp
        .json()
        .await
        .map_err(|e| AppError::new(format!("failed to parse ticket response: {e}")))?;

    Ok(data.ticket)
}

/// Initiate WebSocket connection. Obtains a ticket and connects.
#[tauri::command]
#[specta::specta]
pub async fn ws_connect(app: AppHandle, ws_state: State<'_, WsState>) -> Result<(), AppError> {
    // Abort any existing connection task
    {
        let mut task = ws_state.connection_task.write().await;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }

    // Set state to Connecting
    {
        let mut state = ws_state.connection_state.write().await;
        *state = WsConnectionState::Connecting { attempt: 0 };
        events::emit_state(&app, &state);
    }

    let ws_state_inner = ws_state.inner().clone();

    let app_clone = app.clone();
    let task_handle = tokio::spawn(async move {
        connection_loop(app_clone, ws_state_inner).await;
    });

    let mut task = ws_state.connection_task.write().await;
    *task = Some(task_handle);

    Ok(())
}

/// The main connection loop that handles connecting, receiving, and reconnection.
async fn connection_loop(app: AppHandle, ws_state: WsState) {
    let api_base_url = ws_state.api_base_url.read().await.clone();
    let http_client = ws_state.http_client.clone();
    let mut attempt: u32 = 0;

    loop {
        // Get access token (blocking OS keyring call via spawn_blocking)
        let access_token = match tokio::task::spawn_blocking(auth_service::get_access_token).await {
            Ok(Ok(token)) => token,
            Ok(Err(e)) => {
                tracing::error!("ws: failed to get access token: {e}");
                let mut state = ws_state.connection_state.write().await;
                *state = WsConnectionState::Failed {
                    reason: "no auth token available".into(),
                };
                events::emit_state(&app, &state);
                return;
            }
            Err(e) => {
                tracing::error!("ws: spawn_blocking failed: {e}");
                let mut state = ws_state.connection_state.write().await;
                *state = WsConnectionState::Failed {
                    reason: "internal error".into(),
                };
                events::emit_state(&app, &state);
                return;
            }
        };

        // Obtain WS ticket
        let ticket = match obtain_ws_ticket(&http_client, &api_base_url, &access_token).await {
            Ok(t) => t,
            Err(e) => {
                // Fail immediately on auth error (don't waste retries)
                if e.message == "authentication required" {
                    let mut state = ws_state.connection_state.write().await;
                    *state = WsConnectionState::Failed {
                        reason: "authentication required - please log in again".into(),
                    };
                    events::emit_state(&app, &state);
                    return;
                }

                tracing::warn!("ws: failed to obtain ticket: {e}");
                if attempt >= MAX_RETRIES {
                    let mut state = ws_state.connection_state.write().await;
                    *state = WsConnectionState::Failed {
                        reason: format!("failed after {MAX_RETRIES} retries: {e}"),
                    };
                    events::emit_state(&app, &state);
                    return;
                }
                let delay = handlers::backoff_delay_ms(attempt);
                {
                    let mut state = ws_state.connection_state.write().await;
                    *state = WsConnectionState::Reconnecting {
                        attempt,
                        next_retry_ms: delay,
                    };
                    events::emit_state(&app, &state);
                }
                tokio::time::sleep(Duration::from_millis(delay)).await;
                attempt += 1;
                continue;
            }
        };

        // Derive WS URL and connect
        let ws_url = handlers::derive_ws_url(&api_base_url, &ticket);
        tracing::info!("ws: connecting to {ws_url}");

        let ws_stream = match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((stream, _response)) => stream,
            Err(e) => {
                tracing::warn!("ws: connection failed: {e}");
                if attempt >= MAX_RETRIES {
                    let mut state = ws_state.connection_state.write().await;
                    *state = WsConnectionState::Failed {
                        reason: format!("connection failed after {MAX_RETRIES} retries"),
                    };
                    events::emit_state(&app, &state);
                    return;
                }
                let delay = handlers::backoff_delay_ms(attempt);
                {
                    let mut state = ws_state.connection_state.write().await;
                    *state = WsConnectionState::Reconnecting {
                        attempt,
                        next_retry_ms: delay,
                    };
                    events::emit_state(&app, &state);
                }
                tokio::time::sleep(Duration::from_millis(delay)).await;
                attempt += 1;
                continue;
            }
        };

        // Connected - split the stream
        {
            let mut state = ws_state.connection_state.write().await;
            *state = WsConnectionState::Connected;
            events::emit_state(&app, &state);
        }

        let (ws_write, ws_read) = ws_stream.split();

        // Set up outgoing message channel
        let (tx, mut rx) = mpsc::unbounded_channel::<ClientMessage>();
        {
            let mut outgoing = ws_state.outgoing_tx.write().await;
            *outgoing = Some(tx);
        }

        // Reset last_pong
        {
            let mut last_pong = ws_state.last_pong.write().await;
            *last_pong = std::time::Instant::now();
        }

        // Shutdown signal for recv_loop (shared so heartbeat can also trigger it)
        let shutdown_notify = std::sync::Arc::new(tokio::sync::Notify::new());

        // Spawn recv_loop
        let recv_app = app.clone();
        let recv_ws_state = ws_state.clone();
        let recv_shutdown = shutdown_notify.clone();

        let recv_handle = tokio::spawn(async move {
            handlers::recv_loop(recv_app, recv_ws_state, ws_read, recv_shutdown).await;
        });

        // Spawn write loop
        let write_handle = tokio::spawn(async move {
            let mut ws_write = ws_write;
            while let Some(msg) = rx.recv().await {
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        if let Err(e) = ws_write.send(Message::Text(json)).await {
                            tracing::warn!("ws: write error: {e}");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("ws: failed to serialize outgoing message: {e}");
                    }
                }
            }
        });

        // Spawn heartbeat loop
        let heartbeat_tx = ws_state.outgoing_tx.clone();
        let heartbeat_last_pong = ws_state.last_pong.clone();
        let heartbeat_shutdown = shutdown_notify.clone();
        let heartbeat_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;

                // Check pong timeout
                let elapsed = {
                    let last = heartbeat_last_pong.read().await;
                    last.elapsed()
                };
                if elapsed > Duration::from_secs(PONG_TIMEOUT_SECS) {
                    tracing::warn!("ws: pong timeout ({}s)", elapsed.as_secs());
                    // Signal recv_loop to shut down
                    heartbeat_shutdown.notify_one();
                    break;
                }

                // Send ping
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let tx_guard = heartbeat_tx.read().await;
                if let Some(tx) = tx_guard.as_ref() {
                    if tx.send(ClientMessage::Ping { ts }).is_err() {
                        break;
                    }
                } else {
                    break;
                }
            }
        });

        // Wait for recv_loop to complete (connection dropped or shutdown signalled)
        let _ = recv_handle.await;

        // Clean up
        heartbeat_handle.abort();
        write_handle.abort();
        {
            let mut outgoing = ws_state.outgoing_tx.write().await;
            *outgoing = None;
        }

        // Check if we were intentionally disconnected
        {
            let state = ws_state.connection_state.read().await;
            if matches!(*state, WsConnectionState::Disconnected) {
                return;
            }
        }

        // Connection dropped - start reconnection
        attempt = 0;
        let delay = handlers::backoff_delay_ms(attempt);
        {
            let mut state = ws_state.connection_state.write().await;
            *state = WsConnectionState::Reconnecting {
                attempt,
                next_retry_ms: delay,
            };
            events::emit_state(&app, &state);
        }
        tokio::time::sleep(Duration::from_millis(delay)).await;
        attempt += 1;
    }
}

/// Gracefully disconnect the WebSocket.
#[tauri::command]
#[specta::specta]
pub async fn ws_disconnect(app: AppHandle, ws_state: State<'_, WsState>) -> Result<(), AppError> {
    // Abort the connection task first to prevent race conditions
    {
        let mut task = ws_state.connection_task.write().await;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }

    // Then set state to Disconnected
    {
        let mut state = ws_state.connection_state.write().await;
        *state = WsConnectionState::Disconnected;
        events::emit_state(&app, &state);
    }

    // Clear outgoing channel
    {
        let mut outgoing = ws_state.outgoing_tx.write().await;
        *outgoing = None;
    }

    Ok(())
}

/// Query current connection state.
#[tauri::command]
#[specta::specta]
pub async fn ws_get_state(ws_state: State<'_, WsState>) -> Result<WsConnectionState, AppError> {
    let state = ws_state.connection_state.read().await;
    Ok(state.clone())
}

/// Subscribe to a channel's real-time events.
#[tauri::command]
#[specta::specta]
pub async fn ws_subscribe(
    ws_state: State<'_, WsState>,
    channel_id: String,
) -> Result<(), AppError> {
    let channel_id: ChannelId = channel_id
        .parse()
        .map_err(|_| AppError::new("invalid channel_id"))?;

    // Track the subscription
    {
        let mut channels = ws_state.subscribed_channels.write().await;
        channels.insert(channel_id);
    }

    // Send subscribe message if connected
    let tx_guard = ws_state.outgoing_tx.read().await;
    if let Some(tx) = tx_guard.as_ref() {
        let _ = tx.send(ClientMessage::Subscribe { channel_id });
    }

    Ok(())
}

/// Unsubscribe from a channel's events.
#[tauri::command]
#[specta::specta]
pub async fn ws_unsubscribe(
    ws_state: State<'_, WsState>,
    channel_id: String,
) -> Result<(), AppError> {
    let channel_id: ChannelId = channel_id
        .parse()
        .map_err(|_| AppError::new("invalid channel_id"))?;

    // Remove from tracked subscriptions
    {
        let mut channels = ws_state.subscribed_channels.write().await;
        channels.remove(&channel_id);
    }

    // Send unsubscribe message if connected
    let tx_guard = ws_state.outgoing_tx.read().await;
    if let Some(tx) = tx_guard.as_ref() {
        let _ = tx.send(ClientMessage::Unsubscribe { channel_id });
    }

    Ok(())
}

/// Send a typing indicator for a channel.
#[tauri::command]
#[specta::specta]
pub async fn ws_send_typing(
    ws_state: State<'_, WsState>,
    channel_id: String,
) -> Result<(), AppError> {
    let channel_id: ChannelId = channel_id
        .parse()
        .map_err(|_| AppError::new("invalid channel_id"))?;

    let tx_guard = ws_state.outgoing_tx.read().await;
    if let Some(tx) = tx_guard.as_ref() {
        let _ = tx.send(ClientMessage::StartTyping { channel_id });
    }
    Ok(())
}
