# Screenshots

These are generated from the app, not edited by hand. Run them inside
`nix develop` after a build:

```sh
cargo build -p niri-settings-app
scripts/screenshot.sh appearance 1040x760 docs/screenshots/appearance.png
scripts/screenshot.sh shortcuts 1040x760 docs/screenshots/shortcuts.png
NIRI_SETTINGS_WELCOME_STEP=source \
  scripts/screenshot.sh home 1040x760 docs/screenshots/welcome.png
```

GTK's Broadway backend renders the window off-screen, so nothing opens on the
desktop.
