use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use openconv_server::config::ServerConfig;
use openconv_server::email::{EmailService, MockEmailService, SmtpEmailService};
use openconv_server::jwt::JwtService;
use openconv_server::redis::create_redis_pool;
use openconv_server::router::build_router;
use openconv_server::shutdown::shutdown_signal;
use openconv_server::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let config = ServerConfig::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(config.max_db_connections)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!().run(&pool).await?;

    let redis = create_redis_pool(&config.redis).await?;
    tracing::info!("Redis connected");

    let jwt = Arc::new(JwtService::new(&config.jwt)?);
    tracing::info!("JWT service initialized");

    let email: Arc<dyn EmailService> = if config.email.smtp_host.is_empty() {
        tracing::warn!("SMTP not configured, using mock email service");
        Arc::new(MockEmailService::new())
    } else {
        Arc::new(SmtpEmailService::new(&config.email)?)
    };

    // Shutdown coordination: cleanup task stops when the server does
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        loop {
            match openconv_server::tasks::cleanup::cleanup_expired_refresh_tokens(&cleanup_pool)
                .await
            {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!("Cleaned up {count} expired refresh tokens");
                    }
                }
                Err(e) => tracing::error!("Refresh token cleanup failed: {e}"),
            }
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(3600)) => {}
                _ = shutdown_rx.changed() => {
                    tracing::info!("Cleanup task shutting down");
                    break;
                }
            }
        }
    });

    let addr = format!("{}:{}", config.host, config.port);
    let state = AppState {
        db: pool,
        config: Arc::new(config),
        redis,
        jwt,
        email,
    };
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    let _ = shutdown_tx.send(true);

    Ok(())
}
