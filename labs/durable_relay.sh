#!/usr/bin/env bash
# The relay's durability over the raw wire (#104): SIGKILL and restart the
# real relay binary, and check stored frames and sequence numbers survive.
# The Python side re-implements the wire format from crates/wire, so a bug
# shared by the relay and its own tests would still show. Needs no app, and
# uses its own port (18080).
source "$(dirname "$0")/lib.sh"
need_relay; need_python
exec "$PY" -I "$LABS/durable_relay.py" "$RELAY" "$LAB"
