use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use openconv_shared::api::ws::{ClientMessage, ServerMessage};
use openconv_shared::ids::{ChannelId, DeviceId, DmChannelId, MessageId, UserId};
use tauri::{AppHandle, Manager};
use tokio_tungstenite::tungstenite::Message;

use crate::auth_service;
use crate::cache::messages::{self, CachedMessage};
use crate::cache::read_positions;
use crate::cache::search;
use crate::cache::CacheDb;
use crate::crypto_service::CryptoState;
use crate::notification_service::{self, NotificationState, VisibleChannelState};

use super::events::{self, ChannelPayload, GuildPayload, WsReadyDataPayload};
use super::state::{WsConnectionState, WsState};

/// Message status constants.
pub const MSG_STATUS_PENDING: &str = "pending";
pub const MSG_STATUS_DELIVERED: &str = "delivered";
pub const MSG_STATUS_DECRYPT_FAILED: &str = "decrypt_failed";
pub const MSG_STATUS_QUEUED: &str = "queued";
#[allow(dead_code)]
pub const MSG_STATUS_DELETED: &str = "deleted";

/// Read incoming WebSocket messages and dispatch them.
///
/// Runs until the WebSocket stream ends, errors, or `shutdown_notify` is signalled.
/// On `Ready`, transitions state to `Authenticated` and re-subscribes to tracked channels.
/// On `Pong`, updates `last_pong` timestamp.
/// Message variants (Created/Updated/Deleted) go through the decrypt-store-emit pipeline.
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
                let mut uid = ws_state.current_user_id.write().await;
                *uid = Some(*user_id);
            }
            // Load device_id from local_device table so it's available for crypto operations
            {
                let loaded_device_id = {
                    let cache_db = app.state::<CacheDb>();
                    cache_db
                        .lock()
                        .map_err(|e| e.to_string())
                        .and_then(|conn| {
                            auth_service::get_or_create_device_id(&conn).map_err(|e| e.to_string())
                        })
                        .map(|(did, _name)| did)
                };
                match loaded_device_id {
                    Ok(device_id) => {
                        let mut did = ws_state.current_device_id.write().await;
                        *did = Some(device_id);
                    }
                    Err(e) => {
                        // Without a device id nothing can be encrypted for this
                        // device, so surface the reason rather than the bare
                        // fact of failure — a swallowed error here left the id
                        // unset and every send failing with no explanation.
                        tracing::error!("failed to load device_id from cache DB: {e}");
                    }
                }
            }
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

            // Fetch guild/channel data in the background and emit to frontend
            let app_clone = app.clone();
            let ws_clone = ws_state.clone();
            tokio::spawn(async move {
                if let Err(e) = fetch_and_emit_ready_data(&app_clone, &ws_clone).await {
                    tracing::warn!("failed to fetch ready data: {e}");
                }
            });
        }
        ServerMessage::Pong { ts: _ } => {
            let mut last_pong = ws_state.last_pong.write().await;
            *last_pong = Instant::now();
        }
        ServerMessage::MessageCreated {
            channel_id,
            message_id,
            sender_id,
            sender_device_id,
            sender_signal_device_id,
            ciphertext,
            message_type,
            created_at,
            client_nonce,
        } => {
            handle_message_created(
                app,
                ws_state,
                IncomingMessage {
                    channel_id: *channel_id,
                    message_id: *message_id,
                    sender_id: *sender_id,
                    sender_device_id: *sender_device_id,
                    sender_signal_device_id: *sender_signal_device_id,
                    ciphertext,
                    message_type,
                    timestamp: created_at,
                },
                client_nonce.as_deref(),
            )
            .await;
        }
        ServerMessage::MessageUpdated {
            channel_id,
            message_id,
            sender_id,
            sender_device_id,
            sender_signal_device_id,
            ciphertext,
            message_type,
            edited_at,
        } => {
            handle_message_updated(
                app,
                IncomingMessage {
                    channel_id: *channel_id,
                    message_id: *message_id,
                    sender_id: *sender_id,
                    sender_device_id: *sender_device_id,
                    sender_signal_device_id: *sender_signal_device_id,
                    ciphertext,
                    message_type,
                    timestamp: edited_at,
                },
            )
            .await;
        }
        ServerMessage::DmMessageCreated {
            dm_channel_id,
            message_id,
            sender_id,
            sender_device_id,
            sender_signal_device_id,
            ciphertext,
            message_type,
            created_at,
        } => {
            handle_dm_message_created(
                app,
                IncomingDmMessage {
                    dm_channel_id: *dm_channel_id,
                    message_id: *message_id,
                    sender_id: *sender_id,
                    sender_device_id: *sender_device_id,
                    sender_signal_device_id: *sender_signal_device_id,
                    ciphertext,
                    message_type,
                    created_at,
                },
            )
            .await;
        }
        ServerMessage::MessageDeleted {
            channel_id,
            message_id,
        } => {
            handle_message_deleted(app, *channel_id, *message_id);
        }
        _ => {
            events::dispatch_server_message(app, &server_msg);
        }
    }
}

