# Agent instructions

The standard settings app for the niri compositor. The design is in
`docs/design.md`; read it before any task. It wins over guesses, and over
anything you remember about other niri settings apps.

## State

- Version 0.1.0 is usable: core, CLI, GTK4 app and D-Bus service exist.
- Stack: Rust (edition 2024), GTK4 + libadwaita via `relm4`, `niri-ipc`,
  `notify`. Cargo workspace under `crates/`.
- `crates/core` owns the config model, validation, history, lints and imports.
  `crates/cli` is the `niri-settings-cli` command. `crates/app` is the
  `niri-settings-app` window. `crates/dbus` is `niri-settings-daemon`.
  `crates/schema-gen` reads niri-config source and writes `data/schema.json`.
- The app shell is `crates/app/src/app.rs`; dialogs, display changes, shortcut
  and list writes live in `crates/app/src/app/`.
- `data/schema.json` has 194 options from niri 26.04. The curated catalogue has
  68 designed options. The rest appear under "From the niri schema" in the app.
- Page titles and common controls have German, French and Turkish translations
  in `crates/app/src/i18n.rs`. Option help falls back to English.

## Commands

The GTK headers come from the Nix dev shell. Run commands inside it.

```sh
nix develop
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/check-schema.sh
scripts/check-schema.sh --write
scripts/check-packaging.sh
nix build .#niri-settings-app
cargo run -p niri-settings-schema-gen -- /path/to/niri > data/schema.json
```

`--write` updates both `data/schema.json` and `data/schema.version`. Commit
the diff when the niri version changes.

`scripts/dev-run.sh` runs the app on a test config in `.dev/` (git-ignored),
with history kept there too. `--fresh` resets it from
`tests/configs/default.kdl`.
- `packaging/` holds the nixpkgs, AUR and Fedora recipes. They are submitted
outside this repository; `scripts/check-packaging.sh` builds the nixpkgs
recipe against the working tree. `--strict` fails on release placeholders.

`cargo test` runs `niri validate` for the "all options" test. Skip that test only
if niri is absent.

## Invariants

- `crates/core` has no UI code and is the only crate that reads or writes
  config files or runs `niri`.
- Every write follows design section 7.1: scratch copy, `niri validate`,
  re-check every file on disk, record history, write the recovery journal,
  ordered per-file renames, mark the journal committed, remove it. On open,
  replay an unfinished write: pending rolls back, committed rolls forward.
  The re-check is not a lock: a program that writes in the instant before
  the rename can still be overwritten. Do not claim otherwise.
- An unchanged config must round-trip to the same bytes. Only changed values
  are rewritten.
- In the app, writes run on a worker thread (`crates/app/src/jobs.rs`). While
  one runs, every message not listed in `AppModel::runs_while_busy` waits in a
  queue. A new message that changes the session must stay off that list, or it
  edits an old copy that the finished write then replaces.
- Tests and scripts never touch the real `~/.config/niri/config.kdl`; use
  `tempfile` and `--config`.
- Items marked **Verify** in the design are checked against niri 26.04 source.
  Do not change a dependent behavior without a source check.
- One milestone at a time; small pull requests with tests.

## Verified niri facts

- `include` merges at its own line; later values win. Includes nest up to 10
  levels. `optional=true` accepts a missing file.
- One section node per file. A second `binds` node in one file is an error.
- A later bind with the same key replaces an earlier bind.
- `layout { border {} }` enables the border in the main file only. In an
  included file it does nothing.
- `struts`, preset widths, pointing-device sections and animation subsections
  replace the whole section.
- Top-level flags accept `false`. Nested flags such as `natural-scroll` do not
  accept an argument.
- `niri msg --json outputs|windows|pick-window` works. `niri msg output NAME`
  changes a display for the session only.
- `niri msg action load-config-file` reloads the config.
- The first matching `output` block wins. Display edits use the first block.
- Config lookup (niri 26.04 `src/main.rs`, `niri-config` `ConfigPath`):
  `--config`, then non-empty `$NIRI_CONFIG` (loaded as given, never
  created); else the user file; else `/etc/niri/config.kdl`; else niri creates
  the user file at compositor start. `crates/core/src/location.rs` mirrors
  this. The niri repo is now `github.com/niri-wm/niri`.
- `niri validate` accepts absolute includes. Validation uses a mirror with
  rewritten include paths, so outside files can be edited safely.

## Test notes

- `tests` live in the modules. `session::tests` covers writes, includes,
  managed overrides, undo, binds and output blocks.
- `crates/core/tests/configs.rs` covers real fixtures, symlinks, absolute
  includes and conflict checks.
- `crates/schema-gen` tests the extractor and the checked-in schema.
- `NIRI_SETTINGS_A11Y=1 cargo run -p niri-settings-app` prints an accessibility
  control-name audit. `scripts/a11y-check.sh` runs it on every page and fails
  on a finding. `docs/accessibility.md` has the Orca procedure.
- `scripts/screenshot.sh PAGE [WxH] [OUT.png]` saves a PNG of a page on GTK's
  Broadway backend, so nothing opens on the desktop. Use it to check UI work.
- `NIRI_SETTINGS_MEASURE=1 cargo run -p niri-settings-app` prints the minimum
  width of the top bar and of every page. Pages must fit the 360 px window
  minimum. The bottom-bar breakpoint is computed from the top bar at start.
- `every_option_writes_a_config_that_niri_accepts` is the guard for the option
  writers. Add an option and this test covers it.
- `import` and `lint` have pure-function tests.
