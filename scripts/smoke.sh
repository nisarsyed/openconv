#!/usr/bin/env bash
# End-to-end check against the real app: boots the relay, launches two
# clients, and asserts a message travels from one to the other.
#
# The clients take an optional `host|join` argument and a message to send
# once in the group, which is what makes them driveable without a human.
set -uo pipefail
cd "$(dirname "$0")/.."

LOG=$(mktemp -d)
# Clients now persist state. Give each run its own data dir so a previous
# run's group does not leak into this one.
export OPENCONV_DATA_DIR="$LOG/data"
MESSAGE="hello from bob"
# Carol joins last. Bob was already in the group when she was admitted, so he
# only decrypts her message if he applied the commit that her join produced.
LATE_MESSAGE="hello from carol"

cleanup() {
    kill ${ALICE:-} ${BOB:-} ${CAROL:-} ${RELAY:-} 2>/dev/null
    wait 2>/dev/null
}
trap cleanup EXIT

echo "building..."
cargo build -q -p openconv-server
./scripts/gen-bindings.sh >/dev/null
(cd clients/macos && swift build >/dev/null 2>&1)

# A relay left behind by an earlier run would squat the port and every
# client would fail to connect, which looks like a protocol bug.
if lsof -ti:8080 >/dev/null 2>&1; then
    echo "port 8080 busy; clearing stale listeners"
    lsof -ti:8080 | xargs kill -9 2>/dev/null
    sleep 1
fi

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
sleep 5
"$APP" carol join "$LATE_MESSAGE" > "$LOG/carol.log" 2>&1 &
CAROL=$!
sleep 8

echo
echo "--- alice ---"; cat "$LOG/alice.log"
echo "--- bob ---";   cat "$LOG/bob.log"
echo "--- carol ---"; cat "$LOG/carol.log"
echo

fail=0
check() {
    if grep -q "$2" "$LOG/$1.log"; then
        echo "  ok   $3"
    else
        echo "  FAIL $3"
        fail=1
    fi
}

# Persistence: each client should have written an encrypted vault.
for who in alice bob carol; do
    vault="$OPENCONV_DATA_DIR/$who.vault"
    if [ ! -f "$vault" ]; then
        echo "  FAIL $who wrote no vault"
        fail=1
    elif ! head -c 4 "$vault" | grep -q "OCV1"; then
        echo "  FAIL $who's vault has no magic header"
        fail=1
    elif LC_ALL=C grep -qa "$who" "$vault"; then
        echo "  FAIL $who's identity is readable in its vault"
        fail=1
    else
        echo "  ok   $who persisted an encrypted vault"
    fi
done

check alice "received: $MESSAGE"      "alice decrypts bob (2 members)"
check alice "received: $LATE_MESSAGE" "alice decrypts carol (3 members)"
check bob   "received: $LATE_MESSAGE" "bob decrypts carol after applying her join commit"

echo
if [ "$fail" -eq 0 ]; then
    echo "PASS: messages travelled between all three, encrypted end to end"
    exit 0
fi
echo "FAIL: see above"
echo "--- relay ---"; cat "$LOG/relay.log"
exit 1
