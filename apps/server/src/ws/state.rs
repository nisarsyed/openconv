use std::collections::HashSet;

use dashmap::DashMap;
use openconv_shared::ids::{ChannelId, DeviceId, GuildId, UserId};
use tokio::sync::{broadcast, mpsc};

use super::types::{PresenceStatus, ServerMessage};

const CHANNEL_BROADCAST_CAPACITY: usize = 1000;
const CONNECTION_MPSC_CAPACITY: usize = 256;

/// Shared state for all active WebSocket connections.
pub struct WsState {
    /// Active connections keyed by (user_id, device_id).
    pub connections: DashMap<(UserId, DeviceId), ConnectionState>,

    /// Channel broadcast senders for message fan-out.
    pub channels: DashMap<ChannelId, broadcast::Sender<ServerMessage>>,

    /// Guild broadcast senders for guild-wide events.
    pub guilds: DashMap<GuildId, broadcast::Sender<ServerMessage>>,
}

/// Per-connection state stored in the WsState DashMap.
pub struct ConnectionState {
    /// Sender half of the mpsc channel to push events to this connection's send loop.
    pub sender: mpsc::Sender<ServerMessage>,

    /// Set of channel IDs this connection is currently subscribed to.
    pub subscribed_channels: HashSet<ChannelId>,

    /// Current presence status for this connection.
    pub presence: PresenceStatus,

    /// Device identifier for this connection.
    pub device_id: DeviceId,

    /// Guild IDs the user is a member of (cached on connect).
    pub guild_ids: HashSet<GuildId>,
}

impl WsState {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            channels: DashMap::new(),
            guilds: DashMap::new(),
        }
    }

    /// Register a new connection. Returns the mpsc::Receiver for the send loop.
    pub fn register(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        guild_ids: HashSet<GuildId>,
    ) -> mpsc::Receiver<ServerMessage> {
        let (tx, rx) = mpsc::channel(CONNECTION_MPSC_CAPACITY);
        let conn = ConnectionState {
            sender: tx,
            subscribed_channels: HashSet::new(),
            presence: PresenceStatus::Online,
            device_id,
            guild_ids,
        };
        self.connections.insert((user_id, device_id), conn);
        rx
    }

    /// Register a connection with a pre-created sender.
    /// Allows the caller to enqueue messages (e.g. Ready) before registration.
    pub fn register_with_sender(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        guild_ids: HashSet<GuildId>,
        sender: mpsc::Sender<ServerMessage>,
    ) {
        let conn = ConnectionState {
            sender,
            subscribed_channels: HashSet::new(),
            presence: PresenceStatus::Online,
            device_id,
            guild_ids,
        };
        self.connections.insert((user_id, device_id), conn);
    }

    /// Remove a connection and return its state for cleanup.
    pub fn disconnect(
        &self,
        user_id: UserId,
        device_id: DeviceId,
    ) -> Option<ConnectionState> {
        self.connections.remove(&(user_id, device_id)).map(|(_, v)| v)
    }

    /// Get or create a broadcast sender for a channel.
    pub fn get_or_create_channel_sender(
        &self,
        channel_id: ChannelId,
    ) -> broadcast::Sender<ServerMessage> {
        self.channels
            .entry(channel_id)
            .or_insert_with(|| broadcast::channel(CHANNEL_BROADCAST_CAPACITY).0)
            .clone()
    }

    /// Clean up a channel broadcast sender if it has zero receivers.
    /// Uses `remove_if` to avoid TOCTOU race between check and remove.
    pub fn try_cleanup_channel(&self, channel_id: &ChannelId) {
        self.channels
            .remove_if(channel_id, |_, sender| sender.receiver_count() == 0);
    }

    /// Send a shutdown signal to all connections by dropping their senders.
    pub async fn shutdown_all(&self) {
        self.connections.clear();
        self.channels.clear();
        self.guilds.clear();
    }
}

