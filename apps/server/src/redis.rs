use fred::prelude::*;

use crate::config::RedisConfig;

/// Initialize a Redis connection pool from config.
/// Returns an error if the connection cannot be established.
pub async fn create_redis_pool(
    config: &RedisConfig,
) -> Result<fred::clients::Pool, fred::error::Error> {
    let redis_config = Config::from_url(&config.url)?;
    let pool = fred::clients::Pool::new(redis_config, None, None, None, 5)?;
    pool.init().await?;
    pool.wait_for_connect().await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn redis_pool_fails_gracefully_with_invalid_url() {
        let config = RedisConfig {
            url: "redis://invalid-host-that-does-not-exist:9999".to_string(),
        };
        let result: Result<fred::clients::Pool, _> = create_redis_pool(&config).await;
        assert!(result.is_err());
    }
}
