#!/usr/bin/env bash
# Regenerate the Swift bindings from the Rust core. Run after changing
# anything under crates/core/src/ffi.rs.
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="${1:-release}"
OUT=clients/macos/Sources
GEN=$(mktemp -d)

echo "building openconv-core ($PROFILE)..."
cargo build -p openconv-core --"$PROFILE" >/dev/null

echo "generating swift bindings..."
cargo run --quiet -p openconv-core --features cli --bin uniffi-bindgen -- \
  generate --library "target/$PROFILE/libopenconv_core.a" \
  --language swift --out-dir "$GEN"

# UniFFI emits three files; they belong to two different SwiftPM targets.
cp "$GEN/openconv_core.swift"        "$OUT/OpenConvCore/"
cp "$GEN/openconv_coreFFI.h"         "$OUT/openconv_coreFFI/include/"
# SwiftPM synthesises the modulemap for a C target from include/, so the
# generated one is deliberately not copied.

echo "bindings written to $OUT"
