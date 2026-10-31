//! End-to-end: clients talk through the real relay over real WebSocket connections.
//! Nothing is stubbed but the binary's `main`.

use futures_util::{SinkExt, StreamExt};
use openconv_core::{
    Envelope, Event, FrameKind, Member, decode_delivery, encode_publish, encode_subscribe,
};
use openconv_server::store::Store;
use openconv_wire::Address;
use std::time::Duration;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Boot a relay with an in-memory store on an ephemeral port and return its
/// ws:// URL.
async fn spawn_relay() -> String {
    spawn_relay_on(Store::in_memory().unwrap()).await.0
}

/// Boot a relay over `store`, returning its URL and the task serving it, so a
/// test can stop it.
async fn spawn_relay_on(store: Store) -> (String, tokio::task::JoinHandle<()>) {
    spawn_relay_with(store, openconv_server::Config::default()).await
}

/// Boot a relay over `store` with its own configuration.
async fn spawn_relay_with(
    store: Store,
    config: openconv_server::Config,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, openconv_server::router(store, config).unwrap())
            .await
            .unwrap();
    });
    (format!("ws://{addr}/ws"), server)
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

/// A connection needs a channel and a mailbox; one asking for far more is
/// broken or hostile, and is closed rather than served. Re-subscribing to an
/// address already held does not count against the limit.
#[tokio::test]
async fn too_many_subscriptions_close_the_connection() {
    let url = spawn_relay().await;

    let mut modest = connect(&url).await;
    let room: &[u8] = b"room";
    for _ in 0..100 {
        send(&mut modest, encode_subscribe(&[room.to_vec()]).unwrap()).await;
    }
    publish(&mut modest, b"room", FrameKind::Application, b"still here").await;
    assert_eq!(next(&mut modest).await.body, b"still here");

    let mut greedy = connect(&url).await;
    let many: Vec<Vec<u8>> = (0..65u8).map(|i| vec![i + 1]).collect();
    send(&mut greedy, encode_subscribe(&many).unwrap()).await;
    let closed = tokio::time::timeout(Duration::from_secs(5), greedy.next())
        .await
        .expect("relay kept a client over its subscription limit");
    assert!(
        matches!(closed, None | Some(Ok(Message::Close(_)) | Err(_))),
        "expected the connection to end, got {closed:?}"
    );
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

/// The done condition for durable storage: a restarted relay carries on
/// numbering where it stopped, and what was stored before is still there.
///
/// Reusing a sequence number after a restart is corruption, not a reset: a
/// client whose cursor was past it would silently skip the new frames.
#[tokio::test]
async fn a_restarted_relay_continues_the_sequence_and_keeps_frames() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("relay.db");

    let (url, server) = spawn_relay_on(Store::open(&db).unwrap()).await;
    let mut ws = connect(&url).await;
    subscribe(&mut ws, &[b"room"]).await;
    for body in [&b"m0"[..], b"m1", b"m2"] {
        publish(&mut ws, b"room", FrameKind::Application, body).await;
        assert_eq!(next(&mut ws).await.body, body);
    }
    ws.close(None).await.unwrap();
    server.abort();

    // A new relay over the same file, as after a process restart. Two
    // subscribers, so this checks the number everyone agrees on, not one
    // connection's view.
    let (url, _server) = spawn_relay_on(Store::open(&db).unwrap()).await;
    let mut a = connect(&url).await;
    let mut b = connect(&url).await;
    subscribe(&mut a, &[b"room"]).await;
    subscribe(&mut b, &[b"room"]).await;
    publish(&mut a, b"room", FrameKind::Application, b"m3").await;
    for ws in [&mut a, &mut b] {
        let env = next(ws).await;
        assert_eq!((env.seq, env.body.as_slice()), (3, &b"m3"[..]));
    }

    // Everything from before the restart is still stored, in order, with the
    // number it was delivered under.
    let room = Address::new(b"room".to_vec()).unwrap();
    let stored: Vec<(u64, Vec<u8>)> = Store::open(&db)
        .unwrap()
        .since(&room, 0, 100)
        .unwrap()
        .into_iter()
        .map(|d| (d.seq, decode_delivery(&d.encode()).unwrap().body))
        .collect();
    assert_eq!(
        stored,
        [
            (0, b"m0".to_vec()),
            (1, b"m1".to_vec()),
            (2, b"m2".to_vec()),
            (3, b"m3".to_vec())
        ]
    );
}

