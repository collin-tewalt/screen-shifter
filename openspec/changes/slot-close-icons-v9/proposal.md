## Why

Slots can be created (drag to "+ New", or jump to it) but never removed unless they empty out, so a monitor can be left with a slot that is only there because something is hidden in it. And the exposé's previews are hard to tell apart when many windows are dark-mode apps full of text (Claude, Cursor, terminals): a recognizable app icon on each preview fixes that at a glance.

## What Changes

- Each slot tile in the Ctrl+Alt panel shows a small x. Clicking it closes that slot: its windows fold into the neighboring slot on the left (on the right for the leftmost slot), and the slots after it renumber. Windows are never closed, hidden newly, or lost; folding into the monitor's current slot shows them on screen.
- Closing the monitor's current slot keeps its windows on screen and makes the neighbor the current slot, so the neighbor's hidden windows join them.
- A monitor with a single slot shows no x on it: every monitor always keeps at least one slot. "+ New" never has an x.
- The x is on panel tiles only (not on the exposé spread). Releasing Ctrl+Alt over an x does nothing, and an x is not a drop target for dragged windows or previews.
- Each exposé preview shows the application's icon at its bottom-left, fetched from the window's package or executable at about the size it is drawn (up to 256 px) instead of the 16 to 32 px `WM_GETICON` icon, with that icon as the fallback. No backing plate for now (dark icons on dark windows are accepted); the owner is asked about a plate again before this change is finished.
- **BREAKING**: none.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities
- `monitor-slots`: a slot can be closed, folding its windows into a neighbor.
- `slot-panel`: tiles carry an x that closes their slot; where the x is not shown, and how it is excluded from release and drop.
- `slot-expose`: previews carry a high-resolution app icon.

## Impact

- `src/core.rs`: `State::close_slot` (pure, tested) beside `switch_to`.
- `src/main.rs`: x press and release handling in `on_ldown`/`on_lup`, `close_slot_ui` wiring (write-ahead persist, show, `VISIBLE` merge, panel update, stale hover/drop-hold/spread state), `tile_hit` in click, release and drop paths, WM_SNAPSHOT also drains icons.
- `src/panel.rs`: draws the x, hover highlight, `hit_close`, `tile_hit`.
- `src/icons.rs` (new): resolves a window's package or executable, fetches the icon on a worker thread, caches it.
- `src/expose.rs`: one icon visual per preview, placement, fade with the spread, hide while dragged.
- `src/win.rs`: the process image path and window AppUserModelID helpers.
- Dependencies: none new; `windows` features `Win32_UI_Shell_PropertiesSystem` and `Win32_System_Com` may need adding to `Cargo.toml`.
- Specs: `openspec/specs/monitor-slots`, `slot-panel`, `slot-expose`.
