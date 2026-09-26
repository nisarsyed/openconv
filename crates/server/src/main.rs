//! Binary entry point for the OpenConv relay.

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

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!(%addr, "relay listening");
    axum::serve(listener, openconv_server::router()).await.expect("serve");
}
