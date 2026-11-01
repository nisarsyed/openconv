#!/usr/bin/env bash
# Retention with the real relay and app (#106): bad settings are refused, a
# member away past the window is told so, and a channel stays under its cap.
source "$(dirname "$0")/lib.sh"
need_relay; need_app; need_python
export OPENCONV_AUTO_ACCEPT=1
clear_port

echo "1. malformed settings stop the relay"
for bad in "OPENCONV_RETENTION=30" "OPENCONV_RETENTION=1.5h" "OPENCONV_CHANNEL_CAP=64MB"; do
    env "$bad" OPENCONV_RELAY_DB="$LAB/bad.db" timeout 10 "$RELAY" >"$LAB/bad.log" 2>&1
    code=$?
    { [ $code -ne 0 ] && [ $code -ne 124 ] && grep -q "must be" "$LAB/bad.log"; } ||
        { cat "$LAB/bad.log"; fail "$bad was accepted (exit $code)"; }
done
ok "30, 1.5h and 64MB are each refused at startup"

echo "2. a member away past the window is told so"
start_relay relay.log OPENCONV_RETENTION=3s
launch alice 1 alice host; A=$LAST
wait_for 30 "$LAB/alice.1.log" "created the group"
CH=$(channel_of alice "$LAB/alice.1.log")
launch bob 1 bob join "$CH"; B=$LAST
wait_for 30 "$LAB/bob.1.log" "joined the group"
sleep 1
stop $B $A
launch alice 2 alice host "said while bob was away"; A=$LAST
wait_for 30 "$LAB/alice.2.log" "caught up"
sleep 6
wait_for 10 "$LAB/relay.log" "expired frames past the retention window"
launch bob 2 bob host; B=$LAST
wait_for 30 "$LAB/bob.2.log" "the relay no longer holds what this client missed"
grep -q "received: said while bob was away" "$LAB/bob.2.log" && fail "bob was served an expired message"
ok "the relay expired the frames, and bob was told rather than shown a gap"
stop $R $A $B
sleep 0.3

echo "3. a channel stays under its size cap"
start_relay relay-cap.log OPENCONV_CHANNEL_CAP=4KiB OPENCONV_RELAY_DB="$LAB/capped.db"
"$PY" -I - <<'PYEOF' || fail "the publisher failed"
import asyncio, websockets
async def main():
    async with websockets.connect("ws://127.0.0.1:8080/ws") as ws:
        to = b"busy-channel"
        for i in range(200):  # 200 x 300 bytes = 60 KB, fifteen times the cap
            await ws.send(bytes([0x02, len(to)]) + to + bytes(300))
        probe = b"probe"
        await ws.send(bytes([0x01, 1, len(probe)]) + probe)
        await ws.send(bytes([0x02, len(probe)]) + probe + b"x")
        await asyncio.wait_for(ws.recv(), 30)  # all publishes before it are stored
asyncio.run(main())
PYEOF
read -r bytes floor next < <(sqlite3 -separator ' ' "$LAB/capped.db" "SELECT (SELECT SUM(length(payload)) FROM frames WHERE address = CAST('busy-channel' AS BLOB)), (SELECT MIN(seq) FROM frames WHERE address = CAST('busy-channel' AS BLOB)), (SELECT next_seq FROM sequences WHERE address = CAST('busy-channel' AS BLOB))")
recorded=$(sqlite3 "$LAB/capped.db" "SELECT bytes FROM sequences WHERE address = CAST('busy-channel' AS BLOB)")
echo "  info stored $bytes bytes (recorded total $recorded), seq $floor..$((next - 1)) of 0..199"
{ [ "$bytes" -le 4096 ] && [ "$floor" -gt 0 ] && [ "$next" = 200 ] && [ "$bytes" = "$recorded" ]; } ||
    fail "cap not held"
grep -q "trimmed an address over its size cap" "$LAB/relay-cap.log" || fail "no trim logged"
ok "the channel stayed under 4 KiB, oldest frames went, numbering carried on"
echo "lab passed"
