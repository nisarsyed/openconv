use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use openconv_shared::ids::{ChannelId, DeviceId, GuildId, UserId};
use openconv_shared::permissions::Permissions;
use tokio::sync::{broadcast, mpsc};

use super::types::{PresenceStatus, ServerMessage};

const CHANNEL_BROADCAST_CAPACITY: usize = 1000;
const CONNECTION_MPSC_CAPACITY: usize = 256;
const PERMISSION_CACHE_TTL_SECS: u64 = 60;
const RATE_LIMIT_PER_SECOND: usize = 5;

/// Shared state for all active WebSocket connections.
pub struct WsState {
    /// Active connections keyed by (user_id, device_id).
    pub connections: DashMap<(UserId, DeviceId), ConnectionState>,

    /// Channel broadcast senders for message fan-out.
    pub channels: DashMap<ChannelId, broadcast::Sender<ServerMessage>>,

    /// Guild broadcast senders for guild-wide events.
    pub guilds: DashMap<GuildId, broadcast::Sender<ServerMessage>>,

    /// Cached permission resolutions to reduce DB load on message sends.
    pub permission_cache: PermissionCache,

    /// Per-user-per-channel rate limiter for message sends.
    pub rate_limiter: WsRateLimiter,

    /// Tracks active typing indicators with auto-expiry.
    pub typing: TypingManager,
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

    /// Abort handles for channel subscription forwarding tasks.
    pub channel_forward_tasks: HashMap<ChannelId, tokio::task::AbortHandle>,

    /// Abort handles for guild broadcast forwarding tasks.
    pub guild_forward_tasks: HashMap<GuildId, tokio::task::AbortHandle>,
}

impl Drop for ConnectionState {
    fn drop(&mut self) {
        for (_, handle) in self.channel_forward_tasks.drain() {
            handle.abort();
        }
        for (_, handle) in self.guild_forward_tasks.drain() {
            handle.abort();
        }
    }
}

impl WsState {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            channels: DashMap::new(),
            guilds: DashMap::new(),
            permission_cache: PermissionCache::new(Duration::from_secs(PERMISSION_CACHE_TTL_SECS)),
            rate_limiter: WsRateLimiter::new(RATE_LIMIT_PER_SECOND),
            typing: TypingManager::new(),
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
            channel_forward_tasks: HashMap::new(),
            guild_forward_tasks: HashMap::new(),
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
            channel_forward_tasks: HashMap::new(),
            guild_forward_tasks: HashMap::new(),
        };
        self.connections.insert((user_id, device_id), conn);
    }

    /// Remove a connection and return its state for cleanup.
    pub fn disconnect(&self, user_id: UserId, device_id: DeviceId) -> Option<ConnectionState> {
        self.connections
            .remove(&(user_id, device_id))
            .map(|(_, v)| v)
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

    /// Get or create a broadcast sender for a guild.
    pub fn get_or_create_guild_sender(
        &self,
        guild_id: GuildId,
    ) -> broadcast::Sender<ServerMessage> {
        self.guilds
            .entry(guild_id)
            .or_insert_with(|| broadcast::channel(CHANNEL_BROADCAST_CAPACITY).0)
            .clone()
    }

    /// Clean up a channel broadcast sender if it has zero receivers.
    /// Uses `remove_if` to avoid TOCTOU race between check and remove.
    pub fn try_cleanup_channel(&self, channel_id: &ChannelId) {
        self.channels
            .remove_if(channel_id, |_, sender| sender.receiver_count() == 0);
    }

    /// Clean up a guild broadcast sender if it has zero receivers.
    pub fn try_cleanup_guild(&self, guild_id: &GuildId) {
        self.guilds
            .remove_if(guild_id, |_, sender| sender.receiver_count() == 0);
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

// ─── Permission Cache ────────────────────────────────────────

/// Caches resolved permissions per (user, guild) pair with a configurable TTL.
/// Reduces database load for repeated permission checks (e.g. on every message send).
pub struct PermissionCache {
    cache: DashMap<(UserId, GuildId), (Permissions, Instant)>,
    ttl: Duration,
}

impl PermissionCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: DashMap::new(),
            ttl,
        }
    }

    /// Get cached permissions if they exist and haven't expired.
    pub fn get(&self, user_id: UserId, guild_id: GuildId) -> Option<Permissions> {
        let entry = self.cache.get(&(user_id, guild_id))?;
        if entry.1.elapsed() < self.ttl {
            Some(entry.0)
        } else {
            drop(entry);
            self.cache.remove(&(user_id, guild_id));
            None
        }
    }

    /// Cache a permission resolution result.
    pub fn insert(&self, user_id: UserId, guild_id: GuildId, perms: Permissions) {
        self.cache
            .insert((user_id, guild_id), (perms, Instant::now()));
    }

    /// Invalidate a specific cache entry (e.g. on role change).
    pub fn invalidate(&self, user_id: UserId, guild_id: GuildId) {
        self.cache.remove(&(user_id, guild_id));
    }
}

