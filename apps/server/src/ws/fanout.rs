use std::sync::Arc;
use std::time::Duration;

use openconv_shared::api::ws::RecipientPayload;
use openconv_shared::ids::{ChannelId, DeviceId, GuildId, MessageId, UserId};
use openconv_shared::permissions::Permissions;
use tokio::sync::broadcast;

use crate::extractors::guild_member::resolve_guild_membership;
use crate::state::AppState;

use super::connection::send_error;
use super::replay;
use super::state::WsState;
use super::types::ServerMessage;

// ─── Channel-to-guild resolution ─────────────────────────────

async fn resolve_channel_guild(db: &sqlx::PgPool, channel_id: ChannelId) -> Option<GuildId> {
    sqlx::query_scalar("SELECT guild_id FROM channels WHERE id = $1")
        .bind(channel_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

// ─── Permission resolution with cache ────────────────────────

enum PermissionError {
    Denied,
    Internal,
}

async fn check_permission(
    state: &AppState,
    user_id: UserId,
    guild_id: GuildId,
    required: Permissions,
) -> Result<Permissions, PermissionError> {
    // Check cache first
    if let Some(cached) = state.ws.permission_cache.get(user_id, guild_id) {
        return if cached.contains(required) {
            Ok(cached)
        } else {
            Err(PermissionError::Denied)
        };
    }

    // Cache miss — resolve from DB
    let perms = resolve_guild_membership(&state.db, user_id, guild_id)
        .await
        .map_err(|e| {
            tracing::error!(user_id = %user_id, guild_id = %guild_id, error = ?e, "permission resolution failed");
            PermissionError::Internal
        })?;

    state.ws.permission_cache.insert(user_id, guild_id, perms);

    if perms.contains(required) {
        Ok(perms)
    } else {
        Err(PermissionError::Denied)
    }
}

fn handle_permission_error(
    state: &AppState,
    user_id: UserId,
    device_id: DeviceId,
    err: PermissionError,
) {
    match err {
        PermissionError::Denied => {
            send_error(state, user_id, device_id, 4001, "permission denied");
        }
        PermissionError::Internal => {
            send_error(state, user_id, device_id, 4004, "internal error");
        }
    }
}

// ─── Subscribe ───────────────────────────────────────────────

pub async fn handle_subscribe(
    state: &AppState,
    user_id: UserId,
    device_id: DeviceId,
    channel_id: ChannelId,
) {
    // Resolve channel → guild
    let guild_id = match resolve_channel_guild(&state.db, channel_id).await {
        Some(gid) => gid,
        None => {
            send_error(state, user_id, device_id, 4007, "channel not found");
            return;
        }
    };

    // Check READ_MESSAGES permission
    if let Err(e) = check_permission(state, user_id, guild_id, Permissions::READ_MESSAGES).await {
        handle_permission_error(state, user_id, device_id, e);
        return;
    }

    // Check if already subscribed
    {
        if let Some(conn) = state.ws.connections.get(&(user_id, device_id)) {
            if conn.subscribed_channels.contains(&channel_id) {
                return; // already subscribed, no-op
            }
        }
    }

    // Get sender clone for forwarding task
    let mpsc_tx = match state.ws.connections.get(&(user_id, device_id)) {
        Some(c) => c.sender.clone(),
        None => return,
    };

    // Subscribe to broadcast BEFORE replay (so live messages buffer)
    let broadcast_tx = state.ws.get_or_create_channel_sender(channel_id);
    let broadcast_rx = broadcast_tx.subscribe();

    // Replay missed messages (if any last_seen exists in Redis)
    if let Err(e) = replay::replay_missed_messages(
        &state.db,
        &state.redis,
        user_id,
        device_id,
        channel_id,
        &mpsc_tx,
    )
    .await
    {
        tracing::warn!(
            user_id = %user_id,
            channel_id = %channel_id,
            error = %e,
            "failed to replay missed messages"
        );
    }

    // Spawn forwarding task
    let handle = tokio::spawn(forward_channel_messages(
        broadcast_rx,
        mpsc_tx,
        user_id,
        device_id,
        channel_id,
    ));

    // Track subscription
    if let Some(mut conn) = state.ws.connections.get_mut(&(user_id, device_id)) {
        conn.subscribed_channels.insert(channel_id);
        conn.channel_forward_tasks
            .insert(channel_id, handle.abort_handle());
    }
}

async fn forward_channel_messages(
    mut broadcast_rx: broadcast::Receiver<ServerMessage>,
    mpsc_tx: tokio::sync::mpsc::Sender<ServerMessage>,
    user_id: UserId,
    device_id: DeviceId,
    channel_id: ChannelId,
) {
    loop {
        match broadcast_rx.recv().await {
            Ok(msg) => {
                if mpsc_tx.send(msg).await.is_err() {
                    break; // connection closed
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                let err = ServerMessage::Error {
                    code: 4006,
                    message: format!("missed {n} messages on channel {channel_id}"),
                };
                if mpsc_tx.send(err).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Closed) => {
                tracing::debug!(
                    user_id = %user_id,
                    device_id = %device_id,
                    channel_id = %channel_id,
                    "channel broadcast closed"
                );
                break;
            }
        }
    }
}

// ─── Unsubscribe ─────────────────────────────────────────────

pub fn handle_unsubscribe(
    state: &AppState,
    user_id: UserId,
    device_id: DeviceId,
    channel_id: ChannelId,
) {
    if let Some(mut conn) = state.ws.connections.get_mut(&(user_id, device_id)) {
        conn.subscribed_channels.remove(&channel_id);
        if let Some(handle) = conn.channel_forward_tasks.remove(&channel_id) {
            handle.abort();
        }
    }

    // Clean up broadcast sender if no receivers remain
    state.ws.try_cleanup_channel(&channel_id);
}

// ─── Send Message ────────────────────────────────────────────

const MAX_RECIPIENTS: usize = 200;

pub async fn handle_send_message(
    state: &AppState,
    user_id: UserId,
    sender_device_id: DeviceId,
    channel_id: ChannelId,
    recipients: Vec<RecipientPayload>,
) {
    // Validate recipients
    if recipients.is_empty() {
        send_error(
            state,
            user_id,
            sender_device_id,
            4004,
            "recipients must not be empty",
        );
        return;
    }
    if recipients.len() > MAX_RECIPIENTS {
        send_error(
            state,
            user_id,
            sender_device_id,
            4004,
            "too many recipients",
        );
        return;
    }

    // Rate limit check
    if !state.ws.rate_limiter.check_and_record(user_id, channel_id) {
        send_error(state, user_id, sender_device_id, 4003, "rate limited");
        return;
    }

    // Resolve channel → guild
    let guild_id = match resolve_channel_guild(&state.db, channel_id).await {
        Some(gid) => gid,
        None => {
            send_error(state, user_id, sender_device_id, 4007, "channel not found");
            return;
        }
    };

    // Re-check SEND_MESSAGES permission
    if let Err(e) = check_permission(state, user_id, guild_id, Permissions::SEND_MESSAGES).await {
        handle_permission_error(state, user_id, sender_device_id, e);
        return;
    }

    // Persist message + recipients + channel event in a single transaction
    let (message_id, created_at) = match persist_message(
        &state.db,
        channel_id,
        user_id,
        sender_device_id,
        &recipients,
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(error = %e, "failed to persist message");
            send_error(
                state,
                user_id,
                sender_device_id,
                4004,
                "failed to send message",
            );
            return;
        }
    };

    // Per-device fan-out: send each subscribed connection its own ciphertext
    fan_out_message_created(
        state,
        channel_id,
        message_id,
        user_id,
        sender_device_id,
        &recipients,
        created_at,
    );
}

/// Persist message metadata, per-device recipient payloads, and channel event in one transaction.
async fn persist_message(
    db: &sqlx::PgPool,
    channel_id: ChannelId,
    sender_id: UserId,
    sender_device_id: DeviceId,
    recipients: &[RecipientPayload],
) -> Result<(MessageId, chrono::DateTime<chrono::Utc>), sqlx::Error> {
    let mut tx = db.begin().await?;

    // Insert message row (encrypted_content/nonce NULL; content lives in message_recipients)
    let row: (MessageId, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO messages (channel_id, sender_id, sender_device_id, encrypted_content, nonce) \
         VALUES ($1, $2, $3, NULL, NULL) RETURNING id, created_at",
    )
    .bind(channel_id)
    .bind(sender_id)
    .bind(sender_device_id)
    .fetch_one(&mut *tx)
    .await?;

    let (message_id, created_at) = row;

    // Batch-insert recipient payloads
    for rp in recipients {
        sqlx::query(
            "INSERT INTO message_recipients (message_id, user_id, device_id, ciphertext, message_type) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(message_id)
        .bind(rp.user_id)
        .bind(rp.device_id)
        .bind(&rp.ciphertext)
        .bind(&rp.message_type)
        .execute(&mut *tx)
        .await?;
    }

    // Insert channel event for sync
    sqlx::query(
        "INSERT INTO channel_events (channel_id, event_type, message_id) VALUES ($1, 'message_created', $2)",
    )
    .bind(channel_id)
    .bind(message_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((message_id, created_at))
}

/// Send a personalized MessageCreated to each subscribed connection
/// that has a matching RecipientPayload.
fn fan_out_message_created(
    state: &AppState,
    channel_id: ChannelId,
    message_id: MessageId,
    sender_id: UserId,
    sender_device_id: DeviceId,
    recipients: &[RecipientPayload],
    created_at: chrono::DateTime<chrono::Utc>,
) {
    for conn_ref in state.ws.connections.iter() {
        let ((uid, did), conn) = conn_ref.pair();
        if !conn.subscribed_channels.contains(&channel_id) {
            continue;
        }
        // Find the matching recipient payload for this connection's (user_id, device_id)
        if let Some(rp) = recipients
            .iter()
            .find(|rp| rp.user_id == *uid && rp.device_id == *did)
        {
            let event = ServerMessage::MessageCreated {
                channel_id,
                message_id,
                sender_id,
                sender_device_id,
                ciphertext: rp.ciphertext.clone(),
                message_type: rp.message_type.clone(),
                created_at,
                client_nonce: None,
            };
            let _ = conn.sender.try_send(event);
        }
    }
}

// ─── Edit Message ────────────────────────────────────────────

pub async fn handle_edit_message(
    state: &AppState,
    user_id: UserId,
    sender_device_id: DeviceId,
    channel_id: ChannelId,
    message_id: MessageId,
    recipients: Vec<RecipientPayload>,
) {
    // Validate recipients
    if recipients.is_empty() {
        send_error(
            state,
            user_id,
            sender_device_id,
            4004,
            "recipients must not be empty",
        );
        return;
    }
    if recipients.len() > MAX_RECIPIENTS {
        send_error(
            state,
            user_id,
            sender_device_id,
            4004,
            "too many recipients",
        );
        return;
    }

    // Rate limit check
    if !state.ws.rate_limiter.check_and_record(user_id, channel_id) {
        send_error(state, user_id, sender_device_id, 4003, "rate limited");
        return;
    }

    // Resolve channel → guild and verify membership
    let guild_id = match resolve_channel_guild(&state.db, channel_id).await {
        Some(gid) => gid,
        None => {
            send_error(state, user_id, sender_device_id, 4007, "channel not found");
            return;
        }
    };

    if let Err(e) = check_permission(state, user_id, guild_id, Permissions::READ_MESSAGES).await {
        handle_permission_error(state, user_id, sender_device_id, e);
        return;
    }

    // Atomic update with ownership check, replace recipients, log event
    match persist_edit(&state.db, user_id, channel_id, message_id, &recipients).await {
        Ok(Some(edited_at)) => {
            // Per-device fan-out for edit
            fan_out_message_updated(
                state,
                channel_id,
                message_id,
                user_id,
                sender_device_id,
                &recipients,
                edited_at,
            );
        }
        Ok(None) => {
            send_error(
                state,
                user_id,
                sender_device_id,
                4007,
                "message not found or not yours",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to edit message");
            send_error(
                state,
                user_id,
                sender_device_id,
                4004,
                "failed to edit message",
            );
        }
    }
}

/// Atomic edit: UPDATE message, replace recipient rows, log channel event.
/// Returns Some(edited_at) if a row was updated, None if no matching message found.
async fn persist_edit(
    db: &sqlx::PgPool,
    user_id: UserId,
    channel_id: ChannelId,
    message_id: MessageId,
    recipients: &[RecipientPayload],
) -> Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Update message metadata with ownership check
    let result: Option<(chrono::DateTime<chrono::Utc>,)> = sqlx::query_as(
        "UPDATE messages SET edited_at = NOW() \
         WHERE id = $1 AND channel_id = $2 AND sender_id = $3 AND deleted = false \
         RETURNING edited_at",
    )
    .bind(message_id)
    .bind(channel_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    let edited_at = match result {
        Some((ts,)) => ts,
        None => return Ok(None),
    };

    // Replace recipient rows: delete old, insert new
    sqlx::query("DELETE FROM message_recipients WHERE message_id = $1")
        .bind(message_id)
        .execute(&mut *tx)
        .await?;

    for rp in recipients {
        sqlx::query(
            "INSERT INTO message_recipients (message_id, user_id, device_id, ciphertext, message_type) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(message_id)
        .bind(rp.user_id)
        .bind(rp.device_id)
        .bind(&rp.ciphertext)
        .bind(&rp.message_type)
        .execute(&mut *tx)
        .await?;
    }

    // Log channel event
    sqlx::query(
        "INSERT INTO channel_events (channel_id, event_type, message_id) VALUES ($1, 'message_updated', $2)",
    )
    .bind(channel_id)
    .bind(message_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Some(edited_at))
}

/// Send personalized MessageUpdated to each subscribed connection.
fn fan_out_message_updated(
    state: &AppState,
    channel_id: ChannelId,
    message_id: MessageId,
    sender_id: UserId,
    sender_device_id: DeviceId,
    recipients: &[RecipientPayload],
    edited_at: chrono::DateTime<chrono::Utc>,
) {
    for conn_ref in state.ws.connections.iter() {
        let ((uid, did), conn) = conn_ref.pair();
        if !conn.subscribed_channels.contains(&channel_id) {
            continue;
        }
        if let Some(rp) = recipients
            .iter()
            .find(|rp| rp.user_id == *uid && rp.device_id == *did)
        {
            let event = ServerMessage::MessageUpdated {
                channel_id,
                message_id,
                sender_id,
                sender_device_id,
                ciphertext: rp.ciphertext.clone(),
                message_type: rp.message_type.clone(),
                edited_at,
            };
            let _ = conn.sender.try_send(event);
        }
    }
}

// ─── Delete Message ──────────────────────────────────────────

pub async fn handle_delete_message(
    state: &AppState,
    user_id: UserId,
    device_id: DeviceId,
    channel_id: ChannelId,
    message_id: MessageId,
) {
    // Rate limit check
    if !state.ws.rate_limiter.check_and_record(user_id, channel_id) {
        send_error(state, user_id, device_id, 4003, "rate limited");
        return;
    }

    // Resolve guild for permission check
    let guild_id = match resolve_channel_guild(&state.db, channel_id).await {
        Some(gid) => gid,
        None => {
            send_error(state, user_id, device_id, 4007, "channel not found");
            return;
        }
    };

    // Resolve permissions (needed for MANAGE_MESSAGES check)
    let perms = match check_permission(state, user_id, guild_id, Permissions::READ_MESSAGES).await {
        Ok(p) => p,
        Err(e) => {
            handle_permission_error(state, user_id, device_id, e);
            return;
        }
    };

    let can_manage = perms.contains(Permissions::MANAGE_MESSAGES);

    match persist_delete(&state.db, user_id, can_manage, channel_id, message_id).await {
        Ok(true) => {
            let event = ServerMessage::MessageDeleted {
                channel_id,
                message_id,
            };
            if let Some(sender) = state.ws.channels.get(&channel_id) {
                let _ = sender.send(event);
            }
        }
        Ok(false) => {
            send_error(state, user_id, device_id, 4007, "message not found");
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to delete message");
            send_error(state, user_id, device_id, 4004, "failed to delete message");
        }
    }
}

/// Atomic delete with authorization, cryptographic erasure, and event logging.
/// Tries sender ownership first, then MANAGE_MESSAGES if the user has that permission.
/// Deletes message_recipients rows and logs a channel event within the same transaction.
async fn persist_delete(
    db: &sqlx::PgPool,
    user_id: UserId,
    can_manage_messages: bool,
    channel_id: ChannelId,
    message_id: MessageId,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let empty: &[u8] = &[];

    // Try sender ownership delete first (most common case)
    let result = sqlx::query(
        "UPDATE messages SET deleted = true, encrypted_content = $1, nonce = $2 \
         WHERE id = $3 AND channel_id = $4 AND sender_id = $5 AND deleted = false",
    )
    .bind(empty)
    .bind(empty)
    .bind(message_id)
    .bind(channel_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    let deleted = if result.rows_affected() > 0 {
        true
    } else if can_manage_messages {
        let result = sqlx::query(
            "UPDATE messages SET deleted = true, encrypted_content = $1, nonce = $2 \
             WHERE id = $3 AND channel_id = $4 AND deleted = false",
        )
        .bind(empty)
        .bind(empty)
        .bind(message_id)
        .bind(channel_id)
        .execute(&mut *tx)
        .await?;

        result.rows_affected() > 0
    } else {
        false
    };

    if deleted {
        // Cryptographic erasure: remove per-device ciphertext
        sqlx::query("DELETE FROM message_recipients WHERE message_id = $1")
            .bind(message_id)
            .execute(&mut *tx)
            .await?;

        // Log channel event for sync
        sqlx::query(
            "INSERT INTO channel_events (channel_id, event_type, message_id) VALUES ($1, 'message_deleted', $2)",
        )
        .bind(channel_id)
        .bind(message_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(deleted)
}

// ─── Periodic cleanup ────────────────────────────────────────

/// Spawns a background task that sweeps broadcast channels and guild senders
/// every 5 minutes, removing entries with zero receivers.
pub fn spawn_periodic_cleanup(ws: Arc<WsState>, mut shutdown: broadcast::Receiver<()>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        interval.tick().await; // skip immediate first tick

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let mut channel_removed = 0usize;
                    let mut guild_removed = 0usize;

                    ws.channels.retain(|_, sender| {
                        if sender.receiver_count() == 0 {
                            channel_removed += 1;
                            false
                        } else {
                            true
                        }
                    });

                    ws.guilds.retain(|_, sender| {
                        if sender.receiver_count() == 0 {
                            guild_removed += 1;
                            false
                        } else {
                            true
                        }
                    });

                    if channel_removed > 0 || guild_removed > 0 {
                        tracing::debug!(
                            channel_removed,
                            guild_removed,
                            "periodic broadcast cleanup"
                        );
                    }
                }
                _ = shutdown.recv() => {
                    tracing::info!("periodic cleanup task shutting down");
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn error_code_constants() {
        // Document expected error codes for WS protocol
        assert_eq!(4001_u32, 4001); // permission denied
        assert_eq!(4003_u32, 4003); // rate limited
        assert_eq!(4004_u32, 4004); // internal error
        assert_eq!(4006_u32, 4006); // lagged
        assert_eq!(4007_u32, 4007); // not found
    }

    // Integration tests for subscribe/unsubscribe/send/edit/delete
    // require database and Redis — placed in apps/server/tests/ directory.
}
