// Runtime verification of the Rust<->Swift bridge.
//
// Deliberately a plain executable rather than a test target: XCTest and
// swift-testing both require a full Xcode install, and this needs to run with
// only the Command Line Tools present. Exits non-zero on the first failure.

import Foundation
import OpenConvCore

enum BridgeError: Error {
    case unexpected(String)
}

/// Text of a message event, or nil for anything else.
func messageText(_ event: ClientEvent) -> String? {
    if case .message(let text) = event { return text }
    return nil
}

/// Admit `joiner` to `host`'s group on `channel` the way the relay would when
/// nothing competes, returning the commit for the group's other members.
@discardableResult
func admit(_ joiner: Client, to channel: Data, by host: Client) throws -> Data {
    let commit = try host.proposeAdd(channel: channel, keyPackage: joiner.keyPackage())
    guard case .admitted(let welcome, _) = try host.receive(channel: channel, wire: commit) else {
        throw BridgeError.unexpected("uncontested commit should have been admitted")
    }
    guard try joiner.join(welcome: welcome) == channel else {
        throw BridgeError.unexpected("joined a different group")
    }
    return commit
}

/// The frame body inside a publish request: past its tag, its address and
/// the frame's kind (`Publish 0x02 len:u8 address payload`, the payload
/// being a kind tag and then the body).
func publishedBody(_ request: Data) throws -> Data {
    let bytes = [UInt8](request)
    guard bytes.count > 2, bytes[0] == 0x02 else {
        throw BridgeError.unexpected("not a publish request")
    }
    let start = 2 + Int(bytes[1]) + 1
    guard bytes.count > start else { throw BridgeError.unexpected("an empty publish") }
    return Data(bytes[start...])
}

/// The messages in a page of history, leaving out notices.
func messages(_ items: [HistoryItem]) -> [TranscriptMessage] {
    items.compactMap {
        if case .message(let message) = $0 { return message }
        return nil
    }
}

/// An entry's place in the conversation.
func position(_ item: HistoryItem) -> UInt64? {
    switch item {
    case .message(let message): message.position
    case .notice(let notice): notice.position
    }
}

/// A delivery as the relay writes one (`Delivery 0x01 len:u8 address seq:u64
/// accepted:u64`, then the frame: a kind tag and its body), accepted at 7.
func delivery(to address: Data, seq: UInt64, kind: UInt8, body: Data) -> Data {
    var wire = Data([0x01, UInt8(address.count)])
    wire.append(address)
    wire.append(contentsOf: withUnsafeBytes(of: seq.bigEndian, Array.init))
    wire.append(contentsOf: [0, 0, 0, 0, 0, 0, 0, 7])
    wire.append(kind)
    wire.append(body)
    return wire
}

var failures = 0

func check(_ label: String, _ condition: @autoclosure () throws -> Bool) {
    do {
        if try condition() {
            print("  ok   \(label)")
        } else {
            print("  FAIL \(label)")
            failures += 1
        }
    } catch {
        print("  FAIL \(label) — threw \(error)")
        failures += 1
    }
}

print("bridge: two clients exchange encrypted messages")
do {
    let alice = try Client(identity: "alice")
    let bob = try Client(identity: "bob")

    let channel = try alice.createGroup()
    check("group starts with one member", alice.memberCount(channel: channel) == 1)

    // Two-phase add: stage the commit, then confirm it the way the relay
    // would when nothing competes.
    try admit(bob, to: channel, by: alice)

    check("alice sees two members", alice.memberCount(channel: channel) == 2)
    check("bob sees two members", bob.memberCount(channel: channel) == 2)

    let ciphertext = try alice.send(channel: channel, text: "hello from swift")
    check(
        "bob decrypts alice",
        messageText(try bob.receive(channel: channel, wire: ciphertext)) == "hello from swift")

    let reply = try bob.send(channel: channel, text: "hi back")
    check(
        "alice decrypts bob",
        messageText(try alice.receive(channel: channel, wire: reply)) == "hi back")

    // The relay echoes senders their own frames; they must be recognised.
    let mine = try alice.send(channel: channel, text: "talking to myself")
    check(
        "own frames are recognised as echoes",
        try alice.receive(channel: channel, wire: mine) == .echo)

    check(
        "both are on the same channel",
        try alice.channels() == [channel] && bob.channels() == [channel])

    let secret = "seahorse battery"
    let sealed = try alice.send(channel: channel, text: secret)
    check("plaintext never reaches the wire", sealed.range(of: Data(secret.utf8)) == nil)
} catch {
    print("  FAIL setup — \(error)")
    failures += 1
}