// ─── Rate Limiter ────────────────────────────────────────────

/// In-memory sliding-window rate limiter for WebSocket message sends.
/// Tracks timestamps per (user, channel) pair.
pub struct WsRateLimiter {
    windows: DashMap<(UserId, ChannelId), VecDeque<Instant>>,
    max_per_second: usize,
}

impl WsRateLimiter {
    pub fn new(max_per_second: usize) -> Self {
        Self {
            windows: DashMap::new(),
            max_per_second,
        }
    }

    /// Check if a message send is allowed. Returns true if within limit.
    /// Automatically records the attempt if allowed.
    pub fn check_and_record(&self, user_id: UserId, channel_id: ChannelId) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(1);
        let mut entry = self.windows.entry((user_id, channel_id)).or_default();

        // Evict timestamps outside the 1-second window
        while entry
            .front()
            .is_some_and(|t| now.duration_since(*t) > window)
        {
            entry.pop_front();
        }

        if entry.len() >= self.max_per_second {
            false
        } else {
            entry.push_back(now);
            true
        }
    }
}

// ─── Typing Manager ─────────────────────────────────────────

/// Manages active typing indicators with auto-expiry abort handles.
pub struct TypingManager {
    active: DashMap<(UserId, ChannelId), tokio::task::AbortHandle>,
}

impl Default for TypingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TypingManager {
    pub fn new() -> Self {
        Self {
            active: DashMap::new(),
        }
    }

    /// Register a typing timeout task. Aborts any existing timeout for the same key.
    pub fn start_typing(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
        handle: tokio::task::AbortHandle,
    ) {
        if let Some(old) = self.active.insert((user_id, channel_id), handle) {
            old.abort();
        }
    }

    /// Stop typing and abort the timeout task.
    pub fn stop_typing(&self, user_id: UserId, channel_id: ChannelId) {
        if let Some((_, handle)) = self.active.remove(&(user_id, channel_id)) {
            handle.abort();
        }
    }

    /// Remove an expired entry (called by the timeout task itself).
    pub fn expire(&self, user_id: UserId, channel_id: ChannelId) {
        self.active.remove(&(user_id, channel_id));
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
        assert_eq!(ws.channels.len(), 1);
    }

    #[test]
    fn get_or_create_guild_sender_creates_new() {
        let ws = WsState::new();
        let gid = GuildId::new();
        let sender = ws.get_or_create_guild_sender(gid);
        assert_eq!(sender.receiver_count(), 0);
        assert!(ws.guilds.contains_key(&gid));
    }

