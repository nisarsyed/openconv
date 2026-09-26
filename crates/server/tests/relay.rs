//! End-to-end: two MLS members hold a conversation through the real relay
//! over a real WebSocket. Nothing is stubbed but the binary's `main`.

use futures_util::{SinkExt, StreamExt};
use openconv_core::{FrameKind, Member, decode_frame, encode_frame};
use tokio_tungstenite::tungstenite::Message;

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
        .send(Message::Binary(encode_frame(FrameKind::KeyPackage, &bob.key_package().unwrap()).into()))
        .await
        .unwrap();

    // Alice receives it and admits him.
    let (kind, body) = decode_frame(&alice_ws.next().await.unwrap().unwrap().into_data()).unwrap();
    assert_eq!(kind, FrameKind::KeyPackage);
    let invite = alice.add_member(&body).unwrap();
    alice_ws
        .send(Message::Binary(encode_frame(FrameKind::Welcome, &invite.welcome).into()))
        .await
        .unwrap();

    // Bob joins from the Welcome.
    let (kind, body) = decode_frame(&bob_ws.next().await.unwrap().unwrap().into_data()).unwrap();
    assert_eq!(kind, FrameKind::Welcome);
    bob.join(&body).unwrap();
    assert_eq!(bob.member_count(), 2);

    // Alice -> Bob, encrypted the whole way.
    let ct = alice.send("hello over the wire").unwrap();
    alice_ws.send(Message::Binary(encode_frame(FrameKind::Application, &ct).into())).await.unwrap();

    let (kind, body) = decode_frame(&bob_ws.next().await.unwrap().unwrap().into_data()).unwrap();
    assert_eq!(kind, FrameKind::Application);
    assert_eq!(bob.receive(&body).unwrap().as_deref(), Some("hello over the wire"));

    // Bob -> Alice.
    let ct = bob.send("got it").unwrap();
    bob_ws.send(Message::Binary(encode_frame(FrameKind::Application, &ct).into())).await.unwrap();

    let (_, body) = decode_frame(&alice_ws.next().await.unwrap().unwrap().into_data()).unwrap();
    assert_eq!(alice.receive(&body).unwrap().as_deref(), Some("got it"));
}
