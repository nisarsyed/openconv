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

/// The relay's whole purpose beyond fan-out is deciding an order, and every
/// client must see the same one. Numbering a frame and broadcasting it have
/// to happen together: done separately, two concurrent frames get numbers in
/// one order and reach clients in the other, and members disagree about which
/// commit won its epoch.
#[tokio::test]
async fn concurrent_senders_produce_one_agreed_order() {
    const SENDERS: usize = 4;
    const PER_SENDER: usize = 25;
    const TOTAL: usize = SENDERS * PER_SENDER;

    let url = spawn_relay().await;

    // Two observers that send nothing, so everything they see is someone
    // else's traffic arriving in the relay's order.
    let mut observers = Vec::new();
    for _ in 0..2 {
        let (ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        observers.push(ws);
    }

    // Senders blast concurrently, which is what makes the numbering and the
    // broadcast race.
    let mut senders = Vec::new();
    for _ in 0..SENDERS {
        let (ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        senders.push(ws);
    }
    let mut tasks = Vec::new();
    for (s, mut ws) in senders.into_iter().enumerate() {
        tasks.push(tokio::spawn(async move {
            for i in 0..PER_SENDER {
                let body = format!("{s}:{i}").into_bytes();
                ws.send(Message::Binary(
                    encode_frame(FrameKind::Application, &body).into(),
                ))
                .await
                .unwrap();
            }
            // Hold the socket open until every observer has read.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }));
    }

    let mut transcripts = Vec::new();
    for ws in &mut observers {
        let mut seen = Vec::new();
        for _ in 0..TOTAL {
            let env = decode_envelope(&next(ws).await).unwrap();
            seen.push((env.seq, env.body));
        }
        transcripts.push(seen);
    }
    for t in tasks {
        t.await.unwrap();
    }

    for (which, seen) in transcripts.iter().enumerate() {
        let seqs: Vec<u64> = seen.iter().map(|(s, _)| *s).collect();
        let expected: Vec<u64> = (0..TOTAL as u64).collect();
        assert_eq!(
            seqs, expected,
            "observer {which} saw sequence numbers out of order or with gaps"
        );
    }

    assert_eq!(
        transcripts[0], transcripts[1],
        "observers disagreed about the order of the same frames"
    );
}