    #[test]
    fn try_cleanup_channel_removes_when_zero_receivers() {
        let ws = WsState::new();
        let cid = ChannelId::new();
        let _sender = ws.get_or_create_channel_sender(cid);
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

    #[test]
    fn try_cleanup_guild_removes_when_zero_receivers() {
        let ws = WsState::new();
        let gid = GuildId::new();
        let _sender = ws.get_or_create_guild_sender(gid);
        ws.try_cleanup_guild(&gid);
        assert!(!ws.guilds.contains_key(&gid));
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

    // ─── PermissionCache tests ───────────────────────────────

    #[test]
    fn permission_cache_returns_none_when_empty() {
        let cache = PermissionCache::new(Duration::from_secs(60));
        assert!(cache.get(UserId::new(), GuildId::new()).is_none());
    }

    #[test]
    fn permission_cache_returns_cached_value() {
        let cache = PermissionCache::new(Duration::from_secs(60));
        let uid = UserId::new();
        let gid = GuildId::new();
        let perms = Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES;
        cache.insert(uid, gid, perms);
        assert_eq!(cache.get(uid, gid), Some(perms));
    }

    #[test]
    fn permission_cache_invalidate_removes_entry() {
        let cache = PermissionCache::new(Duration::from_secs(60));
        let uid = UserId::new();
        let gid = GuildId::new();
        cache.insert(uid, gid, Permissions::SEND_MESSAGES);
        cache.invalidate(uid, gid);
        assert!(cache.get(uid, gid).is_none());
    }

    #[test]
    fn permission_cache_expired_entry_returns_none() {
        let cache = PermissionCache::new(Duration::from_millis(0));
        let uid = UserId::new();
        let gid = GuildId::new();
        cache.insert(uid, gid, Permissions::SEND_MESSAGES);
        // With 0ms TTL, should be expired immediately
        assert!(cache.get(uid, gid).is_none());
    }

    // ─── WsRateLimiter tests ────────────────────────────────

    #[test]
    fn rate_limiter_allows_within_limit() {
        let limiter = WsRateLimiter::new(5);
        let uid = UserId::new();
        let cid = ChannelId::new();
        for _ in 0..5 {
            assert!(limiter.check_and_record(uid, cid));
        }
    }

    #[test]
    fn rate_limiter_blocks_over_limit() {
        let limiter = WsRateLimiter::new(5);
        let uid = UserId::new();
        let cid = ChannelId::new();
        for _ in 0..5 {
            limiter.check_and_record(uid, cid);
        }
        assert!(!limiter.check_and_record(uid, cid));
    }

    #[test]
    fn rate_limiter_separate_channels_independent() {
        let limiter = WsRateLimiter::new(2);
        let uid = UserId::new();
        let cid1 = ChannelId::new();
        let cid2 = ChannelId::new();

        assert!(limiter.check_and_record(uid, cid1));
        assert!(limiter.check_and_record(uid, cid1));
        assert!(!limiter.check_and_record(uid, cid1));

        // Different channel still has capacity
        assert!(limiter.check_and_record(uid, cid2));
    }

    // ─── TypingManager tests ─────────────────────────────────

    #[tokio::test]
    async fn typing_manager_start_and_stop() {
        let mgr = TypingManager::new();
        let uid = UserId::new();
        let cid = ChannelId::new();

        let handle = tokio::spawn(async { tokio::time::sleep(Duration::from_secs(100)).await });
        mgr.start_typing(uid, cid, handle.abort_handle());
        assert!(mgr.active.contains_key(&(uid, cid)));

        mgr.stop_typing(uid, cid);
        assert!(!mgr.active.contains_key(&(uid, cid)));
    }

    #[tokio::test]
    async fn typing_manager_start_replaces_previous() {
        let mgr = TypingManager::new();
        let uid = UserId::new();
        let cid = ChannelId::new();

        let h1 = tokio::spawn(async { tokio::time::sleep(Duration::from_secs(100)).await });
        let h1_abort = h1.abort_handle();
        mgr.start_typing(uid, cid, h1_abort.clone());

        let h2 = tokio::spawn(async { tokio::time::sleep(Duration::from_secs(100)).await });
        mgr.start_typing(uid, cid, h2.abort_handle());

        // Old task should be aborted
        assert!(h1.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn typing_manager_expire_removes_entry() {
        let mgr = TypingManager::new();
        let uid = UserId::new();
        let cid = ChannelId::new();

        let task = tokio::spawn(async { tokio::time::sleep(Duration::from_secs(100)).await });
        mgr.active.insert((uid, cid), task.abort_handle());

        mgr.expire(uid, cid);
        assert!(!mgr.active.contains_key(&(uid, cid)));
    }

    #[tokio::test]
    async fn connection_state_drop_aborts_forward_tasks() {
        let ws = WsState::new();
        let uid = UserId::new();
        let did = DeviceId::new();
        let _rx = ws.register(uid, did, HashSet::new());

        let task = tokio::spawn(async { tokio::time::sleep(Duration::from_secs(100)).await });
        let task_handle = task.abort_handle();

        if let Some(mut conn) = ws.connections.get_mut(&(uid, did)) {
            conn.channel_forward_tasks
                .insert(ChannelId::new(), task_handle);
        }

        // Disconnect drops ConnectionState, which should abort the task
        ws.disconnect(uid, did);

        // Task should be cancelled
        assert!(task.await.unwrap_err().is_cancelled());
    }
}
