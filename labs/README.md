# Labs

Each lab drives the **real** relay binary, and most of them the real macOS
app, through a scenario that the Rust tests and the smoke test cannot reach:
SIGKILL mid-session, restarts from disk, a relay that loses its store,
clients away past retention, replayed requests. They check what a user would
see, through the app's own log lines and the relay's database.

They are manual, not part of `just check` or CI. Run the ones that touch what
you changed before opening a PR, and all of them before a release. Several
bugs that every test passed were found here.

## Running

```sh
just lab-setup          # once: the Python environment (websockets)
just labs               # build the relay and app, run every lab
just lab catchup        # run one lab (builds nothing)
labs/catchup.sh /tmp/x  # same, keeping logs and databases in /tmp/x
```

`just lab` and the scripts use whatever is already built, so run
`just lab-build` first after a change. Each run gets a fresh temporary
directory, printed at the start, holding every log and database; a directory
you name must be empty or not exist yet. On failure the lab prints the log
that let it down, and the directory is left for you to inspect.

Every lab binds port 8080 (the durable-relay lab uses 18080) and kills
whatever is already on it. Don't run two at once, or a lab next to `just
relay`. Clients use `OPENCONV_DATA_DIR`, so the Keychain is never touched.

## The labs

| Lab | What it shows | Needs |
|---|---|---|
| `restart` | three apps SIGKILLed and relaunched restore their group from disk and keep talking (#107) | app |
| `catchup` | a member away catches up; a join request made while nobody was online is answered later; clients follow a relay that lost its store (#105) | app |
| `membership` | remove, a replayed join request refused, leave, re-adding a member stranded by retention (#112, #123) | app, Python |
| `approval` | a join waits for a member's approval; a removed member who asks again is flagged and stays out (#125) | app |
| `retention` | bad settings refused, expiry past the window, a channel held under its size cap (#106) | app, Python |
| `abuse` | bad limits refused, an oversized message closes only its sender, a spray across made-up addresses stays under the relay cap (#122) | Python |
| `durable_relay` | SIGKILL and restart the relay under load; frames and sequence numbers survive (#104) | Python |

Every lab but `approval` runs with `OPENCONV_AUTO_ACCEPT=1`, since nobody is
there to approve joins. The Python labs re-implement the wire format from
`crates/wire` rather than reusing the Rust helpers, so a bug shared by the
relay and its own tests still shows.

## Writing one

Source `lib.sh`, which sets up the run directory, the binaries, cleanup of
every process started, and helpers: `start_relay`, `launch`, `wait_for`,
`never`, `channel_of`, `no_errors`, `ok`, `fail`. Wait on a log line rather
than sleeping where you can. A lab earns its place the same way a test does:
break the thing it checks and make sure it goes red.