print("bridge: a message that crosses a commit is resent")
do {
    let alice = try Client(identity: "alice")
    let bob = try Client(identity: "bob")
    let carol = try Client(identity: "carol")
    let channel = try alice.createGroup()
    try admit(bob, to: channel, by: alice)

    // Alice admits Carol while Bob speaks; the commit is ordered first.
    let commit = try alice.proposeAdd(channel: channel, keyPackage: carol.keyPackage())
    let raced = try bob.send(channel: channel, text: "crossed in the post")
    guard case .admitted(let welcome, _) = try alice.receive(channel: channel, wire: commit)
    else {
        throw BridgeError.unexpected("alice's commit was ordered first")
    }
    _ = try carol.join(welcome: welcome)
    _ = try bob.receive(channel: channel, wire: commit)

    check(
        "receivers report the stale copy",
        try alice.receive(channel: channel, wire: raced) == .stale)
    guard case .resend(let again) = try bob.receive(channel: channel, wire: raced) else {
        throw BridgeError.unexpected("bob's stale message should come back as a resend")
    }
    check(
        "the resend reaches the new member",
        messageText(try carol.receive(channel: channel, wire: again)) == "crossed in the post")
} catch {
    print("  FAIL setup — \(error)")
    failures += 1
}

print("bridge: addressing")
do {
    let host = try Client(identity: "host")
    let joiner = try Client(identity: "joiner")
    let channel = try host.createGroup()

    // The Welcome must be addressed to the mailbox the joiner subscribed to.
    let offer = try joiner.keyPackage()
    let mailbox = try joiner.mailbox(keyPackage: offer)
    let commit = try host.proposeAdd(channel: channel, keyPackage: offer)
    guard case .admitted(_, let to) = try host.receive(channel: channel, wire: commit) else {
        throw BridgeError.unexpected("uncontested commit should have been admitted")
    }
    check("welcome is addressed to the joiner's mailbox", to == mailbox)

    let text = formatChannelId(channel: channel)
    check("channel ids round trip as text", try parseChannelId(text: text) == channel)
    do {
        _ = try parseChannelId(text: "not a channel")
        print("  FAIL a bad channel id should throw")
        failures += 1
    } catch {
        print("  ok   a bad channel id throws")
    }
} catch {
    print("  FAIL addressing — \(error)")
    failures += 1
}

print("bridge: framing")
do {
    let body = Data([9, 8, 7])
    let address = Data([0xAB, 0xCD])
    // A delivery as the relay writes one: tag, address, seq, accepted time,
    // then the frame. Built by hand so this checks the format, not just that
    // one function inverts another.
    var wire = Data([0x01, UInt8(address.count)])
    wire.append(address)
    wire.append(contentsOf: [0, 0, 0, 0, 0, 0, 0, 42])
    wire.append(contentsOf: [0, 0, 0, 0, 0, 0, 0, 7])
    wire.append(0x02)  // FrameKind.welcome
    wire.append(body)

    let decoded = try decodeDelivery(wire: wire)
    check("address round trips", decoded.address == address)
    check("sequence round trips", decoded.seq == 42)
    check("accepted time round trips", decoded.acceptedMs == 7)
    check("kind round trips", decoded.kind == .welcome)
    check("body round trips", decoded.body == body)

    let request = try encodePublish(to: address, kind: .welcome, body: body)
    check("publish carries its address", request.prefix(4) == Data([0x02, 2, 0xAB, 0xCD]))
} catch {
    print("  FAIL framing — \(error)")
    failures += 1
}

print("bridge: errors cross as swift errors")
// Catch-up crosses the bridge: requests out, notices in, effects back.
do {
    let host = try Client(identity: "host")
    let channel = try host.createGroup()
    let requests = try host.connect()
    check("a member resumes its channel on connect", requests.map { $0.first } == [0x03])

    let joiner = try Client(identity: "joiner")
    try joiner.offer(channel: channel)
    check("an offer leaves the joiner waiting", try joiner.waitingToJoin() == [channel])
    check(
        "a waiting joiner resumes its mailbox, then re-offers",
        try joiner.connect().map { $0.first } == [0x03, 0x02])

    // Page notice: tag, address, next, live.
    var page = Data([0x02, UInt8(channel.count)])
    page.append(channel)
    page.append(contentsOf: [0, 0, 0, 0, 0, 0, 0, 0, 1])
    guard case .live(let address) = try host.deliver(wire: page).first else {
        throw BridgeError.unexpected("no live effect")
    }
    check("going live crosses as an effect", address == channel)

    var gone = Data([0x03, UInt8(channel.count)])
    gone.append(channel)
    gone.append(contentsOf: [0, 0, 0, 0, 0, 0, 0, 9] + [0, 0, 0, 0, 0, 0, 0, 12])
    guard case .unavailable(_, let floor, let next) = try host.deliver(wire: gone).first else {
        throw BridgeError.unexpected("no unavailable effect")
    }
    check("an unavailable cursor crosses as an effect", floor == 9 && next == 12)
} catch {
    print("  FAIL catch-up — \(error)")
    failures += 1
}

