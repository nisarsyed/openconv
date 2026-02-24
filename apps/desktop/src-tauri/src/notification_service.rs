use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::RwLock;

/// Notification preferences, stored as individual key-value pairs in the
/// local cache `settings` table. Loaded into memory on app launch.
#[derive(Debug, Clone)]
pub struct NotificationSettings {
    /// Master switch for all notifications (default: true)
    pub notifications_enabled: bool,
    /// Whether to include message preview text (default: false -- privacy-by-default)
    pub notification_previews: bool,
    /// Do Not Disturb mode, suppresses all notifications (default: false)
    pub dnd_enabled: bool,
    /// Set of guild IDs whose channels are muted
    pub muted_guilds: HashSet<String>,
    /// Set of channel IDs that are individually muted
    pub muted_channels: HashSet<String>,
    /// Per-channel override for preview setting (overrides global notification_previews)
    pub preview_overrides: HashMap<String, bool>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            notification_previews: false,
            dnd_enabled: false,
            muted_guilds: HashSet::new(),
            muted_channels: HashSet::new(),
            preview_overrides: HashMap::new(),
        }
    }
}

impl NotificationSettings {
    pub fn load_from_db(conn: &Connection) -> Self {
        let mut settings = Self::default();

        let get = |key: &str| -> Option<String> {
            conn.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .ok()
        };

        if let Some(v) = get("notifications_enabled") {
            settings.notifications_enabled = v == "true";
        }
        if let Some(v) = get("notification_previews") {
            settings.notification_previews = v == "true";
        }
        if let Some(v) = get("dnd_enabled") {
            settings.dnd_enabled = v == "true";
        }
        if let Some(v) = get("muted_guilds") {
            if let Ok(guilds) = serde_json::from_str::<Vec<String>>(&v) {
                settings.muted_guilds = guilds.into_iter().collect();
            }
        }
        if let Some(v) = get("muted_channels") {
            if let Ok(channels) = serde_json::from_str::<Vec<String>>(&v) {
                settings.muted_channels = channels.into_iter().collect();
            }
        }
        if let Some(v) = get("preview_overrides") {
            if let Ok(overrides) = serde_json::from_str::<HashMap<String, bool>>(&v) {
                settings.preview_overrides = overrides;
            }
        }

        settings
    }

    pub fn save_setting(conn: &Connection, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }
}

/// DTO for returning notification settings to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettingsDto {
    pub notifications_enabled: bool,
    pub notification_previews: bool,
    pub dnd_enabled: bool,
    pub muted_guilds: Vec<String>,
    pub muted_channels: Vec<String>,
    pub preview_overrides: HashMap<String, bool>,
}

impl From<&NotificationSettings> for NotificationSettingsDto {
    fn from(s: &NotificationSettings) -> Self {
        Self {
            notifications_enabled: s.notifications_enabled,
            notification_previews: s.notification_previews,
            dnd_enabled: s.dnd_enabled,
            muted_guilds: s.muted_guilds.iter().cloned().collect(),
            muted_channels: s.muted_channels.iter().cloned().collect(),
            preview_overrides: s.preview_overrides.clone(),
        }
    }
}

/// Navigation target parsed from a notification identifier.
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationTarget {
    Channel(String),
    DmChannel(String),
}

/// Managed state holding the currently visible channel ID, updated by the frontend.
pub struct VisibleChannelState {
    pub channel_id: Arc<RwLock<Option<String>>>,
}

/// Managed state holding in-memory notification settings.
pub struct NotificationState {
    pub settings: Arc<RwLock<NotificationSettings>>,
}

