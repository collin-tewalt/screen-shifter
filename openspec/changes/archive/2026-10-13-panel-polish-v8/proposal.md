## Why

Two gaps in the Ctrl+Alt panel. Releasing Ctrl+Alt over another monitor's tile does nothing, even though clicking that tile switches it, so a release-to-switch that matches the click is the natural completion of the panel. And the panel can only be opened by holding Ctrl+Alt, which is awkward on a laptop touchpad; a three-finger swipe up is the gesture a touchpad user reaches for.

## What Changes

- Releasing Ctrl+Alt with the mouse over a slot tile (not a preview) switches that tile's monitor to that slot, exactly as a click on the tile does. Releasing over the current slot's tile, or over "+ New", closes the panel without changing anything.
- Three-finger vertical swipes move the screen live, as the fingers travel: 100 touchpad units up brings up the slot panel, 250 units brings up the exposé on every monitor, and moving back undoes each phase. Downward travel mirrors it, closing the exposé and then the panel. Releasing leaves the screen where it is.
- In the swipe-opened exposé, each monitor has its own spread; hovering a tile re-spreads only that monitor, and moving between monitors keeps the others open. Swiping down over a preview at 250 units chooses that window.
- Three-finger left and right swipes switch the slot under the cursor once, as the swipe passes 100 units (left is next, right previous, matching the slide animation).
- The panel closes from swipe mode with Esc, a click on empty space, a click on a tile or preview (which acts), or Ctrl+Alt release. Every exit clears the mode so input is never stuck.
- The Windows three-finger slide setting is turned off only with the owner's Yes at a first-start prompt. The original value is restored on Exit, on a panic, and on the next start after a run that ended without restoring. A tray item turns the feature on or off.
- Expose collapse keeps the previews in their windows' stacking order, so they no longer flicker as the windows reappear.
- **BREAKING**: none for keyboard use. On machines where three-finger swipe is already used for Task View, that gesture changes to this panel once the owner consents.

## Capabilities

### New Capabilities
- `touchpad-panel-gesture`: three-finger vertical swipes that bring up the panel and the exposé live, left and right slot switching, the exit paths for swipe mode, and the owner-consented change to the Windows three-finger setting (restored on Exit).

### Modified Capabilities
- `slot-panel`: release over a slot tile switches its monitor (the release rule previously covered only previews).
- `slot-expose`: previews fold back in their windows' stacking order.

## Impact

- `src/hooks.rs`: a second mode flag beside `CHORD` for mouse swallowing; raw touchpad input for the gesture.
- `src/main.rs`: `WM_CHORD_UP` release handling (tile case); live swipe handling (`on_swipe`, `goto`); Esc and exit handling in the new mode; the Windows setting restore on Exit.
- `src/expose.rs`: one spread per monitor in swipe mode; window-id keys; `restack` before fold and choose.
- `src/panel.rs`: reuses `hit` for the tile lookup on release.
- `src/touchpad.rs` (new): Raw Input contact parsing, the swipe tracker and phases, the consent prompt, and the Windows setting record and registry access.
- Dependencies: none new expected; Raw Input is in the `windows` crate already in use.
- Specs: `openspec/specs/slot-panel`, `openspec/specs/slot-expose`, and the new `touchpad-panel-gesture`.
