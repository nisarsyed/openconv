#!/usr/bin/env bash
# Catch-up with the real app and relay (#105). Clients are SIGKILLed and
# relaunched; everything they missed must arrive through catch-up, including
# a join request made while nobody was online, and after the relay loses its
# store.
source "$(dirname "$0")/lib.sh"
need_relay; need_app
export OPENCONV_AUTO_ACCEPT=1
clear_port
start_relay relay.log

echo "scenario A: a member who was away catches up"
launch alice 1 alice host; A=$LAST
wait_for 30 "$LAB/alice.1.log" "created the group"
CH=$(channel_of alice "$LAB/alice.1.log")
launch bob 1 bob join "$CH"; B=$LAST
wait_for 30 "$LAB/bob.1.log" "joined the group"
launch carol 1 carol join "$CH"; C=$LAST
wait_for 30 "$LAB/carol.1.log" "joined the group"
sleep 1
stop $C
ok "three members; carol killed"
launch dave 1 dave join "$CH" "hello from dave"; D=$LAST
wait_for 30 "$LAB/dave.1.log" "joined the group"
wait_for 30 "$LAB/alice.1.log" "received: hello from dave"
stop $B
launch bob 2 bob host "bob while carol away"; B=$LAST
wait_for 30 "$LAB/alice.1.log" "received: bob while carol away"
ok "dave joined and spoke, bob restarted and spoke, all while carol was away"
launch carol 2 carol host "carol is back"
wait_for 30 "$LAB/carol.2.log" "received: hello from dave"
wait_for 30 "$LAB/carol.2.log" "received: bob while carol away"
ok "carol caught up on both messages, after the commit that admitted dave"
for who in alice dave; do wait_for 30 "$LAB/$who.1.log" "received: carol is back"; done
wait_for 30 "$LAB/bob.2.log" "received: carol is back"
ok "alice, bob and dave all read carol's first words back"
[ "$(channel_of carol "$LAB/carol.2.log")" = "$CH" ] || fail "carol came back on another channel"
no_errors "$LAB"/*.log
stop $A $D $B

echo "scenario B: a join request made while everyone was away"
launch alice2 1 alice2 host; A=$LAST
wait_for 30 "$LAB/alice2.1.log" "caught up"
CH2=$(channel_of alice2 "$LAB/alice2.1.log")
stop $A
launch erin 1 erin join "$CH2"; E=$LAST
wait_for 30 "$LAB/erin.1.log" "asked to join"
sleep 1
stop $E
ok "erin asked while nobody was online, then went away too"
launch alice2 2 alice2 host; A=$LAST
wait_for 30 "$LAB/alice2.2.log" "admitted a new member"
ok "alice came back and admitted erin from the held request"
launch erin 2 erin host "erin made it"; E=$LAST
wait_for 30 "$LAB/erin.2.log" "joined the group"
wait_for 30 "$LAB/alice2.2.log" "received: erin made it"
sleep 1
n=$(grep -c "admitted a new member" "$LAB/alice2.2.log")
[ "$n" = 1 ] || fail "alice admitted erin $n times"
ok "erin joined from her stored Welcome; her re-offer added nobody twice"
no_errors "$LAB"/*.log
stop $A $E

echo "scenario C: the relay comes back with a fresh store"
stop $R
sleep 0.3
start_relay relay2.log OPENCONV_RELAY_DB="$LAB/fresh-relay.db"
grep -q "created a new, empty relay store" "$LAB/relay2.log" || fail "the relay did not warn that its store is new"
ok "the relay warned that its store is new"
launch alice2 3 alice2 host; A=$LAST
wait_for 30 "$LAB/alice2.3.log" "caught up"
launch erin 3 erin host "after the reset"
wait_for 30 "$LAB/erin.3.log" "following it from its start"
wait_for 30 "$LAB/alice2.3.log" "received: after the reset"
grep -q "following it from its start" "$LAB/alice2.3.log" || fail "alice2 did not resync"
ok "both resynced to the new relay and kept talking"
no_errors "$LAB"/*.3.log
echo "lab passed"
