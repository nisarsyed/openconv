use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use openconv_shared::api::ws::ClientMessage;
use openconv_shared::ids::{ChannelId, UserId};
use tokio::sync::{mpsc, RwLock};

/// All possible states of the WebSocket connection.
/// Stored in Arc<RwLock<WsConnectionState>> as Tauri managed state.
/// Every state transition emits a `ws:state` Tauri event to the frontend.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(tag = "status")]
pub enum WsConnectionState {
    Disconnected,
    Connecting { attempt: u32 },
    Connected,
    Authenticated,
    Reconnecting { attempt: u32, next_retry_ms: u64 },
    Failed { reason: String },
}

impl Default for WsConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

/// Holds all WebSocket-related managed state.
/// All fields are Arc-wrapped so the struct is cheaply cloneable.
#[derive(Clone)]
pub struct WsState {
    pub connection_state: Arc<RwLock<WsConnectionState>>,
    pub subscribed_channels: Arc<RwLock<HashSet<ChannelId>>>,
    pub last_pong: Arc<RwLock<Instant>>,
    /// Sender half of a channel to send outgoing messages to the WS write loop.
    pub outgoing_tx: Arc<RwLock<Option<mpsc::UnboundedSender<ClientMessage>>>>,
    /// Handle to the spawned connection task so it can be aborted on disconnect.
    pub connection_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// API base URL (from AuthService config).
    pub api_base_url: Arc<RwLock<String>>,
    /// Shared HTTP client for ticket requests.
    pub http_client: reqwest::Client,
    /// Current authenticated user ID, set on Ready.
    pub current_user_id: Arc<RwLock<Option<UserId>>>,
    /// Maps client_nonce -> local message_id for optimistic insert dedup.
    pub pending_nonces: Arc<RwLock<HashMap<String, String>>>,
}

impl WsState {
    pub fn new() -> Self {
        let api_base_url =
            std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());
        Self {
            connection_state: Arc::new(RwLock::new(WsConnectionState::default())),
            subscribed_channels: Arc::new(RwLock::new(HashSet::new())),
            last_pong: Arc::new(RwLock::new(Instant::now())),
            outgoing_tx: Arc::new(RwLock::new(None)),
            connection_task: Arc::new(RwLock::new(None)),
            api_base_url: Arc::new(RwLock::new(api_base_url)),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            current_user_id: Arc::new(RwLock::new(None)),
            pending_nonces: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_connection_state_default_is_disconnected() {
        let state = WsConnectionState::default();
        assert!(matches!(state, WsConnectionState::Disconnected));
    }

    #[test]
    fn ws_connection_state_connecting_serializes_with_attempt() {
        let state = WsConnectionState::Connecting { attempt: 0 };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["status"], "Connecting");
        assert_eq!(json["attempt"], 0);
    }

    #[test]
    fn ws_connection_state_reconnecting_serializes_with_fields() {
        let state = WsConnectionState::Reconnecting {
            attempt: 3,
            next_retry_ms: 4000,
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["status"], "Reconnecting");
        assert_eq!(json["attempt"], 3);
        assert_eq!(json["next_retry_ms"], 4000);
    }

    #[test]
    fn ws_connection_state_failed_serializes_with_reason() {
        let state = WsConnectionState::Failed {
            reason: "timeout".into(),
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["status"], "Failed");
        assert_eq!(json["reason"], "timeout");
    }

    #[test]
    fn ws_connection_state_authenticated_serializes() {
        let state = WsConnectionState::Authenticated;
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["status"], "Authenticated");
    }

    #[test]
    fn ws_connection_state_connected_serializes() {
        let state = WsConnectionState::Connected;
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["status"], "Connected");
    }

    #[test]
    fn ws_connection_state_disconnected_serializes() {
        let state = WsConnectionState::Disconnected;
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["status"], "Disconnected");
    }

    #[tokio::test]
    async fn ws_state_new_starts_disconnected() {
        let ws = WsState::new();
        let state = ws.connection_state.read().await;
        assert!(matches!(*state, WsConnectionState::Disconnected));
    }

    #[tokio::test]
    async fn ws_state_subscribed_channels_starts_empty() {
        let ws = WsState::new();
        let channels = ws.subscribed_channels.read().await;
        assert!(channels.is_empty());
    }

    #[tokio::test]
    async fn ws_state_tracks_subscribe_unsubscribe() {
        let ws = WsState::new();
        let ch = ChannelId::new();

        {
            let mut channels = ws.subscribed_channels.write().await;
            channels.insert(ch);
        }

        {
            let channels = ws.subscribed_channels.read().await;
            assert!(channels.contains(&ch));
            assert_eq!(channels.len(), 1);
        }

        {
            let mut channels = ws.subscribed_channels.write().await;
            channels.remove(&ch);
        }

        {
            let channels = ws.subscribed_channels.read().await;
            assert!(channels.is_empty());
        }
    }

    #[tokio::test]
    async fn ws_state_outgoing_tx_starts_none() {
        let ws = WsState::new();
        let tx = ws.outgoing_tx.read().await;
        assert!(tx.is_none());
    }

    #[tokio::test]
    async fn ws_state_connection_task_starts_none() {
        let ws = WsState::new();
        let task = ws.connection_task.read().await;
        assert!(task.is_none());
    }

    #[tokio::test]
    async fn ws_state_can_transition_to_connecting() {
        let ws = WsState::new();
        {
            let mut state = ws.connection_state.write().await;
            *state = WsConnectionState::Connecting { attempt: 0 };
        }
        let state = ws.connection_state.read().await;
        assert!(matches!(*state, WsConnectionState::Connecting { attempt: 0 }));
    }

    #[tokio::test]
    async fn ws_state_can_transition_to_authenticated() {
        let ws = WsState::new();
        {
            let mut state = ws.connection_state.write().await;
            *state = WsConnectionState::Connected;
        }
        {
            let mut state = ws.connection_state.write().await;
            *state = WsConnectionState::Authenticated;
        }
        let state = ws.connection_state.read().await;
        assert!(matches!(*state, WsConnectionState::Authenticated));
    }

    #[tokio::test]
    async fn ws_state_can_transition_to_disconnected_from_any_state() {
        let ws = WsState::new();
        // From Authenticated
        {
            let mut state = ws.connection_state.write().await;
            *state = WsConnectionState::Authenticated;
        }
        {
            let mut state = ws.connection_state.write().await;
            *state = WsConnectionState::Disconnected;
        }
        let state = ws.connection_state.read().await;
        assert!(matches!(*state, WsConnectionState::Disconnected));
    }
}
