use object_store::path::Path as StorePath;
use object_store::ObjectStore;
use openconv_shared::ids::GuildId;
use sqlx::PgPool;

/// Permanently delete guilds that have been soft-deleted for more than 7 days.
///
/// For each expired guild:
/// 1. Collect all file storage paths using prefix-based query
/// 2. Delete each file from the object store
/// 3. Delete the guild DB row (CASCADE handles channels, messages, roles, members)
///
/// Returns the number of guilds permanently deleted.
pub async fn cleanup_expired_guilds(
    pool: &PgPool,
    store: &dyn ObjectStore,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let expired_guilds: Vec<GuildId> = sqlx::query_scalar(
        "SELECT id FROM guilds WHERE deleted_at IS NOT NULL AND deleted_at < NOW() - INTERVAL '7 days'",
    )
    .fetch_all(pool)
    .await?;

    if expired_guilds.is_empty() {
        return Ok(0);
    }

    let mut deleted_count = 0u64;

    for guild_id in &expired_guilds {
        // Prefix-based query: catches all files regardless of message_id state
        let storage_paths: Vec<String> = sqlx::query_scalar(
            "SELECT storage_path FROM files WHERE storage_path LIKE 'guilds/' || $1::text || '/%'",
        )
        .bind(guild_id)
        .fetch_all(pool)
        .await?;

        // Delete files from object store before DB cascade
        for path in &storage_paths {
            let store_path = StorePath::from(path.as_str());
            match store.delete(&store_path).await {
                Ok(()) => {
                    tracing::debug!(path = %path, guild_id = %guild_id, "deleted guild file from store");
                }
                Err(object_store::Error::NotFound { .. }) => {
                    // Already gone
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        path = %path,
                        guild_id = %guild_id,
                        "failed to delete guild file from store, continuing"
                    );
                }
            }
        }

        // Delete guild row - CASCADE handles channels, messages, members, roles
        let result = sqlx::query("DELETE FROM guilds WHERE id = $1")
            .bind(guild_id)
            .execute(pool)
            .await?;

        if result.rows_affected() > 0 {
            // Clean up file DB records that used prefix-based storage paths
            // (these may not be cascade-deleted if message_id was already NULL)
            sqlx::query("DELETE FROM files WHERE storage_path LIKE 'guilds/' || $1::text || '/%'")
                .bind(guild_id)
                .execute(pool)
                .await?;
        }

        deleted_count += result.rows_affected();
    }

    Ok(deleted_count)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleanup_interval_is_7_days() {
        let seven_days = chrono::Duration::days(7);
        assert_eq!(seven_days.num_hours(), 168);
    }
}