/// Determines whether a desktop notification should be shown for a message.
/// Returns false if any suppression condition is met.
pub fn should_notify(
    settings: &NotificationSettings,
    is_window_focused: bool,
    is_channel_visible: bool,
    guild_id: Option<&str>,
    channel_id: &str,
) -> bool {
    if !settings.notifications_enabled {
        return false;
    }
    if settings.dnd_enabled {
        return false;
    }
    if let Some(gid) = guild_id {
        if settings.muted_guilds.contains(gid) {
            return false;
        }
    }
    if settings.muted_channels.contains(channel_id) {
        return false;
    }
    if is_window_focused {
        return false;
    }
    if is_channel_visible {
        return false;
    }
    true
}

/// Builds the notification title and body based on privacy settings.
pub fn build_notification_content(
    settings: &NotificationSettings,
    channel_id: &str,
    sender_name: &str,
    plaintext: &str,
) -> (String, String) {
    let title = "OpenConv".to_string();

    let show_preview = settings
        .preview_overrides
        .get(channel_id)
        .copied()
        .unwrap_or(settings.notification_previews);

    let body = if show_preview {
        if plaintext.chars().count() > 100 {
            let truncated: String = plaintext.chars().take(100).collect();
            format!("{sender_name}: {truncated}...")
        } else {
            format!("{sender_name}: {plaintext}")
        }
    } else {
        format!("New message from {sender_name}")
    };

    (title, body)
}

/// Builds a notification identifier encoding the target channel.
pub fn build_notification_identifier(
    channel_id: Option<&str>,
    dm_channel_id: Option<&str>,
) -> String {
    if let Some(dm_id) = dm_channel_id {
        format!("openconv:dm:{dm_id}")
    } else if let Some(ch_id) = channel_id {
        format!("openconv:channel:{ch_id}")
    } else {
        "openconv:unknown".to_string()
    }
}

/// Parses a notification identifier back into a navigation target.
pub fn parse_notification_identifier(identifier: &str) -> Option<NotificationTarget> {
    let parts: Vec<&str> = identifier.splitn(3, ':').collect();
    if parts.len() != 3 || parts[0] != "openconv" {
        return None;
    }
    match parts[1] {
        "channel" => Some(NotificationTarget::Channel(parts[2].to_string())),
        "dm" => Some(NotificationTarget::DmChannel(parts[2].to_string())),
        _ => None,
    }
}

/// Payload emitted when a notification is clicked, for frontend navigation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NotificationClickedPayload {
    pub channel_id: Option<String>,
    pub dm_channel_id: Option<String>,
}

/// Managed state holding the last sent notification target for click routing.
pub struct LastNotificationState {
    pub target: Arc<RwLock<Option<(NotificationTarget, std::time::Instant)>>>,
}

/// Sends a desktop notification if conditions are met.
/// Called from the recv_loop message handler after successful decryption.
pub fn maybe_send_notification(
    app_handle: &tauri::AppHandle,
    settings: &NotificationSettings,
    channel_id: Option<&str>,
    dm_channel_id: Option<&str>,
    guild_id: Option<&str>,
    sender_name: &str,
    plaintext: &str,
    visible_channel_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    use tauri_plugin_notification::NotificationExt;

    let effective_channel_id = dm_channel_id.or(channel_id).unwrap_or("");

    // Check window focus
    let is_window_focused = app_handle
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);

    let is_channel_visible = visible_channel_id
        .map(|v| v == effective_channel_id)
        .unwrap_or(false);

    if !should_notify(
        settings,
        is_window_focused,
        is_channel_visible,
        guild_id,
        effective_channel_id,
    ) {
        return Ok(());
    }

    let (title, body) =
        build_notification_content(settings, effective_channel_id, sender_name, plaintext);
    let identifier = build_notification_identifier(channel_id, dm_channel_id);

    app_handle
        .notification()
        .builder()
        .title(&title)
        .body(&body)
        .auto_cancel()
        .show()
        .map_err(|e| format!("notification send failed: {e}"))?;

    // Store the target for click-to-navigate routing.
    // When the user clicks the notification and the window gains focus,
    // we check this state and emit a navigation event to the frontend.
    if let Some(target) = parse_notification_identifier(&identifier) {
        if let Some(last_notif) = app_handle.try_state::<LastNotificationState>() {
            if let Ok(mut state) = last_notif.target.try_write() {
                *state = Some((target, std::time::Instant::now()));
            }
        }
    }

    tracing::debug!("Sent notification: identifier={identifier}");

    Ok(())
}

