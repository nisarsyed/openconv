use tauri::State;

use crate::auth_service::AppError;
use crate::cache::read_positions::{self, ReadPositionInfo};
use crate::cache::CacheDb;

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
    read_positions::clear_unread(&conn, &channel_id, &last_message_id, last_message_created_at)
}

/// Trigger a batch sync of unsynced read positions to the server.
/// Called periodically (every 30s) and on app minimize/close.
///
/// Currently a no-op until the server REST endpoints are wired in.
/// Positions remain marked as unsynced so they will be sent once the
/// POST /api/users/me/read-state endpoint is implemented.
#[tauri::command]
#[specta::specta]
pub async fn sync_read_positions(
    _cache_db: State<'_, CacheDb>,
) -> Result<(), AppError> {
    // TODO: POST /api/users/me/read-state with unsynced positions.
    // Do NOT mark positions as synced here -- that would lose data.
    // Once the server endpoint is available:
    //   1. get_unsynced() to collect pending positions
    //   2. POST to server
    //   3. On success: mark_positions_synced() for sent channel IDs
    Ok(())
}

/// Fetch read positions from server and merge with local state.
/// Called on app launch.
///
/// Currently returns local positions only until the server REST
/// endpoints are wired in.
#[tauri::command]
#[specta::specta]
pub async fn fetch_and_merge_read_positions(
    cache_db: State<'_, CacheDb>,
) -> Result<Vec<ReadPositionInfo>, AppError> {
    // TODO: GET /api/users/me/read-state to fetch server positions,
    // then call read_positions::merge_read_positions() before returning.
    let conn = cache_db.lock()?;
    let positions = read_positions::get_all_read_positions(&conn)?;
    Ok(positions.into_iter().map(ReadPositionInfo::from).collect())
}
