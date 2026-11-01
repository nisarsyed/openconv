#!/usr/bin/env bash
# Anti-abuse limits on the real relay (#122): bad limits are refused, an
# oversized message closes only its sender, and frames sprayed across made-up
# addresses stay under the relay-wide cap. Needs no app.
source "$(dirname "$0")/lib.sh"
need_relay; need_python
clear_port

echo "1. malformed limits stop the relay"
for bad in "OPENCONV_RELAY_CAP=4GB" "OPENCONV_MAX_FRAME=1MB" "OPENCONV_MAX_FRAME=lots"; do
    env "$bad" OPENCONV_RELAY_DB="$LAB/bad.db" timeout 10 "$RELAY" >"$LAB/bad.log" 2>&1
    code=$?
    { [ $code -ne 0 ] && [ $code -ne 124 ] && grep -q "must be" "$LAB/bad.log"; } ||
        { cat "$LAB/bad.log"; fail "$bad was accepted (exit $code)"; }
done
ok "4GB, 1MB and 'lots' are each refused"

echo "2. an oversized message closes only its sender"
start_relay frame.log OPENCONV_MAX_FRAME=1KiB OPENCONV_RELAY_DB="$LAB/frame.db"
"$PY" -I - <<'PYEOF' || fail "the oversized message was not handled"
import asyncio, websockets
room = b"room"
def pub(payload): return bytes([0x02, len(room)]) + room + payload
async def main():
    async with websockets.connect("ws://127.0.0.1:8080/ws") as member, \
               websockets.connect("ws://127.0.0.1:8080/ws") as flooder:
        await member.send(bytes([0x01, 1, len(room)]) + room)
        await flooder.send(pub(bytes(2048)))
        try:
            await asyncio.wait_for(flooder.recv(), 5)
            raise SystemExit("  FAIL the flooder got a reply instead of being closed")
        except websockets.ConnectionClosed:
            pass
        await member.send(pub(bytes(500)))
        frame = await asyncio.wait_for(member.recv(), 5)
        n = frame[1]
        payload = frame[2 + n + 16:]
        assert len(payload) == 500, f"member got a {len(payload)}-byte frame first"
asyncio.run(main())
PYEOF
curl -sf "$RELAY_URL/healthz" >/dev/null || fail "the relay died"
ok "the 2 KiB message closed the flooder; the member's 500-byte frame went through"
stop $R
sleep 0.3

echo "3. a spray across made-up addresses stays under the relay cap"
start_relay spray.log OPENCONV_RELAY_CAP=16KiB OPENCONV_RELAY_DB="$LAB/spray.db"
"$PY" -I - <<'PYEOF' || fail "the sprayer failed"
import asyncio, websockets
async def main():
    async with websockets.connect("ws://127.0.0.1:8080/ws") as ws:
        for i in range(200):  # 200 addresses x 300 bytes = 60 KB
            to = f"made-up-{i}".encode()
            await ws.send(bytes([0x02, len(to)]) + to + bytes(300))
        probe = b"probe"
        await ws.send(bytes([0x01, 1, len(probe)]) + probe)
        await ws.send(bytes([0x02, len(probe)]) + probe + b"x")
        await asyncio.wait_for(ws.recv(), 60)
asyncio.run(main())
PYEOF
read -r total addresses newest < <(sqlite3 -separator ' ' "$LAB/spray.db" "SELECT SUM(length(payload)), COUNT(DISTINCT address), (SELECT COUNT(*) FROM frames WHERE address = CAST('made-up-199' AS BLOB)) FROM frames")
recorded=$(sqlite3 "$LAB/spray.db" "SELECT SUM(bytes) FROM sequences")
echo "  info stored $total bytes across $addresses addresses (recorded $recorded); newest kept: $newest"
{ [ "$total" -le 16384 ] && [ "$total" = "$recorded" ] && [ "$newest" = 1 ]; } || fail "relay cap not held"
grep -q "over its total cap" "$LAB/spray.log" || fail "no warning logged"
ok "60 KB sprayed, 16 KiB cap held, oldest went first, and the relay warned"
echo "lab passed"
