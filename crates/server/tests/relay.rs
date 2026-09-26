//! End-to-end: two MLS members hold a conversation through the real relay
//! over a real WebSocket. Nothing is stubbed but the binary's `main`.

use futures_util::{SinkExt, StreamExt};
use openconv_core::Member;
use tokio_tungstenite::tungstenite::Message;

/// Frame tags. The relay never reads these; only clients do.
const KEY_PACKAGE: u8 = 1;
const WELCOME: u8 = 2;
const APP: u8 = 3;

fn frame(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 1);
    v.push(tag);
    v.extend_from_slice(body);
    v
}

/// Boot the relay on an ephemeral port and return its ws:// URL.
async fn spawn_relay() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let app = openconv_server::router();
        axum::serve(listener, app).await.unwrap();
    });
    format!("ws://{addr}/ws")
}

#[tokio::test]
async fn two_clients_talk_through_the_relay() {
    let url = spawn_relay().await;

    let (mut alice_ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (mut bob_ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let mut alice = Member::new("alice").unwrap();
    let mut bob = Member::new("bob").unwrap();
    alice.create_group().unwrap();

    // Bob announces his KeyPackage.
    bob_ws
        .send(Message::Binary(frame(KEY_PACKAGE, &bob.key_package().unwrap()).into()))
        .await
        .unwrap();

    // Alice receives it and admits him.
    let msg = alice_ws.next().await.unwrap().unwrap().into_data();
    assert_eq!(msg[0], KEY_PACKAGE);
    let invite = alice.add_member(&msg[1..]).unwrap();
    alice_ws
        .send(Message::Binary(frame(WELCOME, &invite.welcome).into()))
        .await
        .unwrap();

    // Bob joins from the Welcome.
    let msg = bob_ws.next().await.unwrap().unwrap().into_data();
    assert_eq!(msg[0], WELCOME);
    bob.join(&msg[1..]).unwrap();
    assert_eq!(bob.member_count(), 2);

    // Alice -> Bob, encrypted the whole way.
    let ct = alice.send("hello over the wire").unwrap();
    alice_ws.send(Message::Binary(frame(APP, &ct).into())).await.unwrap();

    let msg = bob_ws.next().await.unwrap().unwrap().into_data();
    assert_eq!(msg[0], APP);
    assert_eq!(bob.receive(&msg[1..]).unwrap().as_deref(), Some("hello over the wire"));

    // Bob -> Alice.
    let ct = bob.send("got it").unwrap();
    bob_ws.send(Message::Binary(frame(APP, &ct).into())).await.unwrap();

    let msg = alice_ws.next().await.unwrap().unwrap().into_data();
    assert_eq!(alice.receive(&msg[1..]).unwrap().as_deref(), Some("got it"));
}
