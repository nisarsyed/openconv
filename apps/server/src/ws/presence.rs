use std::collections::HashSet;
use std::time::Duration;

use openconv_shared::ids::{ChannelId, DeviceId, GuildId, UserId};

use crate::state::AppState;

use super::types::{PresenceStatus, ServerMessage};

const TYPING_TIMEOUT_SECS: u64 = 5;

// ─── Guild broadcast subscription ───────────────────────────

/// Set up guild broadcast forwarding tasks on connect.
/// For each guild, subscribes to the guild broadcast and forwards events
/// to the connection's mpsc sender.
pub fn setup_guild_subscriptions(
    state: &AppState,
    user_id: UserId,
    device_id: DeviceId,
    guild_ids: &HashSet<GuildId>,
) {
    let mpsc_tx = match state.ws.connections.get(&(user_id, device_id)) {
        Some(c) => c.sender.clone(),
        None => return,
    };

    for &guild_id in guild_ids {
        let broadcast_tx = state.ws.get_or_create_guild_sender(guild_id);
        let broadcast_rx = broadcast_tx.subscribe();

        let tx = mpsc_tx.clone();
        let handle = tokio::spawn(forward_guild_messages(broadcast_rx, tx));

        if let Some(mut conn) = state.ws.connections.get_mut(&(user_id, device_id)) {
            conn.guild_forward_tasks
                .insert(guild_id, handle.abort_handle());
        }
    }
}

async fn forward_guild_messages(
    mut broadcast_rx: tokio::sync::broadcast::Receiver<ServerMessage>,
    mpsc_tx: tokio::sync::mpsc::Sender<ServerMessage>,
) {
    loop {
        match broadcast_rx.recv().await {
            Ok(msg) => {
                if mpsc_tx.send(msg).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                // Guild events (presence, member join/leave) can be missed
                // without breaking the protocol — client will see the next update.
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

// ─── Presence lifecycle ──────────────────────────────────────

/// Broadcast PresenceUpdate { Online } to all guilds the user belongs to.
pub fn broadcast_connect(state: &AppState, user_id: UserId, guild_ids: &HashSet<GuildId>) {
    let event = ServerMessage::PresenceUpdate {
        user_id,
        status: PresenceStatus::Online,
    };
    broadcast_to_guilds(state, guild_ids, event);
}

/// Broadcast PresenceUpdate { Offline } to all guilds, then clean up guild senders.
pub fn broadcast_disconnect(state: &AppState, user_id: UserId, guild_ids: &HashSet<GuildId>) {
    let event = ServerMessage::PresenceUpdate {
        user_id,
        status: PresenceStatus::Offline,
    };
    broadcast_to_guilds(state, guild_ids, event);

    // Clean up guild broadcast senders with zero receivers
    for guild_id in guild_ids {
        state.ws.try_cleanup_guild(guild_id);
    }
}

/// Handle SetPresence client message.
pub fn handle_set_presence(
    state: &AppState,
    user_id: UserId,
    device_id: DeviceId,
    status: PresenceStatus,
) {
    // Update stored presence
    let guild_ids = if let Some(mut conn) = state.ws.connections.get_mut(&(user_id, device_id)) {
        conn.presence = status;
        conn.guild_ids.clone()
    } else {
        return;
    };

    // Broadcast to all guilds
    let event = ServerMessage::PresenceUpdate { user_id, status };
    broadcast_to_guilds(state, &guild_ids, event);
}

fn broadcast_to_guilds(state: &AppState, guild_ids: &HashSet<GuildId>, event: ServerMessage) {
    for guild_id in guild_ids {
        if let Some(sender) = state.ws.guilds.get(guild_id) {
            let _ = sender.send(event.clone());
        }
    }
}

// ─── Typing indicators ──────────────────────────────────────

/// Handle StartTyping: broadcast to channel subscribers and start timeout.
pub fn handle_start_typing(state: &AppState, user_id: UserId, channel_id: ChannelId) {
    // Broadcast typing started to channel
    let event = ServerMessage::TypingStarted {
        channel_id,
        user_id,
    };
    if let Some(sender) = state.ws.channels.get(&channel_id) {
        let _ = sender.send(event);
    }

    // Start (or reset) typing timeout
    let ws = state.ws.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(TYPING_TIMEOUT_SECS)).await;
        ws.typing.expire(user_id, channel_id);
    });

    state
        .ws
        .typing
        .start_typing(user_id, channel_id, handle.abort_handle());
}

/// Handle StopTyping: cancel the timeout, no broadcast needed.
pub fn handle_stop_typing(state: &AppState, user_id: UserId, channel_id: ChannelId) {
    state.ws.typing.stop_typing(user_id, channel_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_timeout_is_5_seconds() {
        assert_eq!(TYPING_TIMEOUT_SECS, 5);
    }

    // Integration tests for presence broadcasts and typing indicators
    // require WsState with active connections — placed in apps/server/tests/.
}