impl Default for WsState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_connection_appears_in_connections() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did = DeviceId::new();
        let _rx = ws.register(uid, did, HashSet::new());
        assert!(ws.connections.contains_key(&(uid, did)));
    }

    #[test]
    fn register_returns_working_receiver() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did = DeviceId::new();
        let mut rx = ws.register(uid, did, HashSet::new());

        let conn = ws.connections.get(&(uid, did)).unwrap();
        let msg = ServerMessage::Pong { ts: 42 };
        conn.sender.try_send(msg).unwrap();
        drop(conn);

        let received = rx.try_recv().unwrap();
        match received {
            ServerMessage::Pong { ts } => assert_eq!(ts, 42),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn multiple_devices_same_user() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did1 = DeviceId::new();
        let did2 = DeviceId::new();

        let _rx1 = ws.register(uid, did1, HashSet::new());
        let _rx2 = ws.register(uid, did2, HashSet::new());

        assert!(ws.connections.contains_key(&(uid, did1)));
        assert!(ws.connections.contains_key(&(uid, did2)));
        assert_eq!(ws.connections.len(), 2);
    }

    #[test]
    fn disconnect_removes_correct_entry() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did1 = DeviceId::new();
        let did2 = DeviceId::new();

        let _rx1 = ws.register(uid, did1, HashSet::new());
        let _rx2 = ws.register(uid, did2, HashSet::new());

        let removed = ws.disconnect(uid, did1);
        assert!(removed.is_some());
        assert!(!ws.connections.contains_key(&(uid, did1)));
        assert!(ws.connections.contains_key(&(uid, did2)));
    }

    #[test]
    fn disconnect_nonexistent_returns_none() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did = DeviceId::new();
        assert!(ws.disconnect(uid, did).is_none());
    }

    #[test]
    fn get_or_create_channel_sender_creates_new() {
        let ws = WsState::new();
        let cid = ChannelId::new();
        let sender = ws.get_or_create_channel_sender(cid);
        assert_eq!(sender.receiver_count(), 0);
        assert!(ws.channels.contains_key(&cid));
    }

    #[test]
    fn get_or_create_channel_sender_returns_same() {
        let ws = WsState::new();
        let cid = ChannelId::new();
        let _s1 = ws.get_or_create_channel_sender(cid);
        let _s2 = ws.get_or_create_channel_sender(cid);
        // Only one entry in the map
        assert_eq!(ws.channels.len(), 1);
    }

    #[test]
    fn try_cleanup_channel_removes_when_zero_receivers() {
        let ws = WsState::new();
        let cid = ChannelId::new();
        let _sender = ws.get_or_create_channel_sender(cid);
        // No receivers subscribed, should clean up
        ws.try_cleanup_channel(&cid);
        assert!(!ws.channels.contains_key(&cid));
    }

    #[test]
    fn try_cleanup_channel_keeps_when_receivers_exist() {
        let ws = WsState::new();
        let cid = ChannelId::new();
        let sender = ws.get_or_create_channel_sender(cid);
        let _rx = sender.subscribe();
        ws.try_cleanup_channel(&cid);
        assert!(ws.channels.contains_key(&cid));
    }

    #[tokio::test]
    async fn shutdown_all_clears_everything() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did = DeviceId::new();
        let _rx = ws.register(uid, did, HashSet::new());
        let _sender = ws.get_or_create_channel_sender(ChannelId::new());

        ws.shutdown_all().await;
        assert!(ws.connections.is_empty());
        assert!(ws.channels.is_empty());
        assert!(ws.guilds.is_empty());
    }

    #[test]
    fn connection_state_initial_presence_is_online() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did = DeviceId::new();
        let _rx = ws.register(uid, did, HashSet::new());

        let conn = ws.connections.get(&(uid, did)).unwrap();
        assert_eq!(conn.presence, PresenceStatus::Online);
    }

    #[test]
    fn connection_state_stores_guild_ids() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did = DeviceId::new();
        let gid = GuildId::new();
        let guild_ids = HashSet::from([gid]);
        let _rx = ws.register(uid, did, guild_ids);

        let conn = ws.connections.get(&(uid, did)).unwrap();
        assert!(conn.guild_ids.contains(&gid));
    }
}