/// Fetch user profile, guilds, and channels from the server, then emit `ws:ready_data`.
async fn fetch_and_emit_ready_data(
    app: &AppHandle,
    ws_state: &WsState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let api_base_url = ws_state.api_base_url.read().await.clone();
    let access_token = tokio::task::spawn_blocking(auth_service::get_access_token)
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| format!("not authenticated: {e}"))?;

    let client = &ws_state.http_client;

    // 1. Fetch user profile
    let me_resp = client
        .get(format!("{api_base_url}/api/users/me"))
        .bearer_auth(&access_token)
        .send()
        .await?;
    if !me_resp.status().is_success() {
        return Err(format!("GET /api/users/me returned {}", me_resp.status()).into());
    }
    let me: serde_json::Value = me_resp.json().await?;

    let user_id_str = me["id"].as_str().unwrap_or_default();
    let display_name = me["display_name"].as_str().unwrap_or_default().to_string();
    let email = me["email"].as_str().unwrap_or_default().to_string();
    let avatar_url = me["avatar_url"].as_str().map(|s| s.to_string());

    // 2. Fetch guilds
    let guilds_resp = client
        .get(format!("{api_base_url}/api/guilds"))
        .bearer_auth(&access_token)
        .send()
        .await?;
    if !guilds_resp.status().is_success() {
        return Err(format!("GET /api/guilds returned {}", guilds_resp.status()).into());
    }
    let guilds_body: serde_json::Value = guilds_resp.json().await?;
    let guild_list = guilds_body["guilds"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // 3. For each guild, fetch channels
    let mut guild_payloads = Vec::with_capacity(guild_list.len());
    for g in &guild_list {
        let gid = g["id"].as_str().unwrap_or_default();
        let channels_resp = client
            .get(format!("{api_base_url}/api/guilds/{gid}/channels"))
            .bearer_auth(&access_token)
            .send()
            .await?;

        let channel_list: Vec<serde_json::Value> = if channels_resp.status().is_success() {
            channels_resp.json().await.unwrap_or_default()
        } else {
            tracing::warn!(
                "GET /api/guilds/{gid}/channels returned {}",
                channels_resp.status()
            );
            vec![]
        };

        let channels: Vec<ChannelPayload> = channel_list
            .iter()
            .filter_map(|c| {
                Some(ChannelPayload {
                    id: c["id"].as_str()?.parse().ok()?,
                    guild_id: c["guild_id"].as_str()?.parse().ok()?,
                    name: c["name"].as_str()?.to_string(),
                    channel_type: c["channel_type"].as_str().unwrap_or("text").to_string(),
                    position: c["position"].as_i64().unwrap_or(0) as i32,
                })
            })
            .collect();

        guild_payloads.push(GuildPayload {
            id: gid
                .parse()
                .map_err(|_| format!("invalid guild id: {gid}"))?,
            name: g["name"].as_str().unwrap_or_default().to_string(),
            owner_id: g["owner_id"]
                .as_str()
                .unwrap_or_default()
                .parse()
                .map_err(|_| "invalid owner_id")?,
            icon_url: g["icon_url"].as_str().map(|s| s.to_string()),
            channels,
        });
    }

    // 4. Emit
    let payload = WsReadyDataPayload {
        user_id: user_id_str
            .parse()
            .map_err(|_| "invalid user_id in profile")?,
        display_name,
        email,
        avatar_url,
        guilds: guild_payloads,
    };

    events::emit_ready_data(app, &payload);
    tracing::info!("emitted ws:ready_data with {} guilds", payload.guilds.len());

    Ok(())
}