/// A frame to an address nobody is listening on is kept, not dropped. This
/// is what lets a `Welcome` reach a joiner who went offline after asking.
#[tokio::test]
async fn frames_to_an_unwatched_address_are_stored() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("relay.db");
    let (url, _server) = spawn_relay_on(Store::open(&db).unwrap()).await;

    let mut sender = connect(&url).await;
    publish(&mut sender, b"mailbox", FrameKind::Welcome, b"welcome").await;
    // Sync point: the relay handles a connection's requests in order, so by
    // the time this probe echoes, the publish before it has been stored.
    subscribe(&mut sender, &[]).await;

    let mailbox = Address::new(b"mailbox".to_vec()).unwrap();
    let stored = Store::open(&db).unwrap().since(&mailbox, 0, 10).unwrap();
    assert_eq!(stored.len(), 1);
    let env = decode_delivery(&stored[0].encode()).unwrap();
    assert_eq!(
        (env.kind, env.body.as_slice()),
        (FrameKind::Welcome, &b"welcome"[..])
    );
}

/// The done condition's hard part: a client catching up while frames keep
/// arriving ends with every frame exactly once, in order. Catch-up and live
/// delivery must meet without a gap or an overlap.
#[tokio::test]
async fn catch_up_and_live_traffic_meet_without_a_gap() {
    use openconv_wire::{Inbound, Request};
    const STORED: u64 = 400;
    const LIVE: u64 = 300;

    let url = spawn_relay().await;
    let room = Address::new(b"room".to_vec()).unwrap();

    // Traffic while the client is away: stored, not delivered to anyone.
    let mut sender = connect(&url).await;
    for i in 0..STORED {
        publish(
            &mut sender,
            b"room",
            FrameKind::Application,
            &i.to_be_bytes(),
        )
        .await;
    }
    subscribe(&mut sender, &[]).await; // sync: everything above is stored

    // The client comes back and catches up while the sender keeps going.
    let mut client = connect(&url).await;
    let resume = |from| {
        Request::Resume {
            address: room.clone(),
            from,
        }
        .encode()
        .unwrap()
    };
    send(&mut client, resume(0)).await;
    let flood = tokio::spawn(async move {
        for i in STORED..STORED + LIVE {
            publish(
                &mut sender,
                b"room",
                FrameKind::Application,
                &i.to_be_bytes(),
            )
            .await;
        }
        sender
    });

    // Done once every frame is in and the relay has said the client is
    // live. The two can come in either order: if the flood finishes before
    // the last page is read, that page holds the rest and ends with `live`.
    let mut seen = Vec::new();
    let mut live = false;
    while (seen.len() as u64) < STORED + LIVE || !live {
        let msg = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("stalled during catch-up")
            .unwrap()
            .unwrap();
        match Inbound::decode(&msg.into_data()).unwrap() {
            Inbound::Delivery(d) => seen.push(d.seq),
            Inbound::Page {
                live: false, next, ..
            } => send(&mut client, resume(next)).await,
            Inbound::Page { live: true, .. } => live = true,
            other => panic!("unexpected {other:?}"),
        }
    }
    let _sender = flood.await.unwrap();
    assert_eq!(
        seen,
        (0..STORED + LIVE).collect::<Vec<_>>(),
        "gap or duplicate"
    );
    assert!(
        stays_quiet(&mut client).await,
        "frames beyond what was published"
    );
}

// ---- Catch-up with real members (#105) ----

use openconv_core::{Effect, Vault};

