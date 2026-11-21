#!/usr/bin/env bash
# Run the app on a throwaway test config inside the repo.
#
#   scripts/dev-run.sh             reuse .dev/config.kdl (create it if missing)
#   scripts/dev-run.sh --fresh     start again from tests/configs/default.kdl
#   scripts/dev-run.sh --page appearance
#
# The config, the override file and the history all stay in .dev/, which git
# ignores. Your real ~/.config/niri/config.kdl is never read or written.
# Note: the Displays page still changes your real displays for the session.
set -eu
cd "$(dirname "$0")/.."

dir=.dev
if [ "${1:-}" = "--fresh" ]; then
    rm -rf "$dir"
    shift
fi
mkdir -p "$dir/state"
if [ -f "$dir/config.kdl" ] && [ ! -w "$dir/config.kdl" ]; then
    chmod u+w "$dir/config.kdl"
fi
if [ ! -f "$dir/config.kdl" ]; then
    # The fixture is read-only; the copy must be writable or the app treats
    # it like a Nix-generated file and refuses changes.
    install -m 644 tests/configs/default.kdl "$dir/config.kdl"
    echo "Created $dir/config.kdl from tests/configs/default.kdl"
fi

export XDG_STATE_HOME="$PWD/$dir/state"
exec cargo run -p niri-settings-app -- --config "$PWD/$dir/config.kdl" "$@"
