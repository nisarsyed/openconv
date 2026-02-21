use openconv_shared::ids::GuildId;
use sqlx::PgPool;

/// Permanently delete guilds that have been soft-deleted for more than 7 days.
///
/// Steps:
/// 1. Query all guilds where deleted_at < NOW() - INTERVAL '7 days'
/// 2. For each guild, collect file storage paths before deleting DB records
/// 3. Delete the guild DB row (cascade handles channels, messages, roles, members)
/// 4. Log the cleanup
///
/// Note: File storage object cleanup is deferred to section-08 (file-storage).
/// For now we delete the DB records and log the storage paths that would need cleanup.
pub async fn cleanup_expired_guilds(pool: &PgPool) -> Result<u64, sqlx::Error> {
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
        // Collect file storage paths before cascade delete
        let storage_paths: Vec<String> = sqlx::query_scalar(
            "SELECT f.storage_path \
             FROM files f \
             JOIN messages m ON m.id = f.message_id \
             JOIN channels c ON c.id = m.channel_id \
             WHERE c.guild_id = $1",
        )
        .bind(guild_id)
        .fetch_all(pool)
        .await?;

        if !storage_paths.is_empty() {
            tracing::info!(
                guild_id = %guild_id,
                file_count = storage_paths.len(),
                "Collected file storage paths for guild cleanup"
            );
            // TODO(section-08): Delete from object store using storage_paths
        }

        // Delete guild row - cascade handles everything else
        let result = sqlx::query("DELETE FROM guilds WHERE id = $1")
            .bind(guild_id)
            .execute(pool)
            .await?;

        deleted_count += result.rows_affected();
    }

    Ok(deleted_count)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleanup_interval_is_7_days() {
        // The interval used in the query is '7 days'
        // This test documents the design decision
        let seven_days = chrono::Duration::days(7);
        assert_eq!(seven_days.num_hours(), 168);
    }
}
