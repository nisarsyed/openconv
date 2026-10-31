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

    try alice.createGroup()
    check("group starts with one member", alice.memberCount() == 1)

    // Two-phase add: stage the commit, then confirm it the way the relay
    // would when nothing competes.
    let commit = try alice.proposeAdd(keyPackage: bob.keyPackage())
    guard case .admitted(let welcome, _) = try alice.receive(wire: commit) else {
        throw BridgeError.unexpected("uncontested commit should have been admitted")
    }
    try bob.join(welcome: welcome)

    check("alice sees two members", alice.memberCount() == 2)
    check("bob sees two members", bob.memberCount() == 2)

    let ciphertext = try alice.send(text: "hello from swift")
    check(
        "bob decrypts alice", messageText(try bob.receive(wire: ciphertext)) == "hello from swift")

    let reply = try bob.send(text: "hi back")
    check("alice decrypts bob", messageText(try alice.receive(wire: reply)) == "hi back")

    // The relay echoes senders their own frames; they must be recognised.
    let mine = try alice.send(text: "talking to myself")
    check("own frames are recognised as echoes", try alice.receive(wire: mine) == .echo)

    check(
        "both are on the same channel", alice.channel() != nil && alice.channel() == bob.channel())

    let secret = "seahorse battery"
    let sealed = try alice.send(text: secret)
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
    try alice.createGroup()
    let first = try alice.proposeAdd(keyPackage: bob.keyPackage())
    guard case .admitted(let welcome, _) = try alice.receive(wire: first) else {
        throw BridgeError.unexpected("uncontested commit should have been admitted")
    }
    try bob.join(welcome: welcome)

    // Alice admits Carol while Bob speaks; the commit is ordered first.
    let commit = try alice.proposeAdd(keyPackage: carol.keyPackage())
    let raced = try bob.send(text: "crossed in the post")
    guard case .admitted(let welcome, _) = try alice.receive(wire: commit) else {
        throw BridgeError.unexpected("alice's commit was ordered first")
    }
    try carol.join(welcome: welcome)
    _ = try bob.receive(wire: commit)

    check("receivers report the stale copy", try alice.receive(wire: raced) == .stale)
    guard case .resend(let again) = try bob.receive(wire: raced) else {
        throw BridgeError.unexpected("bob's stale message should come back as a resend")
    }
    check(
        "the resend reaches the new member",
        messageText(try carol.receive(wire: again)) == "crossed in the post")
} catch {
    print("  FAIL setup — \(error)")
    failures += 1
}

print("bridge: addressing")
do {
    let host = try Client(identity: "host")
    let joiner = try Client(identity: "joiner")
    try host.createGroup()

    // The Welcome must be addressed to the mailbox the joiner subscribed to.
    let offer = try joiner.keyPackage()
    let mailbox = try joiner.mailbox(keyPackage: offer)
    let commit = try host.proposeAdd(keyPackage: offer)
    guard case .admitted(_, let to) = try host.receive(wire: commit) else {
        throw BridgeError.unexpected("uncontested commit should have been admitted")
    }
    check("welcome is addressed to the joiner's mailbox", to == mailbox)

    guard let channel = host.channel() else {
        throw BridgeError.unexpected("a host has a channel")
    }
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
do {
    let lonely = try Client(identity: "lonely")
    // No group yet, so this must throw rather than trap.
    do {
        _ = try lonely.send(text: "nobody there")
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
