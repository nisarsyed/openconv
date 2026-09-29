//! End-to-end: clients talk through the real relay over real WebSocket connections.
//! Nothing is stubbed but the binary's `main`.

use futures_util::{SinkExt, StreamExt};
use openconv_core::{
    Envelope, Event, FrameKind, Member, decode_delivery, encode_publish, encode_subscribe,
};
use std::time::Duration;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

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

async fn connect(url: &str) -> Ws {
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

async fn send(ws: &mut Ws, bytes: Vec<u8>) {
    ws.send(Message::Binary(bytes.into())).await.unwrap();
}

async fn publish(ws: &mut Ws, to: &[u8], kind: FrameKind, body: &[u8]) {
    send(ws, encode_publish(to, kind, body).unwrap()).await;
}

/// Subscribe, and wait until the relay has actually registered it.
///
/// A subscribe has no acknowledgement, and real clients do not need one: a
/// client that subscribes and then publishes does both on one connection,
/// which the relay handles in order. Tests coordinate *separate* connections,
/// so they need a sync point. This subscribes to a private address as well,
/// publishes a probe to it, and waits for the probe to come back. By then the
/// whole subscribe has been processed.
async fn subscribe(ws: &mut Ws, addresses: &[&[u8]]) {
    let private = format!("probe-{}", rand_suffix()).into_bytes();
    let mut all: Vec<Vec<u8>> = addresses.iter().map(|a| a.to_vec()).collect();
    all.push(private.clone());
    send(ws, encode_subscribe(&all).unwrap()).await;
    publish(ws, &private, FrameKind::Application, b"probe").await;
    loop {
        if next(ws).await.address == private {
            return;
        }
    }
}

fn rand_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Next delivery on a socket.
async fn next(ws: &mut Ws) -> Envelope {
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("timed out waiting for a frame")
        .unwrap()
        .unwrap();
    decode_delivery(&msg.into_data()).unwrap()
}

/// True if nothing arrives within a short window. Used to prove a negative,
/// so the window has to comfortably exceed a local round trip.
async fn stays_quiet(ws: &mut Ws) -> bool {
    tokio::time::timeout(Duration::from_millis(300), ws.next())
        .await
        .is_err()
}

#[tokio::test]
async fn two_members_talk_through_the_relay() {
    let url = spawn_relay().await;
    let mut alice_ws = connect(&url).await;
    let mut bob_ws = connect(&url).await;

    let mut alice = Member::new("alice").unwrap();
    let mut bob = Member::new("bob").unwrap();
    alice.create_group().unwrap();
    let channel = alice.channel().unwrap();
    subscribe(&mut alice_ws, &[&channel]).await;

    // Bob learns the channel out of band, subscribes to it and to his
    // mailbox, then publishes his KeyPackage to the channel.
    let offer = bob.key_package().unwrap();
    let mailbox = bob.mailbox(&offer).unwrap();
    subscribe(&mut bob_ws, &[&mailbox, &channel]).await;
    publish(&mut bob_ws, &channel, FrameKind::KeyPackage, &offer).await;

    // Alice receives it on the channel and stages an add.
    let env = next(&mut alice_ws).await;
    assert_eq!(
        (env.address.as_slice(), env.kind),
        (channel.as_slice(), FrameKind::KeyPackage)
    );
    let commit = alice.propose_add(&env.body).unwrap();
    publish(&mut alice_ws, &channel, FrameKind::Commit, &commit).await;

    // Her own commit comes back, which confirms it and releases the Welcome,
    // addressed to Bob's mailbox rather than the channel.
    let env = next(&mut alice_ws).await;
    assert_eq!(env.kind, FrameKind::Commit);
    let Event::Admitted {
        welcome,
        mailbox: to,
    } = alice.receive(&env.body).unwrap()
    else {
        panic!("uncontested commit should have been admitted");
    };
    assert_eq!(to, mailbox);
    publish(&mut alice_ws, &to, FrameKind::Welcome, &welcome).await;

    // Bob sees his KeyPackage echo and Alice's commit on the channel, then
    // the Welcome on his mailbox.
    let welcome = loop {
        let env = next(&mut bob_ws).await;
        if env.kind == FrameKind::Welcome {
            assert_eq!(env.address, mailbox);
            break env.body;
        }
        assert_eq!(env.address, channel);
    };
    bob.join(&welcome).unwrap();
    assert_eq!(bob.member_count(), 2);

    // Alice never subscribed to Bob's mailbox, so the Welcome did not reach
    // her. Everything else she gets is on the channel.
    let ct = bob.send("got it").unwrap();
    publish(&mut bob_ws, &channel, FrameKind::Application, &ct).await;
    let text = loop {
        let env = next(&mut alice_ws).await;
        assert_eq!(env.address, channel, "alice received off-channel traffic");
        if env.kind == FrameKind::Application {
            let Event::Message(text) = alice.receive(&env.body).unwrap() else {
                panic!("expected a message");
            };
            break text;
        }
    };
    assert_eq!(text, "got it");
}

/// The done condition for channel addressing: clients on different channels
/// cannot see each other's traffic, and each channel keeps its own order.
#[tokio::test]
async fn channels_are_isolated_and_numbered_separately() {
    let url = spawn_relay().await;
    let mut red = connect(&url).await;
    let mut blue = connect(&url).await;
    subscribe(&mut red, &[b"red"]).await;
    subscribe(&mut blue, &[b"blue"]).await;

    publish(&mut red, b"red", FrameKind::Application, b"r0").await;
    publish(&mut red, b"red", FrameKind::Application, b"r1").await;
    let (a, b) = (next(&mut red).await, next(&mut red).await);
    assert_eq!((a.seq, a.body.as_slice()), (0, &b"r0"[..]));
    assert_eq!((b.seq, b.body.as_slice()), (1, &b"r1"[..]));
    assert!(stays_quiet(&mut blue).await, "blue saw red's traffic");

    // Blue's first frame is blue's seq 0, whatever red has done.
    publish(&mut blue, b"blue", FrameKind::Application, b"b0").await;
    let c = next(&mut blue).await;
    assert_eq!((c.address.as_slice(), c.seq), (&b"blue"[..], 0));
    assert!(stays_quiet(&mut red).await, "red saw blue's traffic");
}

/// A joiner publishes to a channel it is not subscribed to. The channel's
/// subscribers get it; the publisher, not listening, gets nothing back.
#[tokio::test]
async fn publishing_does_not_require_subscribing() {
    let url = spawn_relay().await;
    let mut member = connect(&url).await;
    let mut outsider = connect(&url).await;
    subscribe(&mut member, &[b"room"]).await;

    publish(&mut outsider, b"room", FrameKind::KeyPackage, b"offer").await;
    let env = next(&mut member).await;
    assert_eq!(
        (env.kind, env.body.as_slice()),
        (FrameKind::KeyPackage, &b"offer"[..])
    );
    assert!(stays_quiet(&mut outsider).await);
}

/// Requests arrive from the network. A malformed one closes that connection
/// and leaves the relay serving everyone else.
#[tokio::test]
async fn malformed_requests_close_only_that_connection() {
    let url = spawn_relay().await;
    let mut hostile = connect(&url).await;
    let mut member = connect(&url).await;
    subscribe(&mut member, &[b"room"]).await;

    send(&mut hostile, vec![0x7f, 1, 2, 3]).await;
    let closed = tokio::time::timeout(Duration::from_secs(5), hostile.next())
        .await
        .expect("relay kept a malformed client connected");
    assert!(
        matches!(closed, None | Some(Ok(Message::Close(_)) | Err(_))),
        "expected the connection to end, got {closed:?}"
    );

    publish(&mut member, b"room", FrameKind::Application, b"still here").await;
    assert_eq!(next(&mut member).await.body, b"still here");
}

/// The relay's whole purpose beyond routing is deciding an order, and every
/// subscriber must see the same one. Numbering a frame and delivering it have
/// to happen together: done separately, two concurrent frames get numbers in
/// one order and reach subscribers in the other, and members disagree about
/// which commit won its epoch.
#[tokio::test]
async fn concurrent_senders_produce_one_agreed_order() {
    const SENDERS: usize = 4;
    const PER_SENDER: usize = 25;
    const TOTAL: usize = SENDERS * PER_SENDER;

    let url = spawn_relay().await;

    // Observers that publish nothing on the channel, so everything they see
    // there is someone else's traffic arriving in the relay's order.
    let mut observers = Vec::new();
    for _ in 0..2 {
        let mut ws = connect(&url).await;
        subscribe(&mut ws, &[b"room"]).await;
        observers.push(ws);
    }

    // Senders are not subscribed, and blast concurrently, which is what
    // makes the numbering and the delivery race.
    let mut tasks = Vec::new();
    for s in 0..SENDERS {
        let mut ws = connect(&url).await;
        tasks.push(tokio::spawn(async move {
            for i in 0..PER_SENDER {
                let body = format!("{s}:{i}").into_bytes();
                publish(&mut ws, b"room", FrameKind::Application, &body).await;
            }
            // Hold the socket open until every observer has read.
            tokio::time::sleep(Duration::from_millis(500)).await;
        }));
    }

    let mut transcripts = Vec::new();
    for ws in &mut observers {
        let mut seen = Vec::new();
        for _ in 0..TOTAL {
            let env = next(ws).await;
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
