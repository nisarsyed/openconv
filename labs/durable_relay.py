"""Lab for #104: drive the real relay binary over real WebSockets, crash it
with SIGKILL, restart it, and check durability and sequence continuity.

Independent of the Rust test helpers: the wire format is re-implemented here
from crates/wire/src/lib.rs, so a bug shared by the relay and its own tests
would still show up.

Run it through durable_relay.sh. usage: python durable_relay.py <relay-binary> <work-dir>
"""

import asyncio
import os
import signal
import sqlite3
import struct
import subprocess
import sys
import time
import urllib.request

import websockets

RELAY, WORK = sys.argv[1], sys.argv[2]
PORT = 18080
URL = f"ws://127.0.0.1:{PORT}/ws"


def subscribe(*addresses: bytes) -> bytes:
    return bytes([0x01, len(addresses)]) + b"".join(bytes([len(a)]) + a for a in addresses)


def publish(to: bytes, payload: bytes) -> bytes:
    return bytes([0x02, len(to)]) + to + payload


def delivery(frame: bytes):
    assert frame[0] == 0x01, f"unknown delivery tag {frame[0]}"
    n = frame[1]
    address = frame[2 : 2 + n]
    seq, accepted_ms = struct.unpack(">QQ", frame[2 + n : 18 + n])
    return address, seq, accepted_ms, frame[18 + n :]


def start(db: str) -> subprocess.Popen:
    env = dict(os.environ, OPENCONV_RELAY_DB=db, OPENCONV_ADDR=f"127.0.0.1:{PORT}")
    log = open(os.path.join(WORK, "relay.log"), "ab")
    proc = subprocess.Popen([RELAY], env=env, stdout=log, stderr=log)
    for _ in range(100):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{PORT}/healthz", timeout=0.2)
            return proc
        except OSError:
            time.sleep(0.05)
    raise SystemExit("relay never came up")


def crash(proc: subprocess.Popen) -> None:
    proc.send_signal(signal.SIGKILL)
    proc.wait()


async def recv(ws, timeout=5.0):
    return delivery(await asyncio.wait_for(ws.recv(), timeout))


_probe = 0


async def subscribed(ws, *addresses: bytes) -> None:
    """Subscribe and wait until the relay has registered it, by echoing a
    probe on a private address in the same request stream."""
    global _probe
    _probe += 1
    probe = f"probe-{_probe}".encode()
    await ws.send(subscribe(*addresses, probe))
    await ws.send(publish(probe, b"probe"))
    while (await recv(ws))[0] != probe:
        pass


def rows(db: str, address: bytes):
    con = sqlite3.connect(db)
    try:
        frames = con.execute(
            "SELECT seq, accepted_ms, payload FROM frames WHERE address = ? ORDER BY seq",
            (address,),
        ).fetchall()
        nxt = con.execute("SELECT next_seq FROM sequences WHERE address = ?", (address,)).fetchone()
        return frames, (nxt[0] if nxt else None)
    finally:
        con.close()


def check(cond, msg):
    print(("  ok    " if cond else "  FAIL  ") + msg)
    if not cond:
        raise SystemExit(1)


