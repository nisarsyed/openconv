# OpenConv dev tasks.

# Run every check: rust tests plus the swift bridge verification.
check: test bridge smoke

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