// Membership crosses the bridge: the roster, and requests to remove and leave.
do {
    let alice = try Client(identity: "alice")
    let bob = try Client(identity: "bob")
    let channel = try alice.createGroup()
    try admit(bob, to: channel, by: alice)
    let roster = bob.members(channel: channel).map { "\($0.name)\($0.me ? "*" : "")" }
    check("the roster crosses, marking the asker", roster == ["alice", "bob*"])
    guard let alicePlace = bob.members(channel: channel).first(where: { $0.name == "alice" })
    else {
        throw BridgeError.unexpected("alice missing from the roster")
    }
    check(
        "a removal is a publish request",
        try bob.removeMember(channel: channel, index: alicePlace.index).first == 0x02)
    let leaving = try Client(identity: "leaver")
    check("leaving a group it is not in throws", (try? leaving.leave(channel: channel)) == nil)
    check("a new group has no join requests", try alice.joinRequests(channel: channel).isEmpty)
    check(
        "accepting an unknown request throws",
        (try? alice.accept(channel: channel, id: Data([1, 2, 3]))) == nil)
} catch {
    print("  FAIL membership — \(error)")
    failures += 1
}

print("bridge: the stored transcript")
do {
    let alice = try Client(identity: "alice")
    let bob = try Client(identity: "bob")
    let channel = try alice.createGroup()
    try admit(bob, to: channel, by: alice)

    _ = try alice.receive(channel: channel, wire: bob.send(channel: channel, text: "from bob"))
    let mine = try alice.send(channel: channel, text: "from alice")
    let waiting = messages(try alice.history(channel: channel, before: nil, limit: 10))
    check(
        "history crosses, own message last and unplaced",
        waiting.map { $0.text } == ["from bob", "from alice"] && waiting.last?.position == nil
            && waiting.last?.mine == true)
    _ = try alice.receive(channel: channel, wire: mine)
    let placed = messages(try alice.history(channel: channel, before: nil, limit: 10))
    check("the echo places it", placed.allSatisfy { $0.position != nil })
    check("ids cross whole", placed.allSatisfy { $0.id.count == 32 })
    guard let newest = placed.last?.position else { throw BridgeError.unexpected("unplaced") }
    check(
        "paging back crosses",
        messages(try alice.history(channel: channel, before: newest, limit: 10)).map { $0.text }
            == ["from bob"])
    guard
        case .notice(let first) = try alice.history(channel: channel, before: nil, limit: 10)
            .first,
        case .joined(let who, let by?) = first.notice
    else {
        throw BridgeError.unexpected("no notice of bob joining before the messages")
    }
    check(
        "a notice crosses in place, naming who made the change",
        who.name == "bob" && !who.me && by.name == "alice" && by.me)
} catch {
    print("  FAIL transcript — \(error)")
    failures += 1
}

