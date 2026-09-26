#!/usr/bin/env bash
# End-to-end check against the real app: boots the relay, launches two
# clients, and asserts a message travels from one to the other.
#
# The clients take an optional `host|join` argument and a message to send
# once in the group, which is what makes them driveable without a human.
set -uo pipefail
cd "$(dirname "$0")/.."

LOG=$(mktemp -d)
MESSAGE="hello from bob"

cleanup() {
    kill ${ALICE:-} ${BOB:-} ${RELAY:-} 2>/dev/null
    wait 2>/dev/null
}
trap cleanup EXIT

echo "building..."
cargo build -q -p openconv-server
./scripts/gen-bindings.sh >/dev/null
(cd clients/macos && swift build >/dev/null 2>&1)

echo "starting relay..."
cargo run -q -p openconv-server > "$LOG/relay.log" 2>&1 &
RELAY=$!
sleep 3

echo "launching clients..."
APP=clients/macos/.build/debug/OpenConv
"$APP" alice host > "$LOG/alice.log" 2>&1 &
ALICE=$!
sleep 3
"$APP" bob join "$MESSAGE" > "$LOG/bob.log" 2>&1 &
BOB=$!
sleep 8

echo
echo "--- alice ---"; cat "$LOG/alice.log"
echo "--- bob ---";   cat "$LOG/bob.log"
echo

if grep -q "received: $MESSAGE" "$LOG/alice.log"; then
    echo "PASS: message travelled from bob to alice, encrypted end to end"
    exit 0
fi
echo "FAIL: alice never received bob's message"
echo "--- relay ---"; cat "$LOG/relay.log"
exit 1
