#![allow(dead_code)]

use crate::auth_service::AppError;
use rusqlite::Connection;

/// A cached DM channel entry.
pub struct CachedDmChannel {
    pub id: String,
    pub participant_ids: Vec<String>,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<i64>,
    pub created_at: i64,
}

pub fn upsert_dm_channel(conn: &Connection, dm: &CachedDmChannel) -> Result<(), AppError> {
    let participants_json =
        serde_json::to_string(&dm.participant_ids).map_err(|e| AppError::new(e.to_string()))?;

    conn.execute(
        "INSERT INTO dm_channel_cache (id, participant_ids, last_message_preview, last_message_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            participant_ids = excluded.participant_ids,
            last_message_preview = excluded.last_message_preview,
            last_message_at = excluded.last_message_at",
        rusqlite::params![
            dm.id,
            participants_json,
            dm.last_message_preview,
            dm.last_message_at,
            dm.created_at,
        ],
    )?;
    Ok(())
}

pub fn get_dm_channel(
    conn: &Connection,
    dm_channel_id: &str,
) -> Result<Option<CachedDmChannel>, AppError> {
    let result = conn.query_row(
        "SELECT id, participant_ids, last_message_preview, last_message_at, created_at
         FROM dm_channel_cache WHERE id = ?1",
        [dm_channel_id],
        |row| {
            let participants_json: String = row.get(1)?;
            Ok((
                row.get::<_, String>(0)?,
                participants_json,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    );
    match result {
        Ok((id, participants_json, preview, last_at, created_at)) => {
            let participant_ids: Vec<String> = serde_json::from_str(&participants_json)
                .map_err(|e| AppError::new(e.to_string()))?;
            Ok(Some(CachedDmChannel {
                id,
                participant_ids,
                last_message_preview: preview,
                last_message_at: last_at,
                created_at,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_dm_channel_by_participant(
    conn: &Connection,
    user_id: &str,
) -> Result<Option<CachedDmChannel>, AppError> {
    let result = conn.query_row(
        "SELECT d.id, d.participant_ids, d.last_message_preview, d.last_message_at, d.created_at
         FROM dm_channel_cache d, json_each(d.participant_ids) j
         WHERE j.value = ?1 LIMIT 1",
        [user_id],
        |row| {
            let participants_json: String = row.get(1)?;
            Ok((
                row.get::<_, String>(0)?,
                participants_json,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    );
    match result {
        Ok((id, participants_json, preview, last_at, created_at)) => {
            let participant_ids: Vec<String> = serde_json::from_str(&participants_json)
                .map_err(|e| AppError::new(e.to_string()))?;
            Ok(Some(CachedDmChannel {
                id,
                participant_ids,
                last_message_preview: preview,
                last_message_at: last_at,
                created_at,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn list_dm_channels(conn: &Connection) -> Result<Vec<CachedDmChannel>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, participant_ids, last_message_preview, last_message_at, created_at
         FROM dm_channel_cache
         ORDER BY COALESCE(last_message_at, created_at) DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let participants_json: String = row.get(1)?;
        Ok((
            row.get::<_, String>(0)?,
            participants_json,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    let mut channels = Vec::new();
    for row in rows {
        let (id, participants_json, preview, last_at, created_at) = row?;
        let participant_ids: Vec<String> =
            serde_json::from_str(&participants_json).map_err(|e| AppError::new(e.to_string()))?;
        channels.push(CachedDmChannel {
            id,
            participant_ids,
            last_message_preview: preview,
            last_message_at: last_at,
            created_at,
        });
    }
    Ok(channels)
}

pub fn update_dm_last_message(
    conn: &Connection,
    dm_channel_id: &str,
    preview: &str,
    timestamp: i64,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE dm_channel_cache SET last_message_preview = ?1, last_message_at = ?2 WHERE id = ?3",
        rusqlite::params![preview, timestamp, dm_channel_id],
    )?;
    Ok(())
}
