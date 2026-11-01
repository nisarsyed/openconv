#!/usr/bin/env bash
# Fails if the checked-in theme tokens are not what design/tokens.json
# generates: someone changed the tokens without running `just theme`, or
# edited the generated Swift by hand.
set -euo pipefail
cd "$(dirname "$0")/.."

checked_in=clients/macos/Sources/OpenConv/Theme/Tokens.swift
fresh=$(mktemp -d)/Tokens.swift
trap 'rm -rf "$(dirname "$fresh")"' EXIT

swift scripts/gen-theme.swift design/tokens.json "$fresh"
if ! diff -u "$checked_in" "$fresh"; then
    echo
    echo "$checked_in is out of date with design/tokens.json: run \`just theme\`."
    exit 1
fi
echo "theme tokens are up to date"