/// The message fields carried by both `MessageCreated` and `MessageUpdated`
/// server frames.
///
/// These travel together and are always consumed together, so they are grouped
/// rather than threaded through as eight positional parameters. `timestamp` is
/// `created_at` for a new message and `edited_at` for an update.
struct IncomingMessage<'a> {
    channel_id: ChannelId,
    message_id: MessageId,
    sender_id: UserId,
    sender_device_id: DeviceId,
    /// Signal protocol device id of the sender — the `device_id` half of the
    /// `ProtocolAddress` this message must be decrypted against.
    sender_signal_device_id: u32,
    ciphertext: &'a [u8],
    message_type: &'a str,
    timestamp: &'a chrono::DateTime<chrono::Utc>,
}

/// Decrypt one incoming ciphertext on the blocking pool.
///
/// Returns the plaintext and the status to record. A decrypt failure is not an
/// error here: the message is still stored so the user sees a placeholder and
/// `retry_decrypt` can rebuild the session and try again.
async fn decrypt_incoming(
    app: &AppHandle,
    sender_id: &str,
    signal_device_id: u32,
    ciphertext: Vec<u8>,
    message_type: String,
    msg_id_for_log: &str,
) -> (Option<String>, &'static str) {
    let sender_id_owned = sender_id.to_string();
    let app_clone = app.clone();
    let decrypt_result = tokio::task::spawn_blocking(move || {
        let crypto_state = app_clone.state::<CryptoState>();
        crypto_state.crypto_service.decrypt_message(
            &sender_id_owned,
            signal_device_id,
            &ciphertext,
            &message_type,
        )
    })
    .await;

    match decrypt_result {
        Ok(Ok(plaintext_bytes)) => {
            let pt = String::from_utf8_lossy(&plaintext_bytes).to_string();
            (Some(pt), MSG_STATUS_DELIVERED)
        }
        Ok(Err(e)) => {
            tracing::warn!("decrypt failed for message {msg_id_for_log}: {e}");
            (None, MSG_STATUS_DECRYPT_FAILED)
        }
        Err(e) => {
            tracing::error!("spawn_blocking failed for message {msg_id_for_log}: {e}");
            (None, MSG_STATUS_DECRYPT_FAILED)
        }
    }
}

/// The fields carried by a `DmMessageCreated` server frame.
///
/// Grouped for the same reason as [`IncomingMessage`]: they travel and are
/// consumed together, and passing them positionally puts the function over
/// clippy's argument limit.
struct IncomingDmMessage<'a> {
    dm_channel_id: DmChannelId,
    message_id: MessageId,
    sender_id: UserId,
    sender_device_id: DeviceId,
    sender_signal_device_id: u32,
    ciphertext: &'a [u8],
    message_type: &'a str,
    created_at: &'a chrono::DateTime<chrono::Utc>,
}

