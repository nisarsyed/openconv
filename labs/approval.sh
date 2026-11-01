#!/usr/bin/env bash
# Join approval with the real app and relay (#125): a join waits for a
# member, accepting lets it in, and a removed member who asks again is
# flagged and stays out. Runs without auto-accept, unlike the other labs.
source "$(dirname "$0")/lib.sh"
need_relay; need_app
unset OPENCONV_AUTO_ACCEPT
clear_port
start_relay relay.log

echo "1. a join request waits for a member"
launch alice 1 alice host; A=$LAST
wait_for 30 "$LAB/alice.1.log" "created the group"
CH=$(channel_of alice "$LAB/alice.1.log")
launch bob 1 bob join "$CH"; B=$LAST
wait_for 30 "$LAB/alice.1.log" "bob asked to join"
never 4 "$LAB/bob.1.log" "joined the group"
ok "bob asked, alice saw it, and bob stayed out without an answer"

echo "2. accepting lets them in"
stop $A
launch alice 2 alice accept bob; A=$LAST
wait_for 30 "$LAB/alice.2.log" "letting bob in"
wait_for 30 "$LAB/bob.1.log" "joined the group"
ok "alice accepted, and bob joined"

echo "3. a removed member who asks again is flagged and stays out"
stop $A
launch alice 3 alice remove bob; A=$LAST
wait_for 30 "$LAB/bob.1.log" "removed from the group"
stop $B $A
launch alice 4 alice host
wait_for 30 "$LAB/alice.4.log" "caught up"
launch bob 2 bob join "$CH"
wait_for 30 "$LAB/alice.4.log" "bob asked to join (they were removed by alice"
never 4 "$LAB/bob.2.log" "joined the group"
ok "bob's new request was flagged as removed by alice, and he stayed out"
no_errors "$LAB"/*.log
echo "lab passed"
