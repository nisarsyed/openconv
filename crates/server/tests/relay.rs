//! End-to-end: two MLS members hold a conversation through the real relay
//! over a real WebSocket. Nothing is stubbed but the binary's `main`.

use futures_util::{SinkExt, StreamExt};
use openconv_core::{Event, FrameKind, Member, decode_envelope, encode_frame};
use tokio_tungstenite::tungstenite::Message;

/// Next binary frame from a socket.
async fn next<S>(ws: &mut S) -> Vec<u8>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    ws.next().await.unwrap().unwrap().into_data().to_vec()
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
        .send(Message::Binary(
            encode_frame(FrameKind::KeyPackage, &bob.key_package().unwrap()).into(),
        ))
        .await
        .unwrap();

    // Alice receives it and admits him.
    // Alice receives it and stages an add. The relay echoes her own frames
    // back, so she must skip the KeyPackage echo bob's frame is not.
    let env = decode_envelope(&next(&mut alice_ws).await).unwrap();
    assert_eq!(env.kind, FrameKind::KeyPackage);
    let commit = alice.propose_add(&env.body).unwrap();
    alice_ws
        .send(Message::Binary(
            encode_frame(FrameKind::Commit, &commit).into(),
        ))
        .await
        .unwrap();

    // Her own commit comes back first, which confirms it and releases the
    // Welcome.
    let env = decode_envelope(&next(&mut alice_ws).await).unwrap();
    assert_eq!(env.kind, FrameKind::Commit);
    let Event::Admitted { welcome } = alice.receive(&env.body).unwrap() else {
        panic!("uncontested commit should have been admitted");
    };
    alice_ws
        .send(Message::Binary(
            encode_frame(FrameKind::Welcome, &welcome).into(),
        ))
        .await
        .unwrap();

    // Bob skips the echo of his own KeyPackage and Alice's commit (which is
    // not addressed to him), then joins from the Welcome.
    let welcome = loop {
        let env = decode_envelope(&next(&mut bob_ws).await).unwrap();
        if env.kind == FrameKind::Welcome {
            break env.body;
        }
    };
    bob.join(&welcome).unwrap();
    assert_eq!(bob.member_count(), 2);

    // Alice -> Bob, encrypted the whole way.
    let ct = alice.send("hello over the wire").unwrap();
    alice_ws
        .send(Message::Binary(
            encode_frame(FrameKind::Application, &ct).into(),
        ))
        .await
        .unwrap();

    let env = decode_envelope(&next(&mut bob_ws).await).unwrap();
    assert_eq!(env.kind, FrameKind::Application);
    let Event::Message(text) = bob.receive(&env.body).unwrap() else {
        panic!("expected an application message");
    };
    assert_eq!(text, "hello over the wire");

    // Bob -> Alice.
    let ct = bob.send("got it").unwrap();
    bob_ws
        .send(Message::Binary(
            encode_frame(FrameKind::Application, &ct).into(),
        ))
        .await
        .unwrap();

    // Alice's queue still holds echoes of her own Welcome and message. Only
    // application frames go to `receive`; a Welcome is not a protocol message
    // and feeding it in would be an error.
    let text = loop {
        let env = decode_envelope(&next(&mut alice_ws).await).unwrap();
        if env.kind != FrameKind::Application {
            continue;
        }
        if let Event::Message(text) = alice.receive(&env.body).unwrap() {
            break text;
        }
    };
    assert_eq!(text, "got it");
}
