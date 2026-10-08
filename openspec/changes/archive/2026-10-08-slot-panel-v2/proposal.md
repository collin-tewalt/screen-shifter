## Why

v0.1 works, but switching needs a two-handed `Ctrl+Alt+←/→` chord that pulls a hand off the mouse, and there is no way to see what's in a slot or to put a window into a specific slot. Turning "hold Ctrl+Alt" into a mouse-driven slot controller fixes both without leaving the mouse.

## What Changes

- Holding `Ctrl+Alt` (alone, ~250 ms) shows a slot panel on **every** monitor: 4 tiles per monitor listing the windows in each slot, current slot highlighted. Releasing hides it.
- While `Ctrl+Alt` is held, on the monitor under the cursor: wheel up / tilt right = next slot, wheel down / tilt left = previous slot. Those events are not delivered to applications.
- Clicking a tile jumps straight to that slot on that tile's monitor.
- Pressing on a window and dragging shows a translucent, size-capped "soul" of the window following the cursor; dropping it on any tile (either monitor) moves the window into that slot. Dropping elsewhere or releasing `Ctrl+Alt` cancels.
- The "N / 4" indicator is not shown while the panel is visible (the panel's highlight replaces it). `Ctrl+Alt+←/→` keeps working unchanged.
- Non-goals: tile-to-tile dragging, window previews in tiles, app icons, configurable keys/direction.

## Capabilities

### New Capabilities
- `slot-panel`: the Ctrl+Alt panel, mouse wheel/tilt switching, tile click to jump, and drag-to-assign with the ghost preview.

### Modified Capabilities
- `monitor-slots`: adds moving a window into any slot on any monitor; the slot indicator is suppressed while the panel is visible.

## Impact

- New low-level keyboard and mouse hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`) on a dedicated thread.
- New windows: one panel per monitor, one ghost window (DWM thumbnail).
- `core.rs` gains `switch_to` and `move_to_slot`; `win.rs` gains window titles, relocation between monitors, and hit-testing helpers.
- No new crate dependencies expected (uses more `windows` features: DWM thumbnails, hooks).
