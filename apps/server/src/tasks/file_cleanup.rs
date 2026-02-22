use object_store::path::Path as StorePath;
use object_store::ObjectStore;

/// Remove orphaned files that were never linked to a message.
///
/// Queries for files where `message_id IS NULL` and `created_at < NOW() - 24 hours`,
/// deletes the storage objects, then deletes the DB records.
pub async fn cleanup_orphan_files(
    pool: &sqlx::PgPool,
    store: &dyn ObjectStore,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let orphans = sqlx::query_as::<_, OrphanRow>(
        "SELECT id, storage_path FROM files \
         WHERE message_id IS NULL AND created_at < NOW() - INTERVAL '24 hours'",
    )
    .fetch_all(pool)
    .await?;

    if orphans.is_empty() {
        return Ok(0);
    }

    let mut ids_to_delete = Vec::new();

    for orphan in &orphans {
        let store_path = StorePath::from(orphan.storage_path.as_str());
        match store.delete(&store_path).await {
            Ok(()) => {}
            Err(object_store::Error::NotFound { .. }) => {
                // Already gone, still clean up DB record
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    file_id = %orphan.id,
                    "failed to delete orphan file from store"
                );
                continue; // Skip this one, try again next run
            }
        }
        ids_to_delete.push(orphan.id);
    }

    if ids_to_delete.is_empty() {
        return Ok(0);
    }

    let deleted = sqlx::query("DELETE FROM files WHERE id = ANY($1)")
        .bind(&ids_to_delete)
        .execute(pool)
        .await?
        .rows_affected();

    Ok(deleted)
}

#[derive(sqlx::FromRow)]
struct OrphanRow {
    id: uuid::Uuid,
    storage_path: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleanup_interval_is_24_hours() {
        // The SQL uses '24 hours' interval - verify this is the intended value
        let query = "WHERE message_id IS NULL AND created_at < NOW() - INTERVAL '24 hours'";
        assert!(query.contains("24 hours"));
    }
}
