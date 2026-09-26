// Runtime verification of the Rust<->Swift bridge.
//
// Deliberately a plain executable rather than a test target: XCTest and
// swift-testing both require a full Xcode install, and this needs to run with
// only the Command Line Tools present. Exits non-zero on the first failure.

import Foundation
import OpenConvCore

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

    let invite = try alice.addMember(keyPackage: bob.keyPackage())
    try bob.join(welcome: invite.welcome)

    check("alice sees two members", alice.memberCount() == 2)
    check("bob sees two members", bob.memberCount() == 2)

    let ciphertext = try alice.send(text: "hello from swift")
    check("bob decrypts alice", try bob.receive(wire: ciphertext) == "hello from swift")

    let reply = try bob.send(text: "hi back")
    check("alice decrypts bob", try alice.receive(wire: reply) == "hi back")

    let secret = "seahorse battery"
    let sealed = try alice.send(text: secret)
    check("plaintext never reaches the wire", sealed.range(of: Data(secret.utf8)) == nil)
} catch {
    print("  FAIL setup — \(error)")
    failures += 1
}

print("bridge: framing")
do {
    let body = Data([9, 8, 7])
    let decoded = try decodeFrame(wire: encodeFrame(kind: .welcome, body: body))
    check("kind round trips", decoded.kind == .welcome)
    check("body round trips", decoded.body == body)
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