/// A member and its connection, applying what the relay sends the way the
/// app does: every effect that says to send something is sent.
struct Peer {
    member: Member,
    ws: Ws,
    /// Messages from others, in the order this member decrypted them.
    heard: Vec<String>,
    /// Set once this member is no longer in the group.
    removed: bool,
    /// Set if the relay said this member's cursor is gone. Fails the test
    /// unless the test expects it.
    stranded: bool,
    expect_stranding: bool,
    /// Every request this member's effects sent, so a test can replay one.
    sent: Vec<Vec<u8>>,
}

impl Peer {
    async fn online(member: Member, url: &str) -> Self {
        let mut peer = Self {
            member,
            ws: connect(url).await,
            heard: Vec::new(),
            removed: false,
            stranded: false,
            expect_stranding: false,
            sent: Vec::new(),
        };
        for request in peer.member.connect().unwrap() {
            send(&mut peer.ws, request).await;
        }
        peer
    }

    /// Apply whatever has arrived. Returns whether anything did.
    async fn pump(&mut self) -> bool {
        let mut any = false;
        while let Ok(Some(msg)) =
            tokio::time::timeout(Duration::from_millis(150), self.ws.next()).await
        {
            any = true;
            let Message::Binary(wire) = msg.unwrap() else {
                continue;
            };
            for effect in self.member.deliver(&wire).unwrap() {
                match effect {
                    Effect::Send(request) => {
                        self.sent.push(request.clone());
                        send(&mut self.ws, request).await;
                    }
                    Effect::Message(text) => self.heard.push(text),
                    Effect::Unavailable { .. } => {
                        assert!(self.expect_stranding, "cursor unavailable");
                        self.stranded = true;
                    }
                    Effect::Removed => self.removed = true,
                    _ => {}
                }
            }
        }
        any
    }

    async fn say(&mut self, text: &str) {
        let ct = self.member.send(text).unwrap();
        let channel = self.member.channel().unwrap();
        publish(&mut self.ws, &channel, FrameKind::Application, &ct).await;
    }

    /// Remove the member called `name`.
    async fn remove(&mut self, name: &str) {
        let index = self
            .member
            .members()
            .into_iter()
            .find(|m| m.name == name)
            .unwrap()
            .index;
        let commit = self.member.remove_member(index).unwrap();
        let channel = self.member.channel().unwrap();
        publish(&mut self.ws, &channel, FrameKind::Commit, &commit).await;
    }

    /// Ask to leave.
    async fn leave(&mut self) {
        let proposal = self.member.leave().unwrap();
        let channel = self.member.channel().unwrap();
        publish(&mut self.ws, &channel, FrameKind::Proposal, &proposal).await;
    }

    fn names(&self) -> Vec<String> {
        self.member.members().into_iter().map(|m| m.name).collect()
    }

    /// Close the connection and the member, keeping only what is on disk.
    async fn offline(mut self) {
        self.ws.close(None).await.ok();
    }
}

/// Everything said in the group, as a member who said some of it saw it:
/// what alice heard, with bob's own lines placed where alice heard them.
/// Alice heard everyone but herself, so this is checked against alice too.
fn transcript(alice: &Peer, bob: &Peer) -> Vec<String> {
    // Alice spoke "before" and "one"; she heard "two" and "three" from bob.
    let mut all = vec!["before".to_owned(), "one".to_owned()];
    all.extend(alice.heard.iter().cloned());
    assert_eq!(bob.heard, ["before", "one"], "bob heard alice's lines");
    all
}

/// Pump everyone until a whole round passes with nothing arriving.
async fn settle(peers: &mut [&mut Peer]) {
    for _ in 0..60 {
        let mut any = false;
        for peer in peers.iter_mut() {
            any |= peer.pump().await;
        }
        if !any {
            return;
        }
    }
    panic!("never settled");
}

fn persisted(dir: &std::path::Path, name: &str) -> Member {
    Member::restore(
        Vault::with_key(dir.join(format!("{name}.db")), [5u8; 32]),
        name,
    )
    .unwrap()
}

