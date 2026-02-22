use std::collections::HashSet;

use fred::prelude::*;
use futures::future::join_all;
use openconv_shared::ids::{ChannelId, DeviceId, MessageId, UserId};
use tokio::sync::mpsc;

use super::types::ServerMessage;

const LAST_SEEN_TTL_SECS: i64 = 86400; // 24 hours
const MAX_REPLAY_MESSAGES: i64 = 500;

/// Build the Redis key for last_seen timestamp.
fn last_seen_key(user_id: UserId, channel_id: ChannelId) -> String {
    format!("user:{user_id}:last_seen:{channel_id}")
}

/// Store last_seen timestamps for all subscribed channels on disconnect.
/// Uses concurrent Redis calls for efficiency.
pub async fn store_last_seen(
    redis: &fred::clients::Pool,
    user_id: UserId,
    subscribed_channels: &HashSet<ChannelId>,
) {
    if subscribed_channels.is_empty() {
        return;
    }

    let now = chrono::Utc::now().timestamp().to_string();

    let futures: Vec<_> = subscribed_channels
        .iter()
        .map(|&channel_id| {
            let key = last_seen_key(user_id, channel_id);
            let ts = now.clone();
            let r = redis.clone();
            async move {
                if let Err(e) = r
                    .set::<(), _, _>(
                        &key,
                        ts.as_str(),
                        Some(Expiration::EX(LAST_SEEN_TTL_SECS)),
                        None,
                        false,
                    )
                    .await
                {
                    tracing::warn!(
                        user_id = %user_id,
                        channel_id = %channel_id,
                        error = %e,
                        "failed to store last_seen timestamp"
                    );
                }
            }
        })
        .collect();

    join_all(futures).await;
}

/// Replay missed messages for a channel subscription.
/// Returns the number of messages replayed, or 0 if no replay was needed.
/// Capped at MAX_REPLAY_MESSAGES; client should use REST pagination for more.
pub async fn replay_missed_messages(
    db: &sqlx::PgPool,
    redis: &fred::clients::Pool,
    user_id: UserId,
    device_id: DeviceId,
    channel_id: ChannelId,
    sender: &mpsc::Sender<ServerMessage>,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let key = last_seen_key(user_id, channel_id);

    // Get last_seen timestamp from Redis
    let ts_str: Option<String> = redis.get(&key).await?;

    let ts_str = match ts_str {
        Some(s) => s,
        None => return Ok(0), // No last_seen — skip replay
    };

    let last_seen_ts: i64 = ts_str.parse()?;
    let last_seen = chrono::DateTime::from_timestamp(last_seen_ts, 0).ok_or("invalid timestamp")?;

    // Query messages with per-device ciphertext since last_seen (capped)
    let rows: Vec<ReplayRow> = sqlx::query_as(
        "SELECT m.id, m.channel_id, m.sender_id, m.created_at, \
                mr.ciphertext, mr.message_type \
         FROM messages m \
         JOIN message_recipients mr ON mr.message_id = m.id \
         WHERE m.channel_id = $1 AND m.created_at > $2 AND m.deleted = false \
           AND mr.user_id = $3 AND mr.device_id = $4 \
         ORDER BY m.created_at ASC \
         LIMIT $5",
    )
    .bind(channel_id)
    .bind(last_seen)
    .bind(user_id)
    .bind(device_id)
    .bind(MAX_REPLAY_MESSAGES)
    .fetch_all(db)
    .await?;

    let count = rows.len() as u64;

    // Send each as MessageCreated with inline ciphertext
    for row in rows {
        let event = ServerMessage::MessageCreated {
            channel_id: row.channel_id,
            message_id: row.id,
            sender_id: row.sender_id,
            ciphertext: row.ciphertext,
            message_type: row.message_type,
            created_at: row.created_at,
        };
        if sender.send(event).await.is_err() {
            // Connection closed during replay
            return Ok(count);
        }
    }

    // Send ReplayComplete
    let _ = sender
        .send(ServerMessage::ReplayComplete { channel_id })
        .await;

    // Delete the Redis key — replay is done
    let _: () = redis.del(&key).await.unwrap_or(());

    Ok(count)
}

#[derive(sqlx::FromRow)]
struct ReplayRow {
    id: MessageId,
    channel_id: ChannelId,
    sender_id: UserId,
    created_at: chrono::DateTime<chrono::Utc>,
    ciphertext: Vec<u8>,
    message_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_seen_key_format() {
        let uid = UserId::new();
        let cid = ChannelId::new();
        let key = last_seen_key(uid, cid);
        assert!(key.starts_with("user:"));
        assert!(key.contains(":last_seen:"));
        assert!(key.ends_with(&cid.to_string()));
    }

    #[test]
    fn last_seen_ttl_is_24_hours() {
        assert_eq!(LAST_SEEN_TTL_SECS, 86400);
    }

    #[test]
    fn max_replay_cap_is_500() {
        assert_eq!(MAX_REPLAY_MESSAGES, 500);
    }

    // Integration tests for replay require Redis + DB — placed in apps/server/tests/.
}
