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

# Start the relay on 127.0.0.1:8080.
relay:
    cargo run -p openconv-server

# Launch a client. The first one hosts; pass a name to tell them apart.
#   just client alice     (then click Host)
#   just client bob       (then click Join)
client name="me": bindings
    cd clients/macos && swift run OpenConv {{name}}

# Remove build artifacts from both toolchains.
clean:
    cargo clean
    rm -rf clients/macos/.build
