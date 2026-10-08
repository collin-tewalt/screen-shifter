## Why

Four fixed slots per monitor is arbitrary: most of the time one or two are used and the rest are empty tiles, and occasionally four isn't enough. Slots should exist only while they hold something, with an obvious way to make a new one. Separately, every restart (including each recompile while developing the app) currently throws the whole layout away: everything is shown and every monitor starts over at slot 1.

## What Changes

- **Dynamic slots:** each monitor starts with one slot. The panel shows a **"+ New"** tile after the last slot. Clicking it creates an empty slot at the end and switches to it; dragging a window onto it creates the slot with that window in it (hidden, like any move to an inactive slot).
- **Empty slots disappear when left:** a slot that is empty when you leave it is removed and the slots after it renumber. Inactive slots that become empty in any other way are removed too. Nothing else deletes a slot.
- **Ends still clamp:** the wheel, tilt, and hotkeys stop at the first and last slot; scrolling past the end never creates a slot. Limit of 8 slots per monitor ("+ New" disappears at the limit); the panel shrinks its tiles if they wouldn't fit the monitor's width.
- Indicator reads "N / count" instead of "N / 4".
- **Layout survives restarts (option B, safe):** the state file also records each hidden window's monitor and slot and each monitor's active slot. Exit, panic, and logoff still show every window, exactly as today; but on the next start the app takes back every recorded window that still exists, is still the same app, and is still on its recorded monitor, re-hiding it into its slot and restoring each monitor's active slot. Everything else is shown. Only **Restore all** forgets the layout. A crash no longer needs a flash: re-adopted windows are never shown at startup.
- Non-goals: surviving a reboot or app relaunch (window handles don't survive; would need fuzzy matching by exe/title), naming or reordering slots, deleting a non-empty slot, a settings UI.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `monitor-slots`: slot count is per-monitor and dynamic (start at 1, create, prune, limit 8); clamping wording; indicator shows the count; monitor-set reset goes back to one slot.
- `slot-panel`: tiles follow the monitor's slot count plus a "+ New" tile; click and drop on "+ New".
- `window-recovery`: the state file carries layout; startup re-adopts instead of only restoring; Exit keeps the layout, Restore all clears it.

## Impact

- `core.rs`: `slots` becomes a `Vec`; create/prune logic; switch reports whether anything happened and in which direction (pruning renumbers slots, so before/after numbers no longer tell); state file v2 (v1 still read for recovery); pure `adopt` that rebuilds `State` from a parsed layout.
- `panel.rs`: variable tile count, "+ New" tile, fit-to-width scaling; `hit` distinguishes the "+ New" tile.
- `main.rs`: startup re-adoption, Exit vs Restore all, slide direction from the switch result, OSD count.
- `win.rs`: monitor-of-window check for adoption. No new dependencies.
