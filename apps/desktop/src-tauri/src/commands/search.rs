use tauri::State;

use crate::cache::search::{self, SearchResult, SearchScope};
use crate::cache::CacheDb;

#[tauri::command]
#[specta::specta]
pub async fn search_messages(
    cache_db: State<'_, CacheDb>,
    query: String,
    scope: SearchScope,
    limit: Option<u32>,
) -> Result<Vec<SearchResult>, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let conn = cache_db.lock().map_err(|e| e.to_string())?;
    search::search_messages(&conn, &query, &scope, limit.unwrap_or(50)).map_err(|e| e.to_string())
}
