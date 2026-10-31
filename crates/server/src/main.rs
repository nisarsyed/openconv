//! Binary entry point for the OpenConv relay.

use openconv_server::{Config, config, store::Store};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openconv_server=info".into()),
        )
        .init();

    let addr: SocketAddr = std::env::var("OPENCONV_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()
        .expect("OPENCONV_ADDR must be host:port");

    // Where frames are kept. Relative to the working directory by default;
    // point it somewhere durable for anything that is not a dev run.
    let db = std::env::var("OPENCONV_RELAY_DB").unwrap_or_else(|_| "openconv-relay.db".into());
    let existed = std::path::Path::new(&db).exists();
    let store = Store::open(&db).expect("open the relay store");
    let path = std::fs::canonicalize(&db).map_or(db.clone(), |p| p.display().to_string());
    if existed {
        tracing::info!(store = path, "relay store open");
    } else {
        // Loud on purpose. A relay started from another directory quietly
        // gets a fresh store, and every client that used the old one finds
        // itself ahead of this one's history.
        tracing::warn!(
            store = path,
            "created a new, empty relay store; clients that used another one will resync to it. \
             Set OPENCONV_RELAY_DB to keep using a store across directories"
        );
    }

    // How long frames are kept, how much, and how large one may be: see
    // `Config` and `Retention`.
    // Malformed values stop the relay rather than being guessed at.
    let mut settings = Config::default();
    if let Ok(text) = std::env::var("OPENCONV_RETENTION") {
        settings.retention.window = config::parse_duration(&text)
            .expect("OPENCONV_RETENTION must be a duration such as 30d, 24h or 90m");
    }
    if let Ok(text) = std::env::var("OPENCONV_CHANNEL_CAP") {
        settings.retention.channel_cap = config::parse_size(&text)
            .expect("OPENCONV_CHANNEL_CAP must be a size such as 64MiB or 512KiB");
    }
    if let Ok(text) = std::env::var("OPENCONV_RELAY_CAP") {
        settings.retention.relay_cap = config::parse_size(&text)
            .expect("OPENCONV_RELAY_CAP must be a size such as 4GiB or 512MiB");
    }
    if let Ok(text) = std::env::var("OPENCONV_MAX_FRAME") {
        settings.max_frame = config::parse_size(&text)
            .and_then(|n| usize::try_from(n).ok())
            .expect("OPENCONV_MAX_FRAME must be a size such as 1MiB or 256KiB");
    }
    tracing::info!(
        window_secs = settings.retention.window.as_secs(),
        channel_cap = settings.retention.channel_cap,
        relay_cap = settings.retention.relay_cap,
        max_frame = settings.max_frame,
        "limits"
    );

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!(%addr, "relay listening");
    axum::serve(
        listener,
        openconv_server::router(store, settings).expect("start the relay writer"),
    )
    .await
    .expect("serve");
}