/// The done condition for #105: a member closes, misses traffic from two
/// others (messages and a commit admitting someone new), reopens, and ends
/// up with the same group state and message set as everyone else.
#[tokio::test]
async fn a_member_who_was_away_catches_up_to_the_same_state() {
    let url = spawn_relay().await;
    let dir = tempfile::tempdir().unwrap();

    let mut alice = Member::new("alice").unwrap();
    alice.create_group().unwrap();
    let channel = alice.channel().unwrap();
    let mut alice = Peer::online(alice, &url).await;

    let mut bob = Member::new("bob").unwrap();
    bob.offer(&channel).unwrap();
    let mut bob = Peer::online(bob, &url).await;
    let mut carol = persisted(dir.path(), "carol");
    carol.offer(&channel).unwrap();
    let mut carol = Peer::online(carol, &url).await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    assert_eq!(carol.member.member_count(), 3, "carol was not admitted");

    alice.say("before").await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    let mut carol_heard = carol.heard.clone();
    carol.offline().await;

    // While carol is away: messages from two members, and a commit.
    alice.say("one").await;
    bob.say("two").await;
    let mut dave = Member::new("dave").unwrap();
    dave.offer(&channel).unwrap();
    let mut dave = Peer::online(dave, &url).await;
    settle(&mut [&mut alice, &mut bob, &mut dave]).await;
    assert_eq!(dave.member.member_count(), 4, "dave was not admitted");
    bob.say("three, after dave").await;
    settle(&mut [&mut alice, &mut bob, &mut dave]).await;

    let mut carol = Peer::online(persisted(dir.path(), "carol"), &url).await;
    settle(&mut [&mut alice, &mut bob, &mut carol, &mut dave]).await;

    assert_eq!(
        carol.member.member_count(),
        4,
        "carol missed dave's admission"
    );
    carol_heard.extend(carol.heard.iter().cloned());
    assert_eq!(
        carol_heard,
        ["before", "one", "two", "three, after dave"],
        "carol's messages differ from what was said"
    );
    assert_eq!(
        carol_heard,
        transcript(&alice, &bob),
        "carol and the others disagree"
    );
    // Same epoch as everyone: they can all read her.
    carol.say("back").await;
    settle(&mut [&mut alice, &mut bob, &mut carol, &mut dave]).await;
    for peer in [&alice, &bob, &dave] {
        assert_eq!(peer.heard.last().map(String::as_str), Some("back"));
    }
}

/// A join request made while every member is offline is answered when one
/// comes back, and a joiner who was offline too joins from the Welcome kept
/// in its mailbox. Re-offering after being admitted adds nobody twice.
#[tokio::test]
async fn a_join_request_made_while_everyone_was_away_is_answered() {
    let url = spawn_relay().await;
    let dir = tempfile::tempdir().unwrap();

    let mut alice = persisted(dir.path(), "alice");
    alice.create_group().unwrap();
    let channel = alice.channel().unwrap();
    let mut alice = Peer::online(alice, &url).await;
    settle(&mut [&mut alice]).await;
    alice.offline().await;

    // Nobody is online to answer. Bob asks, then goes away too.
    let mut bob = persisted(dir.path(), "bob");
    bob.offer(&channel).unwrap();
    let mut bob = Peer::online(bob, &url).await;
    settle(&mut [&mut bob]).await;
    assert!(bob.member.channel().is_none());
    bob.offline().await;

    // Alice returns, catches up, and answers the held request.
    let mut alice = Peer::online(persisted(dir.path(), "alice"), &url).await;
    settle(&mut [&mut alice]).await;
    assert_eq!(
        alice.member.member_count(),
        2,
        "the held request was not answered"
    );

    // Bob returns: re-offers, and finds his Welcome waiting.
    let mut bob = Peer::online(persisted(dir.path(), "bob"), &url).await;
    settle(&mut [&mut alice, &mut bob]).await;
    assert_eq!(bob.member.channel(), Some(channel));
    assert_eq!(
        alice.member.member_count(),
        2,
        "bob's re-offer added him a second time"
    );
    bob.say("made it").await;
    settle(&mut [&mut alice, &mut bob]).await;
    assert_eq!(alice.heard, ["made it"]);
}

