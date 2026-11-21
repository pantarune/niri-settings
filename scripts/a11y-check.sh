#!/usr/bin/env bash
# Run the accessibility audit on every page on a private Broadway display,
# so it works without a desktop session (CI included).
set -eu
cd "$(dirname "$0")/.."

shot=$(mktemp --suffix=.png)
trap 'rm -f "$shot"' EXIT

cargo build -q -p niri-settings-app

pages="home appearance layout apps keyboard shortcuts mouse displays workspaces animations startup health all config"
status=0
for page in $pages; do
    output=$(NIRI_SETTINGS_A11Y=1 scripts/screenshot.sh "$page" 1040x760 "$shot" 2>&1 || true)
    if echo "$output" | grep -q "a11y issue"; then
        echo "Page $page:"
        echo "$output" | grep "a11y issue"
        status=1
    fi
    if ! echo "$output" | grep -q "a11y audit"; then
        echo "Page $page did not produce an audit:"
        echo "$output"
        status=1
    fi
done

if [ "$status" -eq 0 ]; then
    echo "a11y audit: no issues on all pages"
fi
exit "$status"
