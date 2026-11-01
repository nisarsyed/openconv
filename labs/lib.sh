# shellcheck shell=bash
# Shared by the labs. Source it first: `source "$(dirname "$0")/lib.sh"`.
#
# Each lab drives the real relay binary, and most the real app, through a
# scenario that unit and integration tests cannot reach: SIGKILL, restarts, a
# relay losing its store, clients away past retention. See labs/README.md.
#
# Usage of any lab: `labs/<name>.sh [run-dir]`. The run directory holds every
# log and database; it defaults to a fresh temporary one, and an existing one
# is only used if it is empty.

set -uo pipefail

LABS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$LABS/.." && pwd)"
APP="$REPO/clients/macos/.build/debug/OpenConv"
RELAY="$REPO/target/debug/openconv-server"
PY="$LABS/.venv/bin/python"
RELAY_URL="http://127.0.0.1:8080"

if [ $# -ge 1 ]; then
    LAB="$1"
    if [ -e "$LAB" ] && [ -n "$(ls -A "$LAB")" ]; then
        echo "refusing to use $LAB: it is not empty" >&2
        exit 2
    fi
else
    LAB="$(mktemp -d -t openconv-lab)"
fi
mkdir -p "$LAB/data"
echo "lab run directory: $LAB"

# Clients keep their key in a file here rather than the Keychain, whose prompt
# would hang a headless run (see CLAUDE.md, Traps).
export OPENCONV_DATA_DIR="$LAB/data"
export OPENCONV_RELAY_DB="$LAB/relay.db"

pids=()
cleanup() { [ ${#pids[@]} -gt 0 ] && kill -9 "${pids[@]}" 2>/dev/null; return 0; }
trap cleanup EXIT

# stop PID...: SIGKILL these and reap them, so bash prints no job notice.
stop() {
    kill -9 "$@" 2>/dev/null
    wait "$@" 2>/dev/null
    return 0
}

ok() { echo "  ok   $*"; }
fail() { echo "  FAIL $*"; exit 1; }

need_relay() { [ -x "$RELAY" ] || fail "no relay at $RELAY: run 'just lab-build' first"; }
need_app() { [ -x "$APP" ] || fail "no app at $APP: run 'just lab-build' first"; }
need_python() { [ -x "$PY" ] || fail "no Python for the labs at $PY: run 'just lab-setup' first"; }

# A relay left on port 8080 by an earlier run makes failures look like bugs.
clear_port() { lsof -ti:8080 | xargs kill -9 2>/dev/null; sleep 0.3; }

# wait_for SECONDS FILE PATTERN: wait until PATTERN appears in FILE.
wait_for() {
    for _ in $(seq 1 $(($1 * 10))); do
        grep -q "$3" "$2" 2>/dev/null && return 0
        sleep 0.1
    done
    echo "  FAIL timed out waiting for '$3' in $(basename "$2")"
    echo "--- $(basename "$2")"
    cat "$2"
    exit 1
}

# never SECONDS FILE PATTERN: wait, then fail if PATTERN appeared in FILE.
never() {
    sleep "$1"
    if grep -q "$3" "$2"; then
        echo "  FAIL '$3' appeared in $(basename "$2")"
        cat "$2"
        exit 1
    fi
}

# launch NAME RUN ARGS...: start the app as NAME (log NAME.RUN.log), with ARGS
# as its launch arguments. Leaves its pid in LAST.
launch() {
    local log="$LAB/$1.$2.log"
    shift 2
    "$APP" "$@" >"$log" 2>&1 &
    LAST=$!
    pids+=("$LAST")
}

# start_relay LOG [VAR=value...]: start the relay with extra environment,
# logging to LOG, and wait until it answers. Leaves its pid in R.
start_relay() {
    local log="$LAB/$1"
    shift
    env "$@" "$RELAY" >"$log" 2>&1 &
    R=$!
    pids+=("$R")
    for _ in $(seq 1 50); do
        curl -sf "$RELAY_URL/healthz" >/dev/null && return 0
        sleep 0.1
    done
    fail "the relay never came up; see $log"
}

# channel_of NAME LOG: the channel id NAME logged.
channel_of() { sed -n 's/^\['"$1"'\] channel \([0-9a-f]*\)$/\1/p' "$2" | head -1; }

# no_errors GLOB...: fail if any of these logs reported an error.
no_errors() {
    for f in "$@"; do
        if grep -q 'error' "$f"; then
            echo "  FAIL errors in $(basename "$f")"
            grep error "$f"
            exit 1
        fi
    done
}