/// A relay whose store is replaced by an empty one (started from another
/// directory, say) is behind every client. They follow its new numbering
/// from the start and carry on, rather than stopping as though they had
/// missed frames, and new members can still join through it.
#[tokio::test]
async fn members_carry_on_through_a_relay_whose_store_was_reset() {
    let url = spawn_relay().await;
    let dir = tempfile::tempdir().unwrap();

    let mut alice = persisted(dir.path(), "alice");
    alice.create_group().unwrap();
    let channel = alice.channel().unwrap();
    let mut alice = Peer::online(alice, &url).await;
    let mut bob = persisted(dir.path(), "bob");
    bob.offer(&channel).unwrap();
    let mut bob = Peer::online(bob, &url).await;
    let mut carol = persisted(dir.path(), "carol");
    carol.offer(&channel).unwrap();
    let mut carol = Peer::online(carol, &url).await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    for i in 0..5 {
        alice.say(&format!("old {i}")).await;
    }
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    for peer in [alice, bob, carol] {
        peer.offline().await;
    }

    // A different relay, with nothing stored.
    let url = spawn_relay().await;
    let mut alice = Peer::online(persisted(dir.path(), "alice"), &url).await;
    let mut bob = Peer::online(persisted(dir.path(), "bob"), &url).await;
    let mut carol = Peer::online(persisted(dir.path(), "carol"), &url).await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;

    bob.say("still here").await;
    let mut dave = Member::new("dave").unwrap();
    dave.offer(&channel).unwrap();
    let mut dave = Peer::online(dave, &url).await;
    settle(&mut [&mut alice, &mut bob, &mut carol, &mut dave]).await;
    assert_eq!(alice.heard, ["still here"]);
    assert_eq!(carol.heard, ["still here"]);
    assert_eq!(
        dave.member.member_count(),
        4,
        "nobody could join after the reset"
    );
    dave.say("joined after the reset").await;
    settle(&mut [&mut alice, &mut bob, &mut carol, &mut dave]).await;
    for peer in [&alice, &bob, &carol] {
        assert_eq!(
            peer.heard.last().map(String::as_str),
            Some("joined after the reset")
        );
    }
}

/// The done condition for #106, from a member's side: away longer than the
/// relay keeps frames, a member is told it cannot catch up, rather than being
/// served whatever is left as though nothing were missing.
#[tokio::test]
async fn a_member_away_past_retention_is_told_so() {
    let config = openconv_server::Config {
        retention: openconv_server::Retention {
            window: Duration::from_millis(300),
            ..openconv_server::Retention::default()
        },
        ..openconv_server::Config::default()
    };
    let (url, _server) = spawn_relay_with(Store::in_memory().unwrap(), config).await;
    let dir = tempfile::tempdir().unwrap();

    let mut alice = Member::new("alice").unwrap();
    alice.create_group().unwrap();
    let channel = alice.channel().unwrap();
    let mut alice = Peer::online(alice, &url).await;
    let mut bob = persisted(dir.path(), "bob");
    bob.offer(&channel).unwrap();
    let mut bob = Peer::online(bob, &url).await;
    settle(&mut [&mut alice, &mut bob]).await;
    bob.offline().await;

    alice.say("said while bob was away").await;
    settle(&mut [&mut alice]).await;
    tokio::time::sleep(Duration::from_millis(800)).await; // past the window

    let mut bob = persisted(dir.path(), "bob");
    let mut ws = connect(&url).await;
    for request in bob.connect().unwrap() {
        send(&mut ws, request).await;
    }
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("no answer to the resume")
        .unwrap()
        .unwrap();
    let effects = bob.deliver(&msg.into_data()).unwrap();
    // Told the frames are gone, and (since #112) asking to be re-added.
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Unavailable { .. }, Effect::Rejoining, ..]
        ),
        "expected to be told the frames are gone, got {effects:?}"
    );
}

