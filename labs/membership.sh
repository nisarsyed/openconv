#!/usr/bin/env bash
# Membership with the real app and relay (#112, #123): remove, a replayed
# join request refused, leave, and re-adding a member stranded by retention.
source "$(dirname "$0")/lib.sh"
need_relay; need_app; need_python
export OPENCONV_AUTO_ACCEPT=1
clear_port
start_relay relay.log

launch alice 1 alice host; A=$LAST
wait_for 30 "$LAB/alice.1.log" "created the group"
CH=$(channel_of alice "$LAB/alice.1.log")
launch bob 1 bob join "$CH"; B=$LAST
wait_for 30 "$LAB/bob.1.log" "joined the group"
launch carol 1 carol join "$CH"
wait_for 30 "$LAB/carol.1.log" "joined the group"
launch dave 1 dave join "$CH"; D=$LAST
wait_for 30 "$LAB/dave.1.log" "joined the group"
sleep 1
ok "four members"

echo "1. a member is removed"
stop $A
launch alice 2 alice remove carol; A=$LAST
wait_for 30 "$LAB/carol.1.log" "removed from the group"
stop $B
launch bob 2 bob host "said after carol was removed"; B=$LAST
wait_for 30 "$LAB/alice.2.log" "received: said after carol was removed"
wait_for 30 "$LAB/dave.1.log" "received: said after carol was removed"
sleep 1
grep -q "received: said after carol was removed" "$LAB/carol.1.log" && fail "carol read a message after removal"
ok "carol was told she was removed, and did not receive what came after"

echo "1b. carol's old join request, replayed, does not put her back"
before=$(cat "$LAB"/alice.*.log "$LAB"/bob.*.log | grep -c "admitted a new member")
CHANNEL_HEX=$CH PY_DB="$LAB/relay.db" "$PY" -I - <<'PYEOF' || fail "could not replay the request"
import asyncio, os, sqlite3, websockets
channel = bytes.fromhex(os.environ["CHANNEL_HEX"])
db = sqlite3.connect(os.environ["PY_DB"])
# carol's KeyPackage frame: tag 1, and her name is in its credential.
(payload,) = db.execute(
    "SELECT payload FROM frames WHERE address = ? AND substr(payload, 1, 1) = x'01' "
    "AND instr(payload, CAST('carol' AS BLOB)) > 0 ORDER BY seq LIMIT 1", (channel,)).fetchone()
async def main():
    async with websockets.connect("ws://127.0.0.1:8080/ws") as ws:
        await ws.send(bytes([0x02, len(channel)]) + channel + payload)
        await asyncio.sleep(1)
asyncio.run(main())
PYEOF
sleep 3
after=$(cat "$LAB"/alice.*.log "$LAB"/bob.*.log | grep -c "admitted a new member")
[ "$before" = "$after" ] || fail "the replay admitted someone ($before -> $after)"
ok "the replayed request was refused by everyone online"

echo "2. a member leaves"
stop $D
launch dave 2 dave leave
wait_for 30 "$LAB/dave.2.log" "asked to leave"
wait_for 30 "$LAB/dave.2.log" "left the group"
ok "dave asked to leave, and someone committed it"

echo "3. a member stranded by retention is re-added"
launch erin 1 erin join "$CH"; E=$LAST
wait_for 30 "$LAB/erin.1.log" "joined the group"
sleep 1
stop $E
launch frank 1 frank join "$CH" "frank joined while erin was away"
wait_for 30 "$LAB/frank.1.log" "joined the group"
wait_for 30 "$LAB/alice.2.log" "received: frank joined while erin was away"
# Retention takes everything, frank's admitting commit included.
sqlite3 "$LAB/relay.db" "DELETE FROM frames;"
launch erin 2 erin host "erin is back"
wait_for 30 "$LAB/erin.2.log" "the relay no longer holds what this client missed"
wait_for 30 "$LAB/erin.2.log" "asked the group to add this client back"
wait_for 30 "$LAB/erin.2.log" "joined the group"
wait_for 30 "$LAB/alice.2.log" "received: erin is back"
wait_for 30 "$LAB/frank.1.log" "received: erin is back"
ok "erin was stranded, asked to come back, was re-added, and everyone reads her"
no_errors "$LAB"/*.log
echo "lab passed"
