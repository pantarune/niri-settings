#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/.."
config=$(mktemp)
trap 'rm -f "$config"' EXIT
printf 'layout {\n    gaps 10\n}\n' > "$config"
dbus-run-session -- bash -c "
  target/debug/niri-settings-daemon --config '$config' >/tmp/niri-settings-dbus.log 2>&1 &
  sleep 1
  busctl --user call io.github.niri_settings.NiriSettings1 /io/github/niri_settings/NiriSettings1 io.github.niri_settings.NiriSettings1 Get s layout.gaps
  busctl --user call io.github.niri_settings.NiriSettings1 /io/github/niri_settings/NiriSettings1 io.github.niri_settings.NiriSettings1 Set ss layout.gaps 20
  busctl --user call io.github.niri_settings.NiriSettings1 /io/github/niri_settings/NiriSettings1 io.github.niri_settings.NiriSettings1 Undo
"
