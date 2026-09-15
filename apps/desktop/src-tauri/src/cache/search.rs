#![allow(dead_code)]

use crate::auth_service::AppError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum SearchScope {
    AllMessages,
    Guild(String),
    Channel(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub message_id: String,
    pub channel_id: Option<String>,
    pub sender_id: String,
    pub snippet: String,
    pub created_at: i64,
}

pub fn index_message(conn: &Connection, message_id: &str, plaintext: &str) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO messages_fts (message_id, plaintext) VALUES (?1, ?2)",
        rusqlite::params![message_id, plaintext],
    )?;
    Ok(())
}

pub fn deindex_message(conn: &Connection, message_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM messages_fts WHERE message_id = ?1",
        [message_id],
    )?;
    Ok(())
}

pub fn reindex_message(
    conn: &Connection,
    message_id: &str,
    new_plaintext: &str,
) -> Result<(), AppError> {
    deindex_message(conn, message_id)?;
    index_message(conn, message_id, new_plaintext)?;
    Ok(())
}

fn sanitize_fts_query(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' | '*' | '(' | ')' => {
                // Skip FTS5 special characters
            }
            _ => result.push(ch),
        }
    }
    // Remove FTS5 boolean keywords
    let result = result.replace(" AND ", " ");
    let result = result.replace(" OR ", " ");
    let result = result.replace(" NOT ", " ");
    let result = result.replace(" NEAR ", " ");
    // Quote each word to make it a phrase-like search
    let words: Vec<&str> = result.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    // Wrap individual words in double quotes for safe matching
    words
        .iter()
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn search_messages(
    conn: &Connection,
    query: &str,
    scope: &SearchScope,
    limit: u32,
) -> Result<Vec<SearchResult>, AppError> {
    let sanitized = sanitize_fts_query(query);
    if sanitized.is_empty() {
        return Ok(Vec::new());
    }

    let limit = if limit == 0 { 50 } else { limit.min(50) };

    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match scope {
        SearchScope::AllMessages => {
            let sql = "SELECT f.message_id, m.channel_id, m.sender_id, snippet(messages_fts, 1, '<b>', '</b>', '...', 32), m.created_at
                 FROM messages_fts f
                 JOIN messages m ON m.id = f.message_id
                 WHERE messages_fts MATCH ?1
                 ORDER BY bm25(messages_fts) - (1.0 / ((?2 - m.created_at) / 86400000.0 + 1.0))
                 LIMIT ?3".to_string();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| AppError::new(e.to_string()))?
                .as_millis() as i64;
            (
                sql,
                vec![
                    Box::new(sanitized) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(now),
                    Box::new(limit),
                ],
            )
        }
        SearchScope::Channel(channel_id) => {
            let sql = "SELECT f.message_id, m.channel_id, m.sender_id, snippet(messages_fts, 1, '<b>', '</b>', '...', 32), m.created_at
                 FROM messages_fts f
                 JOIN messages m ON m.id = f.message_id
                 WHERE messages_fts MATCH ?1 AND m.channel_id = ?2
                 ORDER BY bm25(messages_fts) - (1.0 / ((?3 - m.created_at) / 86400.0 + 1.0))
                 LIMIT ?4".to_string();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| AppError::new(e.to_string()))?
                .as_millis() as i64;
            (
                sql,
                vec![
                    Box::new(sanitized) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(channel_id.clone()),
                    Box::new(now),
                    Box::new(limit),
                ],
            )
        }
        SearchScope::Guild(guild_id) => {
            let sql = "SELECT f.message_id, m.channel_id, m.sender_id, snippet(messages_fts, 1, '<b>', '</b>', '...', 32), m.created_at
                 FROM messages_fts f
                 JOIN messages m ON m.id = f.message_id
                 JOIN channel_cache cc ON cc.id = m.channel_id
                 WHERE messages_fts MATCH ?1 AND cc.guild_id = ?2
                 ORDER BY bm25(messages_fts) - (1.0 / ((?3 - m.created_at) / 86400.0 + 1.0))
                 LIMIT ?4".to_string();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| AppError::new(e.to_string()))?
                .as_millis() as i64;
            (
                sql,
                vec![
                    Box::new(sanitized) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(guild_id.clone()),
                    Box::new(now),
                    Box::new(limit),
                ],
            )
        }
    };

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(SearchResult {
            message_id: row.get(0)?,
            channel_id: row.get(1)?,
            sender_id: row.get(2)?,
            snippet: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::messages::{insert_message, CachedMessage};

    fn test_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
        )
        .unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::cache::migrations::run_cache_migrations(&conn).unwrap();
        conn
    }

    fn insert_and_index(
        conn: &Connection,
        id: &str,
        channel_id: &str,
        text: &str,
        created_at: i64,
    ) {
        let msg = CachedMessage {
            id: id.to_string(),
            channel_id: Some(channel_id.to_string()),
            dm_channel_id: None,
            sender_id: "u1".to_string(),
            sender_device_id: None,
            sender_signal_device_id: None,
            plaintext: Some(text.to_string()),
            ciphertext: None,
            message_type: None,
            created_at,
            edited_at: None,
            decrypted_at: None,
            status: "delivered".to_string(),
        };
        insert_message(conn, &msg).unwrap();
        index_message(conn, id, text).unwrap();
    }

    #[test]
    fn test_search_with_snippets() {
        let conn = test_conn();
        insert_and_index(
            &conn,
            "m1",
            "ch1",
            "the quick brown fox jumps over the lazy dog",
            1000,
        );

        let results = search_messages(&conn, "quick brown", &SearchScope::AllMessages, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message_id, "m1");
        assert!(!results[0].snippet.is_empty());
    }

    #[test]
    fn test_search_escapes_special_chars() {
        let conn = test_conn();
        insert_and_index(&conn, "m1", "ch1", "testing special characters", 1000);

        // Should not crash with special FTS5 chars
        let results =
            search_messages(&conn, "testing*\"()", &SearchScope::AllMessages, 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_no_matches() {
        let conn = test_conn();
        insert_and_index(&conn, "m1", "ch1", "hello world", 1000);

        let results = search_messages(&conn, "nonexistent", &SearchScope::AllMessages, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_default_limit() {
        let conn = test_conn();
        for i in 0..60 {
            insert_and_index(
                &conn,
                &format!("m{i}"),
                "ch1",
                &format!("common keyword message {i}"),
                1000 + i,
            );
        }

        // Using limit=0 should default to 50
        let results = search_messages(&conn, "common", &SearchScope::AllMessages, 0).unwrap();
        assert!(
            results.len() <= 50,
            "should limit to 50 results, got {}",
            results.len()
        );
    }

    #[test]
    fn test_search_recency_boost() {
        let conn = test_conn();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Old message (30 days ago in ms)
        insert_and_index(
            &conn,
            "old",
            "ch1",
            "identical keyword here",
            now - 86400_000 * 30,
        );
        // Recent message (1 minute ago in ms)
        insert_and_index(&conn, "new", "ch1", "identical keyword here", now - 60_000);

        let results =
            search_messages(&conn, "identical keyword", &SearchScope::AllMessages, 10).unwrap();
        assert_eq!(results.len(), 2);
        // Recent message should rank higher (appear first)
        assert_eq!(results[0].message_id, "new");
    }
}
