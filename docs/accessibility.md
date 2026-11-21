# Accessibility

Every control has an accessible name. Icon-only buttons use a tooltip as the
name. Switches use the setting label.

## Automated audit

Run this command in the Nix dev shell:

```sh
scripts/a11y-check.sh
```

The script checks every page on a private Broadway display. It prints
`a11y audit: no issues on all pages`, or one line for each problem, and exits
non-zero when a page has a problem or does not produce an audit. The check
looks for icon buttons without a name and switches without a label.

## Manual Orca check

1. Start Orca.
2. Start niri settings.
3. Press Tab through the window.
4. Check that Orca reads the page name, the setting name and the help text.
5. Open a dialog. Check that Orca reads the heading and each button.
6. Turn on high contrast. Check that focus is visible on every page.
7. Set a large text scale. Check that no label is cut off.
8. Press Control+F and search for `gaps`. Check that the result is read aloud.

Run this check on every page before a release. Record the version of Orca and
niri in the release notes.

## Keyboard

- Control+Z: undo.
- Control+Shift+Z: redo.
- Control+F: search.
- Control+,: preferences.
- Escape: close a dialog or revert a risky display change.
- Tab and Shift+Tab: move through controls in reading order.
- Displays: Tab to the arrangement picture. Press 1–9 to choose a display,
  the arrow keys to move it, Shift+arrow for one pixel, Enter to apply and
  Escape to cancel the pending move.
- Shortcut recorder: Escape cancels, and a plain Tab leaves the Recorder
  button. A recorded chord stops the recording, so focus is never trapped.