async def restart_scenario():
    print("scenario A: crash and restart keeps numbering and frames")
    db = os.path.join(WORK, "a.db")
    relay = start(db)
    async with websockets.connect(URL) as a:
        await subscribed(a, b"room")
        seen = []
        for body in (b"m0", b"m1", b"m2"):
            await a.send(publish(b"room", body))
            _, seq, ts, payload = await recv(a)
            seen.append((seq, payload))
        check(seen == [(0, b"m0"), (1, b"m1"), (2, b"m2")], f"first run numbered 0..2: {seen}")
        # Nobody listens on the mailbox; the frame must be kept anyway.
        await a.send(publish(b"mailbox", b"welcome"))
        await subscribed(a)  # sync: the publish above has been handled
    crash(relay)

    frames, nxt = rows(db, b"room")
    check([(s, p) for s, _, p in frames] == [(0, b"m0"), (1, b"m1"), (2, b"m2")],
          "after SIGKILL the store holds m0..m2 with their numbers")
    check(nxt == 3, f"stored next_seq for room is 3 (got {nxt})")
    check(all(ts > 1_700_000_000_000 for _, ts, _ in frames), "accepted timestamps are stored")
    mail, _ = rows(db, b"mailbox")
    check([(s, p) for s, _, p in mail] == [(0, b"welcome")], "frame to an unwatched mailbox was stored")

    relay = start(db)
    async with websockets.connect(URL) as a, websockets.connect(URL) as b:
        await subscribed(a, b"room", b"mailbox")
        await subscribed(b, b"room")
        await a.send(publish(b"room", b"m3"))
        got = [(await recv(ws))[1:4:2] for ws in (a, b)]
        check(got == [(3, b"m3"), (3, b"m3")], f"after restart both subscribers get m3 as seq 3: {got}")
        await a.send(publish(b"mailbox", b"second"))
        _, seq, _, _ = await recv(a)
        check(seq == 1, f"mailbox continues at seq 1 (got {seq})")
    crash(relay)


async def crash_under_load_scenario():
    print("scenario B: SIGKILL mid-flood loses nothing that was delivered")
    db = os.path.join(WORK, "b.db")
    senders, per_sender = 4, 2000
    relay = start(db)

    observers = [await websockets.connect(URL, max_queue=None) for _ in range(2)]
    for ws in observers:
        await subscribed(ws, b"load")
    transcripts = [[] for _ in observers]

    async def observe(i, ws):
        try:
            while True:
                _, seq, _, payload = await recv(ws, timeout=10)
                transcripts[i].append((seq, payload))
        except (websockets.ConnectionClosed, asyncio.TimeoutError):
            pass

    async def send(s):
        try:
            async with websockets.connect(URL) as ws:
                for i in range(per_sender):
                    await ws.send(publish(b"load", f"{s}:{i}".encode()))
                await asyncio.sleep(30)
        except (websockets.ConnectionClosed, OSError):
            pass

    watchers = [asyncio.create_task(observe(i, ws)) for i, ws in enumerate(observers)]
    blasters = [asyncio.create_task(send(s)) for s in range(senders)]
    while min(len(t) for t in transcripts) < 1500:
        await asyncio.sleep(0.005)
    crash(relay)
    await asyncio.gather(*watchers)
    for t in blasters:
        t.cancel()
    await asyncio.gather(*blasters, return_exceptions=True)

    frames, nxt = rows(db, b"load")
    stored = {s: p for s, _, p in frames}
    total = senders * per_sender
    print(f"  info  killed with {len(stored)} of {total} frames stored; "
          f"observers saw {[len(t) for t in transcripts]}")
    check(len(stored) < total, "the crash really landed mid-flood")
    check(sorted(stored) == list(range(len(stored))), "stored sequence is gapless from 0")
    check(nxt == len(stored), f"next_seq ({nxt}) matches the frames stored")
    for i, t in enumerate(transcripts):
        check([s for s, _ in t] == list(range(len(t))), f"observer {i} saw a gapless order")
        check(all(stored.get(s) == p for s, p in t),
              f"every frame observer {i} received is on disk with the same bytes")
    shorter = min(transcripts, key=len)
    check(all(t[: len(shorter)] == shorter for t in transcripts), "observers agree on the order")

    relay = start(db)
    async with websockets.connect(URL) as ws:
        await subscribed(ws, b"load")
        await ws.send(publish(b"load", b"after"))
        _, seq, _, _ = await recv(ws)
        check(seq == len(stored), f"after restart the next frame is seq {len(stored)} (got {seq})")
    crash(relay)


async def main():
    await restart_scenario()
    await crash_under_load_scenario()
    print("lab passed")


asyncio.run(main())