print("bridge: who changed what, and conversations in every state")
do {
    // Carol hosts, so she reads the channel and the relay's frames reach her
    // through deliver, effects and all.
    let carol = try Client(identity: "carol")
    let alice = try Client(identity: "alice")
    let bob = try Client(identity: "bob")
    let dave = try Client(identity: "dave")
    let channel = try carol.createGroup()
    try admit(alice, to: channel, by: carol)
    _ = try alice.receive(channel: channel, wire: admit(bob, to: channel, by: carol))

    do {
        try carol.deleteConversation(channel: channel)
        print("  FAIL deleting a conversation while a member should throw")
        failures += 1
    } catch ClientError.StillInConversation {
        print("  ok   deleting a conversation while a member throws a typed error")
    }

    // Bob lets dave in; carol hears who did it.
    let add = try bob.proposeAdd(channel: channel, keyPackage: dave.keyPackage())
    _ = try bob.receive(channel: channel, wire: add)
    let added = try carol.deliver(wire: delivery(to: channel, seq: 0, kind: 3, body: add))
    check(
        "a membership change crosses, credited to whoever made it",
        added.contains {
            guard case .membership(_, let changes) = $0,
                case .joined(let who, let by?) = changes.first
            else { return false }
            return who.name == "dave" && by.name == "bob" && !by.me
        })

    _ = try carol.deliver(
        wire: delivery(to: channel, seq: 1, kind: 4, body: bob.send(channel: channel, text: "hi")))
    let listed = try carol.conversations()
    check("a member's conversation is listed", listed.map { $0.channel } == [channel])
    check(
        "a list row carries the members and the unread count",
        listed.first?.members == ["alice", "bob", "dave"] && listed.first?.unread == 1)
    guard let read = listed.first?.last.flatMap(position) else {
        throw BridgeError.unexpected("no last entry to mark read")
    }
    try carol.markRead(channel: channel, position: read)
    check("a read marker clears unread", try carol.conversations().first?.unread == 0)
    let size = try carol.historySize(channel: channel)
    check("history size crosses", size.messages == 1 && size.notices == 3 && size.bytes > 0)

    // Bob removes carol. She hears who did it, and keeps her conversation.
    guard let carolPlace = bob.members(channel: channel).first(where: { $0.name == "carol" })
    else {
        throw BridgeError.unexpected("carol missing from the roster")
    }
    let removal = try publishedBody(bob.removeMember(channel: channel, index: carolPlace.index))
    let gone = try carol.deliver(wire: delivery(to: channel, seq: 2, kind: 3, body: removal))
    guard case .removed(_, .removed(let by?)) = gone.first else {
        throw BridgeError.unexpected("carol was not told who removed her: \(gone)")
    }
    check("a removal crosses, saying who did it", by.name == "bob")
    guard case .removed(_, let removedBy?) = try carol.conversations().first?.state else {
        throw BridgeError.unexpected("carol's conversation is not listed as removed")
    }
    check("a removed member's conversation stays listed", removedBy.name == "bob")
    try carol.deleteHistory(channel: channel)
    check(
        "history can be deleted", try carol.history(channel: channel, before: nil, limit: 9).isEmpty
    )
    try carol.deleteConversation(channel: channel)
    check("once out, a conversation can be deleted", try carol.conversations().isEmpty)

    check(
        "names are made safe to show",
        displayName(name: "Chen\u{202E}gnp.exe") == "Chen\u{FFFD}gnp.exe"
            && displayName(name: "Aisha") == "Aisha")
} catch {
    print("  FAIL conversations — \(error)")
    failures += 1
}

print("bridge: several groups, and their names")
do {
    let alice = try Client(identity: "alice")
    let bob = try Client(identity: "bob")
    let carol = try Client(identity: "carol")
    let red = try alice.createGroup()
    let blue = try carol.createGroup()
    try admit(bob, to: red, by: alice)
    try admit(bob, to: blue, by: carol)
    check("one client is in both groups", try bob.channels() == [red, blue])

    let toRed = try bob.send(channel: red, text: "for red")
    let toBlue = try bob.send(channel: blue, text: "for blue")
    check(
        "red reads its own", messageText(try alice.receive(channel: red, wire: toRed)) == "for red")
    check(
        "blue reads its own",
        messageText(try carol.receive(channel: blue, wire: toBlue)) == "for blue")
    check(
        "a frame for one group does not open in the other",
        (try? carol.receive(channel: blue, wire: toRed)) == nil)

    check("a new group has no name", try alice.name(channel: red) == nil)
    let request = try alice.setName(channel: red, name: "Red room")
    check("a rename is a publish request", request.first == 0x02)
    let commit = try publishedBody(request)
    check(
        "the rename is confirmed by its echo",
        try alice.receive(channel: red, wire: commit) == .committed)
    _ = try bob.receive(channel: red, wire: commit)
    check(
        "every member reads the name",
        try alice.name(channel: red) == "Red room" && bob.name(channel: red) == "Red room")
    check("the other group keeps none", try bob.name(channel: blue) == nil)
    check(
        "a name past the limit throws",
        (try? alice.setName(channel: red, name: String(repeating: "x", count: 129))) == nil)
} catch {
    print("  FAIL several groups — \(error)")
    failures += 1
}

do {
    let lonely = try Client(identity: "lonely")
    // No group yet, so this must throw rather than trap.
    do {
        _ = try lonely.send(channel: Data([1, 2, 3]), text: "nobody there")
        print("  FAIL sending without a group should throw")
        failures += 1
    } catch {
        print("  ok   sending without a group throws")
    }
} catch {
    print("  FAIL setup — \(error)")
    failures += 1
}

if failures > 0 {
    print("\n\(failures) check(s) failed")
    exit(1)
}
print("\nall checks passed")
