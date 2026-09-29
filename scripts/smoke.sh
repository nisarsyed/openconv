#!/usr/bin/env bash
# End-to-end check against the real app: boots the relay, launches three
# clients in one channel and a fourth in another, and asserts messages travel
# within a channel and never between them.
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
# Dave hosts a channel of his own. Nobody in alice's should ever hear him.
OTHER_MESSAGE="hello from dave"

dump_logs() {
    for who in alice bob carol dave relay; do
        if [ -s "$LOG/$who.log" ]; then
            echo "--- $who ---"; cat "$LOG/$who.log"
        elif [ -f "$LOG/$who.log" ]; then
            echo "--- $who --- (empty: process produced no output)"
        fi
    done
    # A client that dies on exec or traps at startup leaves a crash report.
    local reports=~/Library/Logs/DiagnosticReports
    if [ -d "$reports" ]; then
        for r in "$reports"/OpenConv*.ips; do
            [ -e "$r" ] || continue
            echo "--- crash report $(basename "$r") ---"
            head -40 "$r"
        done
    fi
}

cleanup() {
    kill ${ALICE:-} ${BOB:-} ${CAROL:-} ${DAVE:-} ${RELAY:-} 2>/dev/null
    wait 2>/dev/null
}
trap cleanup EXIT

# Any exit before the assertions below is a failure worth explaining.
die() {
    echo "$1"
    echo
    dump_logs
    exit 1
}

# Wait for a condition rather than guessing at a duration. Fixed sleeps are
# both slower than necessary here and unreliable on a loaded CI runner, where
# the safe sleep and the fast one are far apart.
#   wait_for <seconds> <description> <command...>
wait_for() {
    local timeout=$1 what=$2; shift 2
    local deadline=$(( SECONDS + timeout ))
    until "$@" >/dev/null 2>&1; do
        if (( SECONDS >= deadline )); then
            echo "FAIL: timed out after ${timeout}s waiting for $what"
            return 1
        fi
        sleep 0.2
    done
}

# The clients log their state transitions to stdout, which is what makes them
# observable without a human watching a window.
said() { grep -q "$2" "$LOG/$1.log" 2>/dev/null; }

echo "building..."
# Each of these is checked: a swallowed build failure surfaces later as an
# absent binary and reads like a protocol bug, which is exactly how long this
# took to diagnose the first time.
cargo build -q -p openconv-server || { echo "FAIL: relay did not build"; exit 1; }
./scripts/gen-bindings.sh >/dev/null || { echo "FAIL: binding generation failed"; exit 1; }
(cd clients/macos && swift build >/dev/null) || { echo "FAIL: client did not build"; exit 1; }

APP=clients/macos/.build/debug/OpenConv
[ -x "$APP" ] || { echo "FAIL: $APP missing after a successful build"; exit 1; }

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
wait_for 30 "the relay to accept connections" \
    curl -sf http://127.0.0.1:8080/healthz || die "relay never came up"

echo "launching clients..."
"$APP" alice host > "$LOG/alice.log" 2>&1 &
ALICE=$!
wait_for 30 "alice to create the group" said alice "created the group" \
    || die "alice never started"
# Joiners learn the channel out of band; here, from alice's log.
CHANNEL=$(sed -n 's/^\[alice\] channel \([0-9a-f]*\)$/\1/p' "$LOG/alice.log" | head -1)
[ -n "$CHANNEL" ] || die "alice never logged her channel"

# Dave starts his own channel alongside, so both carry traffic at once.
"$APP" dave host "$OTHER_MESSAGE" > "$LOG/dave.log" 2>&1 &
DAVE=$!
wait_for 30 "dave to create his group" said dave "created the group" \
    || die "dave never started"

"$APP" bob join "$CHANNEL" "$MESSAGE" > "$LOG/bob.log" 2>&1 &
BOB=$!
wait_for 30 "bob to join" said bob "joined the group" || die "bob never joined"

# Carol joins last. Both alice and bob are in the group and will race to
# admit her; the relay's ordering decides which commit wins.
"$APP" carol join "$CHANNEL" "$LATE_MESSAGE" > "$LOG/carol.log" 2>&1 &
CAROL=$!
wait_for 30 "carol to join" said carol "joined the group" || die "carol never joined"

# The assertions below are about delivery, so wait for the last message to
# arrive rather than for a fixed interval.
wait_for 30 "carol's message to reach bob" said bob "received: $LATE_MESSAGE" \
    || die "carol's message never reached bob"

echo
echo "--- alice ---"; cat "$LOG/alice.log"
echo "--- bob ---";   cat "$LOG/bob.log"
echo "--- carol ---"; cat "$LOG/carol.log"
echo "--- dave ---";  cat "$LOG/dave.log"
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

# Isolation. Checking that nobody *decrypts* dave would prove nothing: a
# relay that leaked his frames would only make the others log errors. What a
# leak really does is show dave bob's and carol's requests to join, which he
# would then admit into his own group. So: dave spoke, dave admitted nobody,
# and nobody hit an error.
check dave "sent: $OTHER_MESSAGE" "dave spoke in his own channel"
if grep -q "admitted a new member" "$LOG/dave.log"; then
    echo "  FAIL dave admitted someone from another channel"
    fail=1
else
    echo "  ok   dave saw no requests to join another channel"
fi
for who in alice bob carol dave; do
    if grep -q "error:" "$LOG/$who.log"; then
        echo "  FAIL $who logged an error"
        fail=1
    fi
done

echo
if [ "$fail" -eq 0 ]; then
    echo "PASS: messages travelled between all three, encrypted end to end"
    exit 0
fi
echo "FAIL: see above"
echo "--- relay ---"; cat "$LOG/relay.log"
exit 1
