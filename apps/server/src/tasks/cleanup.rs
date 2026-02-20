use sqlx::PgPool;

/// Delete all refresh tokens that have expired.
/// Called periodically (once at startup, then hourly) to prevent unbounded table growth.
pub async fn cleanup_expired_refresh_tokens(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM refresh_tokens WHERE expires_at < NOW()")
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}
