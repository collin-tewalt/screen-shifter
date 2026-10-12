## Why

Two gaps in the Ctrl+Alt panel. Releasing Ctrl+Alt over another monitor's tile does nothing, even though clicking that tile switches it, so a release-to-switch that matches the click is the natural completion of the panel. And the panel can only be opened by holding Ctrl+Alt, which is awkward on a laptop touchpad; a three-finger swipe up is the gesture a touchpad user reaches for.

## What Changes

- Releasing Ctrl+Alt with the mouse over a slot tile (not a preview) switches that tile's monitor to that slot, exactly as a click on the tile does. Releasing over the current slot's tile, or over "+ New", closes the panel without changing anything.
- A three-finger swipe up opens the same existing panel (no separate panel). From there the user can hover to spread windows, click a tile to switch, or click a preview to choose a window. Clicks decide; key release does not apply in this mode.
- The panel closes from this mode with Esc, a click on empty space, a three-finger swipe down, or after a click on a tile or preview. Every exit clears the mode so input is never stuck.
- The app detects the gesture from the touchpad directly. If Windows does not let the app suppress its own three-finger action (Task View by default), the app asks the owner in-app whether to turn that Windows setting off; only with consent does it change the setting, and it restores the original value on Exit.
- **BREAKING**: none for keyboard use. On machines where three-finger swipe up is already used for Task View, that gesture changes to open this panel once the owner consents.

## Capabilities

### New Capabilities
- `touchpad-panel-gesture`: a three-finger swipe up opens the slot panel, the exit paths for that mode, and the owner-consented change to the Windows three-finger setting (with restore on Exit).

### Modified Capabilities
- `slot-panel`: release over a slot tile switches its monitor (the release rule currently covers only previews).

## Impact

- `src/hooks.rs`: a second mode flag beside `CHORD` for mouse swallowing; raw touchpad input for the gesture.
- `src/main.rs`: `WM_CHORD_UP` release handling (tile case); a new message that opens the panel on swipe; Esc and exit handling in the new mode; the Windows setting restore on Exit.
- `src/panel.rs`: reuses `hit` for the tile lookup on release.
- `src/win.rs` or a new small module: read/write of the Windows three-finger setting (registry; value name to be confirmed by spike).
- Dependencies: none new expected; Raw Input is in the `windows` crate already in use.
- Specs: `openspec/specs/slot-panel` and the new `touchpad-panel-gesture`.
