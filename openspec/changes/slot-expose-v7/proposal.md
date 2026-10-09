## Why

The panel says *which* windows are in a slot but not what they look like, and the only way to reach a specific window in another slot is to switch and then find it. A Mission Control–style spread fixes both: hover a slot's tile and its windows fan out across that monitor as previews; pick one and you're there, with that window in front.

## What Changes

- **Exposé on hover:** with the panel up, resting the mouse on a slot tile for ~300 ms dims that tile's monitor and lays the slot's windows out as previews (grid above the panel). Moving to another tile (on either monitor) swaps the spread to that slot and monitor immediately; leaving the monitor, releasing `Ctrl+Alt`, or any switch collapses it.
- **Choose a window:** click a preview, or release `Ctrl+Alt` while hovering one: that monitor switches to the slot (if it isn't current) and that window gets focus.
- **Snapshots:** hidden windows can't be drawn live, so when the panel appears, a background thread snapshots the visible windows on every monitor (`PrintWindow` full content, downscaled; hung and minimized windows skipped). A window without a snapshot shows a card with its icon and title.
- **Animated:** previews grow out of their real window positions (current slot) or out of the hovered tile (hidden slot) while the monitor dims; collapse folds them back; choosing expands the chosen preview onto its window while the rest fade. All compositor-driven (DirectComposition, the v6 GPU layer). Hovering a preview highlights it.
- Follows Windows' "Animation effects" (off: appear/disappear without motion). Requires the GPU path; where it's unavailable, hovering does nothing.
- **Drag a preview to a slot** (added after first use): press a preview and move it; it lifts to ghost size and follows the mouse; dropping it on a panel tile (either monitor, "+ New" included) moves that window there, exactly like dragging a real window onto a tile, even if the window is currently hidden in another slot. Dropping anywhere else springs it back.
- Non-goals: live (continuously updating) previews, closing windows from the exposé, keyboard navigation.

## Capabilities

### New Capabilities
- `slot-expose`: hover-to-spread previews of a slot's windows, choosing a window, snapshots, and their animations.

### Modified Capabilities
<!-- none: the panel, switching, and motion requirements are unchanged; the exposé is additive -->

## Impact

- New `src/expose.rs` (state, pure grid layout, hit testing, animations) and `src/snap.rs` (snapshot worker thread and store).
- `gpu.rs`: shared helpers (surface from pixels, generic eased animation, an exposé overlay per monitor below the panel).
- `hooks.rs`: post mouse moves while the panel is up (today only while dragging).
- `main.rs`: hover timer, snapshot kick-off when the panel appears, click/release choose path, collapse on switch/panel hide; `run_switch` gains a "no sheet" option for switches chosen from the exposé.
- `win.rs`: visible frame bounds (`DWMWA_EXTENDED_FRAME_BOUNDS`), focus a specific window.
- `Cargo.toml`: `Win32_Storage_Xps` (PrintWindow, already enabled). No new crates.