/// Check if there's a pending notification click and emit navigation event.
/// Called when the window gains focus. Only emits if a notification was sent
/// recently (within 5 seconds) to avoid false positives from regular focus events.
pub fn check_pending_notification_click(app_handle: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};

    let last_notif = match app_handle.try_state::<LastNotificationState>() {
        Some(s) => s,
        None => return,
    };

    let target = {
        let mut guard = match last_notif.target.try_write() {
            Ok(g) => g,
            Err(_) => return,
        };
        match guard.take() {
            Some((target, sent_at)) if sent_at.elapsed().as_secs() < 5 => Some(target),
            _ => None,
        }
    };

    if let Some(target) = target {
        let payload = match target {
            NotificationTarget::Channel(id) => NotificationClickedPayload {
                channel_id: Some(id),
                dm_channel_id: None,
            },
            NotificationTarget::DmChannel(id) => NotificationClickedPayload {
                channel_id: None,
                dm_channel_id: Some(id),
            },
        };

        if let Err(e) = app_handle.emit("notification:clicked", payload) {
            tracing::warn!("failed to emit notification:clicked: {e}");
        }
    }
}

// --- Tauri Commands ---

/// Check if the app has notification permission from the OS.
#[tauri::command]
#[specta::specta]
pub async fn check_notification_permission(
    app_handle: tauri::AppHandle,
) -> Result<bool, String> {
    use tauri_plugin_notification::NotificationExt;
    let granted = app_handle
        .notification()
        .permission_state()
        .map_err(|e| e.to_string())?;
    Ok(granted == tauri_plugin_notification::PermissionState::Granted)
}

/// Request notification permission from the OS.
#[tauri::command]
#[specta::specta]
pub async fn request_notification_permission(
    app_handle: tauri::AppHandle,
) -> Result<bool, String> {
    use tauri_plugin_notification::NotificationExt;
    let state = app_handle
        .notification()
        .request_permission()
        .map_err(|e| e.to_string())?;
    Ok(state == tauri_plugin_notification::PermissionState::Granted)
}

/// Get all notification settings.
#[tauri::command]
#[specta::specta]
pub async fn get_notification_settings(
    state: tauri::State<'_, NotificationState>,
) -> Result<NotificationSettingsDto, String> {
    let settings = state.settings.read().await;
    Ok(NotificationSettingsDto::from(&*settings))
}

/// Update a notification setting. Persists to the local settings table.
#[tauri::command]
#[specta::specta]
pub async fn update_notification_setting(
    key: String,
    value: String,
    db_state: tauri::State<'_, crate::cache::CacheDb>,
    notif_state: tauri::State<'_, NotificationState>,
) -> Result<(), String> {
    // Persist to DB
    {
        let conn = db_state.lock().map_err(|e| e.to_string())?;
        NotificationSettings::save_setting(&conn, &key, &value).map_err(|e| e.to_string())?;
    }

    // Update in-memory state
    let mut settings = notif_state.settings.write().await;
    match key.as_str() {
        "notifications_enabled" => settings.notifications_enabled = value == "true",
        "notification_previews" => settings.notification_previews = value == "true",
        "dnd_enabled" => settings.dnd_enabled = value == "true",
        "muted_guilds" => {
            if let Ok(guilds) = serde_json::from_str::<Vec<String>>(&value) {
                settings.muted_guilds = guilds.into_iter().collect();
            }
        }
        "muted_channels" => {
            if let Ok(channels) = serde_json::from_str::<Vec<String>>(&value) {
                settings.muted_channels = channels.into_iter().collect();
            }
        }
        "preview_overrides" => {
            if let Ok(overrides) = serde_json::from_str::<HashMap<String, bool>>(&value) {
                settings.preview_overrides = overrides;
            }
        }
        _ => return Err(format!("unknown notification setting key: {key}")),
    }

    Ok(())
}

