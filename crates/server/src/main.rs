//! Binary entry point for the OpenConv relay.

use openconv_server::store::Store;
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
    let store = Store::open(&db).expect("open the relay store");
    tracing::info!(db, "relay store open");

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!(%addr, "relay listening");
    axum::serve(
        listener,
        openconv_server::router(store).expect("start the relay writer"),
    )
    .await
    .expect("serve");
}
