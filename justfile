# OpenConv dev tasks.

# Run every check: formatting, lints, tests, bridge, and the real app.
check: fmt-check lint test bridge smoke

# Format both languages.
fmt:
    cargo fmt --all
    cd clients/macos && swift format --in-place --recursive Sources/OpenConv Sources/BridgeCheck

# Fail if anything is unformatted.
fmt-check:
    cargo fmt --all --check
    cd clients/macos && swift format lint --strict --recursive Sources/OpenConv Sources/BridgeCheck

# Check dependency licences, advisories, bans and sources.
# Needs `cargo install cargo-deny --locked`.
deny:
    cargo deny check

# Lint both languages. Generated bindings are deliberately not linted.
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cd clients/macos && swift format lint --recursive Sources/OpenConv Sources/BridgeCheck

# Rust unit and integration tests.
test:
    cargo test

# Regenerate the Swift bindings from crates/core/src/ffi.rs.
bindings:
    ./scripts/gen-bindings.sh

# Verify the Rust<->Swift bridge at runtime.
bridge: bindings
    cd clients/macos && swift run BridgeCheck

# End-to-end check against the real app: relay plus two live clients.
smoke:
    ./scripts/smoke.sh

# Build what the labs drive: the relay and the app.
lab-build: bindings
    cargo build -p openconv-server
    cd clients/macos && swift build

# Create the Python environment the labs use.
lab-setup:
    python3 -m venv labs/.venv
    labs/.venv/bin/pip install --quiet -r labs/requirements.txt

# Run one lab (labs/README.md) against what is built: `just lab catchup`.
lab name *dir:
    labs/{{name}}.sh {{dir}}

# Build, then run every lab and report which failed.
labs: lab-build
    #!/usr/bin/env bash
    set -u
    failed=()
    for lab in restart catchup membership approval retention abuse durable_relay; do
        echo "== $lab"
        labs/$lab.sh || failed+=("$lab")
    done
    if [ ${#failed[@]} -gt 0 ]; then echo "failed: ${failed[*]}"; exit 1; fi
    echo "all labs passed"

# Start the relay on 127.0.0.1:8080.
relay:
    cargo run -p openconv-server

# Launch a client. The first one hosts; pass a name to tell them apart.
#   just client alice     (then click Host, and copy the channel id it shows)
#   just client bob       (then paste that channel id and click Join)
client name="me": bindings
    cd clients/macos && swift run OpenConv {{name}}

# Remove build artifacts from both toolchains.
clean:
    cargo clean
    rm -rf clients/macos/.build
