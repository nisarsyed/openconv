use tauri::State;

use openconv_shared::api::sync::{BatchUpdateReadStateRequest, ReadStateEntry, ReadStateUpdate};
use openconv_shared::ids::{ChannelId, MessageId};

use crate::auth_service::{self, AppError};
use crate::cache::read_positions::{self, ReadPositionInfo, ServerReadPosition};
use crate::cache::CacheDb;

/// Response from GET /api/users/me/read-state.
#[derive(serde::Deserialize)]
struct ReadStateResponse {
    read_positions: Vec<ReadStateEntry>,
}

/// Get all read positions for initializing frontend state.
#[tauri::command]
#[specta::specta]
pub async fn get_read_positions(
    cache_db: State<'_, CacheDb>,
) -> Result<Vec<ReadPositionInfo>, AppError> {
    let conn = cache_db.lock()?;
    let positions = read_positions::get_all_read_positions(&conn)?;
    Ok(positions.into_iter().map(ReadPositionInfo::from).collect())
}

/// Mark a channel as read. Called when user views a channel.
#[tauri::command]
#[specta::specta]
pub async fn mark_channel_read(
    channel_id: String,
    last_message_id: String,
    last_message_created_at: i64,
    cache_db: State<'_, CacheDb>,
) -> Result<(), AppError> {
    let conn = cache_db.lock()?;
    read_positions::clear_unread(
        &conn,
        &channel_id,
        &last_message_id,
        last_message_created_at,
    )
}

/// Trigger a batch sync of unsynced read positions to the server.
/// Called periodically (every 30s) and on app minimize/close.
///
/// 1. Collects unsynced positions from local cache
/// 2. POSTs them to the server
/// 3. On success, marks them as synced locally
#[tauri::command]
#[specta::specta]
pub async fn sync_read_positions(cache_db: State<'_, CacheDb>) -> Result<(), AppError> {
    // 1. Collect unsynced positions
    let unsynced = {
        let conn = cache_db.lock()?;
        read_positions::get_unsynced(&conn)?
    };

    if unsynced.is_empty() {
        return Ok(());
    }

    // 2. Build request payload
    let entries: Vec<ReadStateUpdate> = unsynced
        .iter()
        .filter_map(|rp| {
            let channel_id: ChannelId = rp.channel_id.parse().ok()?;
            let last_read_message_id: MessageId = rp.last_read_message_id.parse().ok()?;
            Some(ReadStateUpdate {
                channel_id,
                last_read_message_id,
            })
        })
        .collect();

    if entries.is_empty() {
        return Ok(());
    }

    let channel_ids: Vec<String> = unsynced.iter().map(|rp| rp.channel_id.clone()).collect();

    // 3. POST to server
    let access_token = tokio::task::spawn_blocking(auth_service::get_access_token)
        .await
        .map_err(|e| AppError::new(format!("internal error: {e}")))?
        .map_err(|_| AppError::new("not authenticated"))?;

    let api_base_url =
        std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_base_url}/api/users/me/read-state"))
        .bearer_auth(&access_token)
        .json(&BatchUpdateReadStateRequest { entries })
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::new(format!(
            "sync read positions failed ({status}): {body}"
        )));
    }

    // 4. Mark as synced locally
    {
        let conn = cache_db.lock()?;
        let refs: Vec<&str> = channel_ids.iter().map(|s| s.as_str()).collect();
        read_positions::mark_positions_synced(&conn, &refs)?;
    }

    Ok(())
}

/// Fetch read positions from server and merge with local state.
/// Called on app launch.
///
/// 1. Fetches server-side read positions
/// 2. Merges with local state (server wins if more recent)
/// 3. Returns all positions for frontend initialization
#[tauri::command]
#[specta::specta]
pub async fn fetch_and_merge_read_positions(
    cache_db: State<'_, CacheDb>,
) -> Result<Vec<ReadPositionInfo>, AppError> {
    // 1. Fetch from server
    let access_token = tokio::task::spawn_blocking(auth_service::get_access_token)
        .await
        .map_err(|e| AppError::new(format!("internal error: {e}")))?;

    // If not authenticated, just return local positions
    let access_token = match access_token {
        Ok(t) => t,
        Err(_) => {
            let conn = cache_db.lock()?;
            let positions = read_positions::get_all_read_positions(&conn)?;
            return Ok(positions.into_iter().map(ReadPositionInfo::from).collect());
        }
    };

    let api_base_url =
        std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{api_base_url}/api/users/me/read-state"))
        .bearer_auth(&access_token)
        .send()
        .await;

    // 2. Merge server positions with local
    if let Ok(resp) = resp {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<ReadStateResponse>().await {
                let server_positions: Vec<ServerReadPosition> = body
                    .read_positions
                    .into_iter()
                    .map(|entry| ServerReadPosition {
                        channel_id: entry.channel_id.to_string(),
                        last_read_message_id: entry.last_read_message_id.to_string(),
                        updated_at: entry.updated_at.timestamp_millis(),
                    })
                    .collect();

                if !server_positions.is_empty() {
                    let conn = cache_db.lock()?;
                    read_positions::merge_read_positions(&conn, &server_positions)?;
                }
            }
        }
    }

    // 3. Return merged positions
    let conn = cache_db.lock()?;
    let positions = read_positions::get_all_read_positions(&conn)?;
    Ok(positions.into_iter().map(ReadPositionInfo::from).collect())
}
