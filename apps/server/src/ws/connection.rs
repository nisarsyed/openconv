use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use openconv_shared::ids::{DeviceId, GuildId, UserId};
use tokio::sync::mpsc;

use crate::state::AppState;

use super::types::{ClientMessage, ServerMessage};

const PING_INTERVAL: Duration = Duration::from_secs(30);
const MAX_MISSED_PONGS: u8 = 2;

/// Handle a single WebSocket connection after upgrade.
pub async fn handle_connection(
    socket: WebSocket,
    state: AppState,
    user_id: UserId,
    device_id: DeviceId,
) {
    let (mut ws_sender, ws_receiver) = socket.split();

    let guild_ids = match fetch_user_guild_ids(&state, user_id).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "failed to fetch guild memberships");
            let _ = ws_sender
                .send(Message::Close(Some(CloseFrame {
                    code: 1011,
                    reason: "internal error".into(),
                })))
                .await;
            return;
        }
    };

    // Close any existing connection for this (user_id, device_id) before registering
    if let Some(old) = state.ws.disconnect(user_id, device_id) {
        drop(old); // drop old sender, causing old send loop to exit
    }

    let (tx, rx) = mpsc::channel(256);
    // Send Ready message directly through the sender before registering
    let ready = ServerMessage::Ready {
        user_id,
        guild_ids: guild_ids.iter().copied().collect(),
    };
    if let Err(e) = tx.try_send(ready) {
        tracing::warn!(user_id = %user_id, error = %e, "failed to enqueue Ready message");
    }

    // Now register the connection in WsState
    state.ws.register_with_sender(user_id, device_id, guild_ids.clone(), tx);

    // Set up guild broadcast subscriptions and announce presence
    super::presence::setup_guild_subscriptions(&state, user_id, device_id, &guild_ids);
    super::presence::broadcast_connect(&state, user_id, &guild_ids);

    let pong_received = Arc::new(AtomicBool::new(true));

    let mut send_handle = tokio::spawn(send_loop(ws_sender, rx, pong_received.clone()));
    let mut recv_handle = tokio::spawn(recv_loop(
        ws_receiver,
        state.clone(),
        user_id,
        device_id,
        pong_received,
    ));

    // Wait for either task to finish, then abort the other
    tokio::select! {
        _ = &mut send_handle => {
            recv_handle.abort();
            tracing::debug!(user_id = %user_id, "send loop exited, aborting recv loop");
        }
        _ = &mut recv_handle => {
            send_handle.abort();
            tracing::debug!(user_id = %user_id, "recv loop exited, aborting send loop");
        }
    }

    // Cleanup
    cleanup_connection(&state, user_id, device_id).await;
}

async fn send_loop(
    mut ws_sender: SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<ServerMessage>,
    pong_received: Arc<AtomicBool>,
) {
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    ping_interval.tick().await; // skip immediate first tick
    let mut missed_pongs: u8 = 0;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(server_msg) => {
                        match serde_json::to_string(&server_msg) {
                            Ok(json) => {
                                if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "failed to serialize ServerMessage");
                            }
                        }
                    }
                    None => {
                        // Channel closed (shutdown or disconnect)
                        let _ = ws_sender.send(Message::Close(Some(CloseFrame {
                            code: 1001,
                            reason: "going away".into(),
                        }))).await;
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                if !pong_received.swap(false, Ordering::SeqCst) {
                    missed_pongs += 1;
                    if missed_pongs >= MAX_MISSED_PONGS {
                        tracing::info!("connection timed out: no pong received");
                        let _ = ws_sender.send(Message::Close(Some(CloseFrame {
                            code: 1001,
                            reason: "ping timeout".into(),
                        }))).await;
                        break;
                    }
                } else {
                    missed_pongs = 0;
                }
                if ws_sender.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn recv_loop(
    mut ws_receiver: futures::stream::SplitStream<WebSocket>,
    state: AppState,
    user_id: UserId,
    device_id: DeviceId,
    pong_received: Arc<AtomicBool>,
) {
    while let Some(result) = ws_receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(client_msg) => {
                        handle_client_message(&state, user_id, device_id, client_msg).await;
                    }
                    Err(_) => {
                        send_error(&state, user_id, device_id, 4004, "invalid message format");
                    }
                }
            }
            Ok(Message::Pong(_)) => {
                pong_received.store(true, Ordering::SeqCst);
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Ok(Message::Binary(_)) => {
                send_error(&state, user_id, device_id, 4004, "binary messages not supported");
            }
            Ok(Message::Ping(_)) => {
                // Axum auto-responds with Pong
            }
            Err(e) => {
                tracing::debug!(error = %e, "websocket receive error");
                break;
            }
        }
    }
}

