#!/usr/bin/env bash
# Three real apps form a group, are all SIGKILLed, and are relaunched from
# their encrypted databases; they must still talk (#107).
source "$(dirname "$0")/lib.sh"
need_relay; need_app
export OPENCONV_AUTO_ACCEPT=1
clear_port
start_relay relay.log

echo "phase 1: form a three-member group"
launch alice 1 alice host; A=$LAST
wait_for 30 "$LAB/alice.1.log" "created the group"
CHANNEL=$(channel_of alice "$LAB/alice.1.log")
launch bob 1 bob join "$CHANNEL" "before the crash"; B=$LAST
wait_for 30 "$LAB/bob.1.log" "joined the group"
launch carol 1 carol join "$CHANNEL"; C=$LAST
wait_for 30 "$LAB/carol.1.log" "joined the group"
wait_for 30 "$LAB/alice.1.log" "received: before the crash"
sleep 1 # let carol's join commit reach everyone
ok "group formed on channel ${CHANNEL:0:16}…"

echo "phase 2: SIGKILL all three clients"
stop $A $B $C

echo "phase 3: relaunch from disk; bob speaks"
launch alice 2 alice host
wait_for 30 "$LAB/alice.2.log" "rejoined the group"
launch carol 2 carol host
wait_for 30 "$LAB/carol.2.log" "rejoined the group"
launch bob 2 bob host "after the crash"
wait_for 30 "$LAB/bob.2.log" "rejoined the group"
wait_for 30 "$LAB/alice.2.log" "received: after the crash"
wait_for 30 "$LAB/carol.2.log" "received: after the crash"
for who in alice carol bob; do
    ch=$(channel_of $who "$LAB/$who.2.log")
    [ "$ch" = "$CHANNEL" ] || fail "$who came back on a different channel: $ch"
done
ok "all three restored the same channel from disk"
ok "alice and carol decrypted bob's message sent after the crash"
no_errors "$LAB"/*.2.log
echo "lab passed"