/// A message over the size limit closes the connection that sent it, and
/// nothing else. Without the limit, one frame could fill a channel's cap and
/// push out all of its history.
#[tokio::test]
async fn an_oversized_message_closes_only_that_connection() {
    let config = openconv_server::Config {
        max_frame: 1024,
        ..openconv_server::Config::default()
    };
    let (url, _server) = spawn_relay_with(Store::in_memory().unwrap(), config).await;
    let mut member = connect(&url).await;
    subscribe(&mut member, &[b"room"]).await;

    let mut flooder = connect(&url).await;
    publish(&mut flooder, b"room", FrameKind::Application, &[0; 2000]).await;
    let closed = tokio::time::timeout(Duration::from_secs(5), flooder.next())
        .await
        .expect("the relay kept an oversized sender connected");
    assert!(
        matches!(closed, None | Some(Ok(Message::Close(_)) | Err(_))),
        "expected the connection to end, got {closed:?}"
    );
    assert!(
        stays_quiet(&mut member).await,
        "the oversized frame was relayed"
    );

    publish(&mut member, b"room", FrameKind::Application, &[0; 900]).await;
    assert_eq!(
        next(&mut member).await.body.len(),
        900,
        "a frame under the limit failed"
    );
}

// ---- Membership (#112) ----

/// Alice, bob and carol in one group, all online.
async fn three(url: &str) -> (Peer, Peer, Peer, Vec<u8>) {
    let mut alice = Member::new("alice").unwrap();
    alice.create_group().unwrap();
    let channel = alice.channel().unwrap();
    let mut alice = Peer::online(alice, url).await;
    let mut bob = Member::new("bob").unwrap();
    bob.offer(&channel).unwrap();
    let mut bob = Peer::online(bob, url).await;
    let mut carol = Member::new("carol").unwrap();
    carol.offer(&channel).unwrap();
    let mut carol = Peer::online(carol, url).await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    assert_eq!(carol.member.member_count(), 3);
    (alice, bob, carol, channel)
}

/// A member removed through the relay learns it, keeps nothing of the group,
/// and the others carry on without it.
#[tokio::test]
async fn a_member_can_be_removed() {
    let url = spawn_relay().await;
    let (mut alice, mut bob, mut carol, _) = Box::pin(three(&url)).await;

    alice.remove("carol").await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    assert!(carol.removed, "carol was not told");
    assert_eq!(carol.member.channel(), None, "carol kept the group");
    assert_eq!(alice.names(), ["alice", "bob"]);
    assert_eq!(bob.names(), alice.names());

    bob.say("without carol").await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    assert_eq!(alice.heard, ["without carol"]);
    assert!(carol.heard.is_empty());
}

/// A member asks to leave, and whoever is online commits it.
#[tokio::test]
async fn a_member_can_leave() {
    let url = spawn_relay().await;
    let (mut alice, mut bob, mut carol, _) = Box::pin(three(&url)).await;

    carol.leave().await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    assert!(carol.removed, "carol's leave was never committed");
    assert_eq!(alice.names(), ["alice", "bob"]);
    assert_eq!(bob.names(), alice.names());
}

/// A request to leave is bound to its epoch. If another commit lands
/// first, the leaver makes it again in the new epoch, and still gets out.
#[tokio::test]
async fn a_leave_that_lapses_is_made_again() {
    let url = spawn_relay().await;
    let (mut alice, mut bob, mut carol, channel) = Box::pin(three(&url)).await;

    // Carol decides to leave, but before her request reaches anyone, dave
    // is admitted: a new epoch, in which her request means nothing.
    let _lapsed = carol.member.leave().unwrap();
    let mut dave = Member::new("dave").unwrap();
    dave.offer(&channel).unwrap();
    let mut dave = Peer::online(dave, &url).await;
    settle(&mut [&mut alice, &mut bob, &mut carol, &mut dave]).await;

    assert!(carol.removed, "carol's lapsed request was not made again");
    assert_eq!(alice.names(), ["alice", "bob", "dave"]);
    assert_eq!(dave.names(), alice.names());
}