async fn handle_client_message(
    state: &AppState,
    user_id: UserId,
    device_id: DeviceId,
    msg: ClientMessage,
) {
    match msg {
        ClientMessage::Ping { ts } => {
            let pong = ServerMessage::Pong { ts };
            send_to_connection(state, user_id, device_id, pong);
        }
        ClientMessage::SetPresence { status } => {
            super::presence::handle_set_presence(state, user_id, device_id, status);
        }
        ClientMessage::Subscribe { channel_id } => {
            super::fanout::handle_subscribe(state, user_id, device_id, channel_id).await;
        }
        ClientMessage::Unsubscribe { channel_id } => {
            super::fanout::handle_unsubscribe(state, user_id, device_id, channel_id);
        }
        ClientMessage::SendMessage {
            channel_id,
            encrypted_content,
            nonce,
        } => {
            super::fanout::handle_send_message(
                state,
                user_id,
                device_id,
                channel_id,
                encrypted_content,
                nonce,
            )
            .await;
        }
        ClientMessage::EditMessage {
            channel_id,
            message_id,
            encrypted_content,
            nonce,
        } => {
            super::fanout::handle_edit_message(
                state,
                user_id,
                device_id,
                channel_id,
                message_id,
                encrypted_content,
                nonce,
            )
            .await;
        }
        ClientMessage::DeleteMessage {
            channel_id,
            message_id,
        } => {
            super::fanout::handle_delete_message(state, user_id, device_id, channel_id, message_id)
                .await;
        }
        ClientMessage::StartTyping { channel_id } => {
            super::presence::handle_start_typing(state, user_id, channel_id);
        }
        ClientMessage::StopTyping { channel_id } => {
            super::presence::handle_stop_typing(state, user_id, channel_id);
        }
    }
}

pub(super) fn send_to_connection(state: &AppState, user_id: UserId, device_id: DeviceId, msg: ServerMessage) {
    if let Some(conn) = state.ws.connections.get(&(user_id, device_id)) {
        if let Err(e) = conn.sender.try_send(msg) {
            tracing::warn!(
                user_id = %user_id,
                device_id = %device_id,
                error = %e,
                "failed to send message to connection (channel full or closed)"
            );
        }
    }
}

pub(super) fn send_error(state: &AppState, user_id: UserId, device_id: DeviceId, code: u32, message: &str) {
    let err = ServerMessage::Error {
        code,
        message: message.to_string(),
    };
    send_to_connection(state, user_id, device_id, err);
}

async fn cleanup_connection(state: &AppState, user_id: UserId, device_id: DeviceId) {
    if let Some(conn) = state.ws.disconnect(user_id, device_id) {
        // Store last_seen timestamps for message replay on reconnect
        super::replay::store_last_seen(&state.redis, user_id, &conn.subscribed_channels).await;

        // Clean up channel broadcast senders with zero receivers
        for channel_id in &conn.subscribed_channels {
            state.ws.try_cleanup_channel(channel_id);
        }

        // Broadcast offline presence to guild members
        super::presence::broadcast_disconnect(state, user_id, &conn.guild_ids);

        tracing::info!(
            user_id = %user_id,
            device_id = %device_id,
            "connection cleaned up"
        );
    }
}

async fn fetch_user_guild_ids(
    state: &AppState,
    user_id: UserId,
) -> Result<std::collections::HashSet<GuildId>, sqlx::Error> {
    let rows: Vec<GuildId> = sqlx::query_scalar(
        "SELECT guild_id FROM guild_members WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_interval_is_30_seconds() {
        assert_eq!(PING_INTERVAL, Duration::from_secs(30));
    }

    #[test]
    fn max_missed_pongs_is_2() {
        assert_eq!(MAX_MISSED_PONGS, 2);
    }

    #[test]
    fn pong_received_atomic_flag_works() {
        let flag = Arc::new(AtomicBool::new(true));
        assert!(flag.swap(false, Ordering::SeqCst));
        assert!(!flag.load(Ordering::SeqCst));
        flag.store(true, Ordering::SeqCst);
        assert!(flag.load(Ordering::SeqCst));
    }
}
