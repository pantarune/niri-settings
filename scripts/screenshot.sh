#!/usr/bin/env bash
# Save a PNG of one page without opening a window on the desktop.
#
#   scripts/screenshot.sh PAGE [WIDTHxHEIGHT] [OUT.png]
#   scripts/screenshot.sh keyboard 360x640 /tmp/keyboard.png
#
# Runs the app on GTK's Broadway backend (a private display that nothing
# shows) with a copy of tests/configs/default.kdl in .dev/shot. Run it inside
# `nix develop` after `cargo build -p niri-settings-app`.
set -eu
cd "$(dirname "$0")/.."

page=${1:?page, for example appearance}
size=${2:-1040x760}
out=${3:-.dev/shot/$page-$size.png}

dir=.dev/shot
mkdir -p "$dir/state"
install -m 644 tests/configs/default.kdl "$dir/config.kdl"

display=:${NIRI_SETTINGS_BROADWAY:-7}
gtk4-broadwayd "$display" >/dev/null 2>&1 &
broadway=$!
trap 'kill $broadway 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do
    [ -S "${XDG_RUNTIME_DIR:-/tmp}/broadway$(( ${display#:} + 1 )).socket" ] && break
    sleep 0.1
done

XDG_STATE_HOME="$PWD/$dir/state" GDK_BACKEND=broadway BROADWAY_DISPLAY="$display" \
    GSK_RENDERER=cairo NIRI_SETTINGS_SCREENSHOT="$out" NIRI_SETTINGS_SIZE="$size" \
    timeout 30 target/debug/niri-settings-app --config "$PWD/$dir/config.kdl" --page "$page"