/// The done condition's hardest case: a member stranded by retention asks
/// to be re-added, is, and ends up with the same group as everyone else,
/// in its old place rather than a second one.
#[tokio::test]
async fn a_stranded_member_is_readded() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("relay.db");
    let (url, _server) = spawn_relay_on(Store::open(&db).unwrap()).await;

    let mut alice = Member::new("alice").unwrap();
    alice.create_group().unwrap();
    let channel = alice.channel().unwrap();
    let mut alice = Peer::online(alice, &url).await;
    let mut bob = Member::new("bob").unwrap();
    bob.offer(&channel).unwrap();
    let mut bob = Peer::online(bob, &url).await;
    let mut carol = persisted(dir.path(), "carol");
    carol.offer(&channel).unwrap();
    let mut carol = Peer::online(carol, &url).await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;
    carol.offline().await;

    // While carol is away the group changes (dave joins), and then retention
    // takes everything, that commit included. Done through a second
    // connection to the store, so it happens exactly once: a tiny window
    // would strand carol again and again as she recovered.
    let mut dave = Member::new("dave").unwrap();
    dave.offer(&channel).unwrap();
    let mut dave = Peer::online(dave, &url).await;
    settle(&mut [&mut alice, &mut bob, &mut dave]).await;
    Store::open(&db).unwrap().expire(i64::MAX as u64).unwrap();

    let mut carol = Peer::online(persisted(dir.path(), "carol"), &url).await;
    carol.expect_stranding = true;
    settle(&mut [&mut alice, &mut bob, &mut carol, &mut dave]).await;

    assert!(
        carol.stranded,
        "carol was not stranded; the test proves nothing"
    );
    assert_eq!(
        carol.member.channel(),
        Some(channel),
        "carol was not re-added"
    );
    assert_eq!(
        alice.names(),
        ["alice", "bob", "carol", "dave"],
        "carol has two places"
    );
    assert_eq!(carol.names(), alice.names());
    carol.say("back in").await;
    alice.say("welcome back").await;
    settle(&mut [&mut alice, &mut bob, &mut carol, &mut dave]).await;
    assert!(bob.heard.contains(&"back in".to_owned()));
    assert!(carol.heard.contains(&"welcome back".to_owned()));

    // A late copy of carol's request, as a reconnect before her Welcome
    // would send. It was already acted on, so nobody may act on it again:
    // that would replace her place a second time and push her back out.
    let rejoin = carol
        .sent
        .iter()
        .find(|r| matches!(openconv_wire::Request::decode(r), Ok(openconv_wire::Request::Publish { payload, .. }) if payload.first() == Some(&6)))
        .expect("carol never sent a re-add request")
        .clone();
    send(&mut carol.ws, rejoin).await;
    settle(&mut [&mut alice, &mut bob, &mut carol, &mut dave]).await;
    assert!(!carol.removed, "a stale re-add request pushed carol out");
    assert_eq!(alice.names(), ["alice", "bob", "carol", "dave"]);
}

/// Re-adding replaces a member's place, so only that member may ask for
/// it. A stranger cannot: a request from an identity not in the group is
/// ignored, and nobody is removed or added for it.
#[tokio::test]
async fn a_rejoin_from_outside_the_group_is_ignored() {
    let url = spawn_relay().await;
    let (mut alice, mut bob, mut carol, channel) = Box::pin(three(&url)).await;

    let mut stranger = Member::new("carol").unwrap(); // claims carol's name
    let offer = stranger.key_package().unwrap();
    let mut ws = connect(&url).await;
    publish(&mut ws, &channel, FrameKind::Rejoin, &offer).await;
    settle(&mut [&mut alice, &mut bob, &mut carol]).await;

    assert_eq!(alice.names(), ["alice", "bob", "carol"]);
    assert!(!carol.removed, "the stranger's request replaced carol");
    let _ = &mut stranger;
}
