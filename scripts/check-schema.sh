#!/usr/bin/env bash
# Fail when data/schema.json does not match the niri source in the dev shell.
# With --write, regenerate the schema and record the niri version.
set -eu
cd "$(dirname "$0")/.."

: "${NIRI_SRC:?run this inside nix develop; NIRI_SRC is not set}"
: "${NIRI_VERSION:?run this inside nix develop; NIRI_VERSION is not set}"

if [ "${1:-}" = "--write" ]; then
    cargo run -q -p niri-settings-schema-gen -- "$NIRI_SRC" > data/schema.json
    printf '%s\n' "$NIRI_VERSION" > data/schema.version
    echo "Wrote data/schema.json and data/schema.version for niri $NIRI_VERSION."
    exit 0
fi

recorded=$(cat data/schema.version 2>/dev/null || true)
if [ "$recorded" != "$NIRI_VERSION" ]; then
    echo "data/schema.json was generated for niri ${recorded:-unknown}, but"
    echo "the dev shell has niri $NIRI_VERSION. A niri bump needs a reviewed"
    echo "schema diff:"
    echo "  scripts/check-schema.sh --write"
    echo "  git diff data/schema.json data/schema.version"
    exit 1
fi

generated=$(mktemp)
trap 'rm -f "$generated"' EXIT

cargo run -q -p niri-settings-schema-gen -- "$NIRI_SRC" > "$generated"

if ! diff -u data/schema.json "$generated"; then
    echo
    echo "data/schema.json is stale. Regenerate it inside the dev shell:"
    echo "  scripts/check-schema.sh --write"
    exit 1
fi

echo "data/schema.json matches niri $NIRI_VERSION from the dev shell."