/// Decrypt an incoming DM, store it against its DM channel, and emit.
///
/// Mirrors `handle_message_created` but writes `dm_channel_id` instead of
/// `channel_id`. DMs carry no guild, so there is no guild lookup and no
/// per-channel unread counter — unread for DMs is tracked by the DM list.
async fn handle_dm_message_created(app: &AppHandle, msg: IncomingDmMessage<'_>) {
    let IncomingDmMessage {
        dm_channel_id,
        message_id,
        sender_id,
        sender_device_id,
        sender_signal_device_id,
        ciphertext,
        message_type,
        created_at,
    } = msg;

    let msg_id_str = message_id.to_string();
    let dm_channel_id_str = dm_channel_id.to_string();

    let (plaintext, status) = decrypt_incoming(
        app,
        &sender_id.to_string(),
        sender_signal_device_id,
        ciphertext.to_vec(),
        message_type.to_string(),
        &msg_id_str,
    )
    .await;

    let now_ms = chrono::Utc::now().timestamp_millis();
    let cached = CachedMessage {
        id: msg_id_str.clone(),
        channel_id: None,
        dm_channel_id: Some(dm_channel_id_str.clone()),
        sender_id: sender_id.to_string(),
        sender_device_id: Some(sender_device_id.to_string()),
        sender_signal_device_id: Some(sender_signal_device_id),
        plaintext: plaintext.clone(),
        ciphertext: if status == MSG_STATUS_DECRYPT_FAILED {
            Some(ciphertext.to_vec())
        } else {
            None
        },
        message_type: Some(message_type.to_string()),
        created_at: created_at.timestamp_millis(),
        edited_at: None,
        decrypted_at: if plaintext.is_some() {
            Some(now_ms)
        } else {
            None
        },
        status: status.into(),
    };

    if let Ok(conn) = app.state::<CacheDb>().lock() {
        if let Err(e) = messages::insert_message(&conn, &cached) {
            tracing::warn!("failed to cache DM message {msg_id_str}: {e}");
        }
        if let Some(ref pt) = plaintext {
            if let Err(e) = search::index_message(&conn, &msg_id_str, pt) {
                tracing::warn!("failed to index DM message {msg_id_str}: {e}");
            }
        }
    }

    // Notify unless this DM is the channel currently on screen.
    if let Some(ref pt) = plaintext {
        let visible = match app.try_state::<VisibleChannelState>() {
            Some(vs) => vs.channel_id.read().await.clone(),
            None => None,
        };
        if let Some(notif_state) = app.try_state::<NotificationState>() {
            let settings = notif_state.settings.read().await.clone();
            let sender_display = {
                app.state::<CacheDb>()
                    .lock()
                    .ok()
                    .and_then(|conn| {
                        conn.query_row(
                            "SELECT display_name FROM user_cache WHERE id = ?1",
                            [&sender_id.to_string()],
                            |row| row.get::<_, String>(0),
                        )
                        .ok()
                    })
                    .unwrap_or_else(|| "Someone".to_string())
            };
            if let Err(e) = notification_service::maybe_send_notification(
                app,
                &settings,
                None,
                Some(&dm_channel_id_str),
                None,
                &sender_display,
                pt,
                visible.as_deref(),
            ) {
                tracing::warn!("failed to send DM notification: {e}");
            }
        }
    }

    events::emit_dm_message(
        app,
        &events::WsDmMessagePayload {
            dm_channel_id,
            message_id,
            sender_id,
            plaintext,
            status: status.to_string(),
            created_at: created_at.to_rfc3339(),
        },
    );
}

