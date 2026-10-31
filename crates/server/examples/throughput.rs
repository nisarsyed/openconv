//! How many frames a second the relay stores and delivers, against how many
//! clients are publishing at once.
//!
//! Like `crates/core/examples/measure.rs`, this answers a design question
//! (does the write path scale with load, #117) rather than tracking a budget:
//! one run, no statistics, orders of magnitude.
//!
//! Each publisher has its own channel, subscribes to it, publishes a fixed
//! number of frames back to back, and counts them coming back, so a frame is
//! counted only once it has been stored and delivered. The store is a real
//! file with a real flush per commit, which is the cost being measured: on
//! macOS that is `F_FULLFSYNC`, on Linux `fsync`.
//!
//! `cargo run --release -p openconv-server --example throughput`

// Reporting numbers to a human is this binary's entire job.
#![allow(clippy::print_stdout)]

use futures_util::{SinkExt, StreamExt};
use openconv_server::store::Store;
use openconv_wire::{Address, Delivery, Request};
use std::time::Instant;
use tokio_tungstenite::tungstenite::Message;

/// Frames each publisher sends.
const PER_PUBLISHER: usize = 50;

/// A typical MLS application message.
const PAYLOAD: usize = 300;

async fn frames_per_second(publishers: usize) -> f64 {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("relay.db")).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            openconv_server::router(store, openconv_server::Retention::default()).unwrap(),
        )
        .await
        .unwrap();
    });

    // Connect and subscribe everyone before the clock starts. A connection's
    // requests are handled in order, so each subscription is in place before
    // that connection's first publish.
    let mut clients = Vec::new();
    for p in 0..publishers {
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        let channel = Address::new(format!("channel-{p}").into_bytes()).unwrap();
        let subscribe = Request::Subscribe(vec![channel.clone()]).encode().unwrap();
        ws.send(Message::Binary(subscribe.into())).await.unwrap();
        clients.push((ws, channel));
    }

    let start = Instant::now();
    let tasks: Vec<_> = clients
        .into_iter()
        .map(|(ws, channel)| {
            tokio::spawn(async move {
                let (mut tx, mut rx) = ws.split();
                let send = async {
                    for _ in 0..PER_PUBLISHER {
                        let publish = Request::Publish {
                            to: channel.clone(),
                            payload: vec![0; PAYLOAD],
                        };
                        let bytes = publish.encode().unwrap();
                        tx.send(Message::Binary(bytes.into())).await.unwrap();
                    }
                    tx
                };
                let receive = async {
                    let mut seen = 0;
                    while seen < PER_PUBLISHER {
                        if let Message::Binary(frame) = rx.next().await.unwrap().unwrap() {
                            Delivery::decode(&frame).unwrap();
                            seen += 1;
                        }
                    }
                };
                // Keep the sending half alive until every frame is back.
                let (_tx, ()) = tokio::join!(send, receive);
            })
        })
        .collect();
    for task in tasks {
        task.await.unwrap();
    }
    let elapsed = start.elapsed();
    server.abort();

    #[allow(clippy::cast_precision_loss)] // counts are tiny
    let frames = (publishers * PER_PUBLISHER) as f64;
    frames / elapsed.as_secs_f64()
}

#[tokio::main]
async fn main() {
    println!("relay throughput, {PAYLOAD}-byte frames, one channel per publisher");
    println!("  publishers   frames/s");
    for publishers in [1, 4, 16, 64] {
        let rate = frames_per_second(publishers).await;
        println!("  {publishers:>10}   {rate:>8.0}");
    }
}