/// Set the currently visible channel (called by frontend on navigation).
#[tauri::command]
#[specta::specta]
pub async fn set_visible_channel(
    channel_id: Option<String>,
    state: tauri::State<'_, VisibleChannelState>,
) -> Result<(), String> {
    let mut visible = state.channel_id.write().await;
    *visible = channel_id;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_settings() -> NotificationSettings {
        NotificationSettings::default()
    }

    // --- should_notify tests ---

    #[test]
    fn test_notification_fires_when_unfocused_and_channel_not_visible() {
        let settings = default_settings();
        assert!(should_notify(&settings, false, false, Some("g1"), "ch1"));
    }

    #[test]
    fn test_no_notification_when_window_focused() {
        let settings = default_settings();
        assert!(!should_notify(&settings, true, false, Some("g1"), "ch1"));
    }

    #[test]
    fn test_no_notification_when_channel_visible() {
        let settings = default_settings();
        assert!(!should_notify(&settings, false, true, Some("g1"), "ch1"));
    }

    #[test]
    fn test_dnd_suppresses_all() {
        let mut settings = default_settings();
        settings.dnd_enabled = true;
        assert!(!should_notify(&settings, false, false, Some("g1"), "ch1"));
    }

    #[test]
    fn test_muted_guild_suppresses() {
        let mut settings = default_settings();
        settings.muted_guilds.insert("g1".to_string());
        assert!(!should_notify(&settings, false, false, Some("g1"), "ch1"));
    }

    #[test]
    fn test_muted_channel_suppresses() {
        let mut settings = default_settings();
        settings.muted_channels.insert("ch1".to_string());
        assert!(!should_notify(
            &settings, false, false,
            Some("g1"),
            "ch1"
        ));
    }

    #[test]
    fn test_global_disable_suppresses_all() {
        let mut settings = default_settings();
        settings.notifications_enabled = false;
        assert!(!should_notify(&settings, false, false, Some("g1"), "ch1"));
    }

    // --- build_notification_content tests ---

    #[test]
    fn test_privacy_mode_body() {
        let settings = default_settings(); // previews disabled by default
        let (title, body) = build_notification_content(&settings, "ch1", "Alice", "Hello world");
        assert_eq!(title, "OpenConv");
        assert_eq!(body, "New message from Alice");
    }

    #[test]
    fn test_preview_mode_body() {
        let mut settings = default_settings();
        settings.notification_previews = true;
        let (_, body) = build_notification_content(&settings, "ch1", "Alice", "Hello world");
        assert_eq!(body, "Alice: Hello world");
    }

    #[test]
    fn test_preview_mode_truncates_long_message() {
        let mut settings = default_settings();
        settings.notification_previews = true;
        let long_msg = "a".repeat(150);
        let (_, body) = build_notification_content(&settings, "ch1", "Bob", &long_msg);
        // Should be "Bob: " + 100 chars + "..."
        let expected = format!("Bob: {}...", "a".repeat(100));
        assert_eq!(body, expected);
    }

    #[test]
    fn test_per_conversation_preview_override() {
        let mut settings = default_settings();
        settings.notification_previews = false; // global off
        settings
            .preview_overrides
            .insert("ch1".to_string(), true); // channel override on
        let (_, body) = build_notification_content(&settings, "ch1", "Alice", "Secret msg");
        assert_eq!(body, "Alice: Secret msg");
    }

    #[test]
    fn test_per_conversation_preview_override_disables() {
        let mut settings = default_settings();
        settings.notification_previews = true; // global on
        settings
            .preview_overrides
            .insert("ch1".to_string(), false); // channel override off
        let (_, body) = build_notification_content(&settings, "ch1", "Alice", "Secret msg");
        assert_eq!(body, "New message from Alice");
    }

    // --- notification identifier tests ---

    #[test]
    fn test_notification_identifier_encodes_channel_id() {
        let id = build_notification_identifier(Some("abc-123"), None);
        assert_eq!(id, "openconv:channel:abc-123");
    }

    #[test]
    fn test_notification_identifier_encodes_dm_channel_id() {
        let id = build_notification_identifier(None, Some("dm-456"));
        assert_eq!(id, "openconv:dm:dm-456");
    }

    #[test]
    fn test_dm_takes_precedence_over_channel() {
        let id = build_notification_identifier(Some("ch-1"), Some("dm-1"));
        assert_eq!(id, "openconv:dm:dm-1");
    }

    #[test]
    fn test_parse_notification_identifier_channel() {
        let target = parse_notification_identifier("openconv:channel:abc-123");
        assert_eq!(
            target,
            Some(NotificationTarget::Channel("abc-123".to_string()))
        );
    }

    #[test]
    fn test_parse_notification_identifier_dm() {
        let target = parse_notification_identifier("openconv:dm:dm-456");
        assert_eq!(
            target,
            Some(NotificationTarget::DmChannel("dm-456".to_string()))
        );
    }

    #[test]
    fn test_parse_notification_identifier_invalid() {
        assert_eq!(parse_notification_identifier("invalid"), None);
        assert_eq!(parse_notification_identifier("openconv:unknown:x"), None);
        assert_eq!(parse_notification_identifier(""), None);
    }

    // --- settings persistence tests ---

    #[test]
    fn test_settings_load_defaults_when_empty() {
        let db = crate::cache::CacheDb::open_in_memory();
        let conn = db.lock().unwrap();
        let settings = NotificationSettings::load_from_db(&conn);
        assert!(settings.notifications_enabled);
        assert!(!settings.notification_previews);
        assert!(!settings.dnd_enabled);
        assert!(settings.muted_guilds.is_empty());
        assert!(settings.muted_channels.is_empty());
        assert!(settings.preview_overrides.is_empty());
    }

    #[test]
    fn test_settings_save_and_load_roundtrip() {
        let db = crate::cache::CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        NotificationSettings::save_setting(&conn, "notifications_enabled", "false").unwrap();
        NotificationSettings::save_setting(&conn, "notification_previews", "true").unwrap();
        NotificationSettings::save_setting(&conn, "dnd_enabled", "true").unwrap();
        NotificationSettings::save_setting(
            &conn,
            "muted_guilds",
            r#"["g1","g2"]"#,
        )
        .unwrap();
        NotificationSettings::save_setting(
            &conn,
            "muted_channels",
            r#"["ch1"]"#,
        )
        .unwrap();
        NotificationSettings::save_setting(
            &conn,
            "preview_overrides",
            r#"{"ch2":true}"#,
        )
        .unwrap();

        let loaded = NotificationSettings::load_from_db(&conn);
        assert!(!loaded.notifications_enabled);
        assert!(loaded.notification_previews);
        assert!(loaded.dnd_enabled);
        assert!(loaded.muted_guilds.contains("g1"));
        assert!(loaded.muted_guilds.contains("g2"));
        assert!(loaded.muted_channels.contains("ch1"));
        assert_eq!(loaded.preview_overrides.get("ch2"), Some(&true));
    }

    #[test]
    fn test_settings_save_overwrites() {
        let db = crate::cache::CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        NotificationSettings::save_setting(&conn, "dnd_enabled", "true").unwrap();
        NotificationSettings::save_setting(&conn, "dnd_enabled", "false").unwrap();

        let loaded = NotificationSettings::load_from_db(&conn);
        assert!(!loaded.dnd_enabled);
    }
}