/// Decrypt an incoming message via spawn_blocking, store in cache, index in FTS, emit event.
async fn handle_message_created(
    app: &AppHandle,
    ws_state: &WsState,
    msg: IncomingMessage<'_>,
    client_nonce: Option<&str>,
) {
    let IncomingMessage {
        channel_id,
        message_id,
        sender_id,
        sender_device_id,
        sender_signal_device_id,
        ciphertext,
        message_type,
        timestamp: created_at,
    } = msg;

    let msg_id_str = message_id.to_string();
    let sender_id_str = sender_id.to_string();
    let channel_id_str = channel_id.to_string();
    let created_at_ms = created_at.timestamp_millis();
    let created_at_rfc3339 = created_at.to_rfc3339();
    let ciphertext_owned = ciphertext.to_vec();
    let message_type_owned = message_type.to_string();

    // Check for optimistic insert dedup via nonce-based matching
    if let Some(nonce) = client_nonce {
        let local_msg_id = {
            let mut nonces = ws_state.pending_nonces.write().await;
            nonces.remove(nonce)
        };
        if let Some(local_id) = local_msg_id {
            // This is our own message echoed back - update status to delivered
            if let Ok(conn) = app.state::<CacheDb>().lock() {
                let _ = messages::update_message_status(&conn, &local_id, MSG_STATUS_DELIVERED);
                if let Ok(Some(existing)) = messages::get_message(&conn, &local_id) {
                    if let Some(ref pt) = existing.plaintext {
                        events::emit_message(
                            app,
                            &events::WsMessagePayload {
                                channel_id,
                                message_id,
                                sender_id,
                                plaintext: Some(pt.clone()),
                                status: MSG_STATUS_DELIVERED.into(),
                                created_at: created_at_rfc3339,
                            },
                        );
                    }
                }
            }
            return;
        }
    }

    let (plaintext, status) = decrypt_incoming(
        app,
        &sender_id_str,
        sender_signal_device_id,
        ciphertext_owned,
        message_type_owned,
        &msg_id_str,
    )
    .await;

    // Store in local cache
    let now_ms = chrono::Utc::now().timestamp_millis();
    let cached = CachedMessage {
        id: msg_id_str.clone(),
        channel_id: Some(channel_id_str.clone()),
        dm_channel_id: None,
        sender_id: sender_id.to_string(),
        sender_device_id: Some(sender_device_id.to_string()),
        sender_signal_device_id: Some(sender_signal_device_id),
        plaintext: plaintext.clone(),
        ciphertext: if status == MSG_STATUS_DECRYPT_FAILED {
            Some(ciphertext.to_vec())
        } else {
            None
        },
        message_type: Some(message_type.to_string()),
        created_at: created_at_ms,
        edited_at: None,
        decrypted_at: if status == MSG_STATUS_DELIVERED {
            Some(now_ms)
        } else {
            None
        },
        status: status.to_string(),
    };

    // Check if this is the sender's own message (skip unread increment if so)
    let is_own_message = {
        let uid = ws_state.current_user_id.read().await;
        uid.as_ref() == Some(&sender_id)
    };

    if let Ok(conn) = app.state::<CacheDb>().lock() {
        if let Err(e) = messages::insert_message(&conn, &cached) {
            tracing::warn!("failed to cache message {msg_id_str}: {e}");
        }

        // Index in FTS if decrypted successfully
        if let Some(ref pt) = plaintext {
            if let Err(e) = search::index_message(&conn, &msg_id_str, pt) {
                tracing::warn!("failed to index message {msg_id_str} in FTS: {e}");
            }
        }

        if !is_own_message {
            if let Err(e) = read_positions::increment_unread(&conn, &channel_id_str) {
                tracing::warn!("failed to increment unread for channel {channel_id_str}: {e}");
            }
        }
    }

    // Send desktop notification for non-own messages with plaintext
    if !is_own_message {
        if let Some(ref pt) = plaintext {
            let sender_display = {
                if let Ok(conn) = app.state::<CacheDb>().lock() {
                    conn.query_row(
                        "SELECT display_name FROM user_cache WHERE id = ?1",
                        [&sender_id.to_string()],
                        |row| row.get::<_, String>(0),
                    )
                    .unwrap_or_else(|_| "Someone".to_string())
                } else {
                    "Someone".to_string()
                }
            };

            // Look up guild_id from channel_cache for mute checks
            let guild_id = {
                if let Ok(conn) = app.state::<CacheDb>().lock() {
                    conn.query_row(
                        "SELECT guild_id FROM channel_cache WHERE id = ?1",
                        [&channel_id_str],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
                } else {
                    None
                }
            };

            let visible_channel = {
                let vis_state = app.try_state::<VisibleChannelState>();
                if let Some(vs) = vis_state {
                    vs.channel_id.read().await.clone()
                } else {
                    None
                }
            };

            let notif_settings = {
                let notif_state = app.try_state::<NotificationState>();
                if let Some(ns) = notif_state {
                    Some(ns.settings.read().await.clone())
                } else {
                    None
                }
            };

            if let Some(settings) = notif_settings {
                if let Err(e) = notification_service::maybe_send_notification(
                    app,
                    &settings,
                    Some(&channel_id_str),
                    None, // channel messages don't have dm_channel_id
                    guild_id.as_deref(),
                    &sender_display,
                    pt,
                    visible_channel.as_deref(),
                ) {
                    tracing::warn!("failed to send notification: {e}");
                }
            }
        }
    }

    // Emit event to frontend
    events::emit_message(
        app,
        &events::WsMessagePayload {
            channel_id,
            message_id,
            sender_id,
            plaintext,
            status: status.to_string(),
            created_at: created_at_rfc3339,
        },
    );
}

/// Decrypt an updated message, update cache and FTS, emit event.
async fn handle_message_updated(app: &AppHandle, msg: IncomingMessage<'_>) {
    let IncomingMessage {
        channel_id,
        message_id,
        sender_id,
        sender_device_id: _,
        sender_signal_device_id,
        ciphertext,
        message_type,
        timestamp: edited_at,
    } = msg;

    let msg_id_str = message_id.to_string();
    let sender_id_str = sender_id.to_string();
    let signal_device_id = sender_signal_device_id;
    let edited_at_ms = edited_at.timestamp_millis();
    let edited_at_rfc3339 = edited_at.to_rfc3339();
    let ciphertext_owned = ciphertext.to_vec();
    let message_type_owned = message_type.to_string();

    // Attempt decryption
    let app_clone = app.clone();
    let decrypt_result = tokio::task::spawn_blocking(move || {
        let crypto_state = app_clone.state::<CryptoState>();
        crypto_state.crypto_service.decrypt_message(
            &sender_id_str,
            signal_device_id,
            &ciphertext_owned,
            &message_type_owned,
        )
    })
    .await;

    let (plaintext, status) = match decrypt_result {
        Ok(Ok(plaintext_bytes)) => {
            let pt = String::from_utf8_lossy(&plaintext_bytes).to_string();
            (Some(pt), MSG_STATUS_DELIVERED)
        }
        Ok(Err(e)) => {
            tracing::warn!("decrypt failed for updated message {msg_id_str}: {e}");
            (None, MSG_STATUS_DECRYPT_FAILED)
        }
        Err(e) => {
            tracing::error!("spawn_blocking failed for updated message {msg_id_str}: {e}");
            (None, MSG_STATUS_DECRYPT_FAILED)
        }
    };

    // Update local cache
    if let Ok(conn) = app.state::<CacheDb>().lock() {
        if let Some(ref pt) = plaintext {
            if let Err(e) = messages::update_message_content(&conn, &msg_id_str, pt, edited_at_ms) {
                tracing::warn!("failed to update cached message {msg_id_str}: {e}");
            }
            // Update FTS index
            if let Err(e) = search::reindex_message(&conn, &msg_id_str, pt) {
                tracing::warn!("failed to reindex message {msg_id_str} in FTS: {e}");
            }
        }
    }

    // Emit event
    events::emit_message_updated(
        app,
        &events::WsMessageUpdatedPayload {
            channel_id,
            message_id,
            sender_id,
            plaintext,
            status: status.to_string(),
            edited_at: edited_at_rfc3339,
        },
    );
}

/// Handle a deleted message: soft-delete in cache, remove from FTS, emit event.
fn handle_message_deleted(app: &AppHandle, channel_id: ChannelId, message_id: MessageId) {
    let msg_id_str = message_id.to_string();

    if let Ok(conn) = app.state::<CacheDb>().lock() {
        if let Err(e) = messages::soft_delete_message(&conn, &msg_id_str) {
            tracing::warn!("failed to soft-delete message {msg_id_str}: {e}");
        }
        if let Err(e) = search::deindex_message(&conn, &msg_id_str) {
            tracing::warn!("failed to deindex message {msg_id_str} from FTS: {e}");
        }
    }

    events::emit_message_deleted(app, &channel_id, &message_id);
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
