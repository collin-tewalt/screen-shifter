## Why

Switching and moving windows work well but happen in a single frame: windows blink out and in. A short, directional motion makes it obvious which way you went and where a window landed, which is most of what makes macOS Spaces feel good. We can't animate other apps' windows, so the motion is an illusion drawn by our own overlays while the real switch happens underneath.

## What Changes

- **Sheet slide-off:** on every switch, the affected monitor briefly shows a frozen picture of its previous contents that slides off to the **left for next** and to the **right for previous** (~220 ms, ease-out, soft shadow on the trailing edge), revealing the live new slot underneath. Confined to that monitor; never takes focus or clicks.
- **Chained sheets:** switching again while a sheet is moving gives the new step its own sheet; the running one speeds up so fast scrolling reads as sheets peeling off back to back.
- **Flying soul:** dropping a dragged window animates its ghost the rest of the way: into the target tile (shrink, glide, fade) with the tile pulsing; onto the other monitor's current slot it glides to where the window appears and fades into it; on cancel it springs back to the window and fades.
- Animations follow Windows' "Animation effects" setting; when it's off, behavior is exactly v0.2.
- Non-goals: true push/side-by-side (needs a picture of the new slot), zoom-to-tile, HDR-correct capture, a settings UI.

## Capabilities

### New Capabilities
- `slot-motion`: the sheet slide-off and its chaining, the flying-soul drop animations and tile pulse, motion settings, and overlay safety.

### Modified Capabilities
<!-- none: motion is additive; switching, moving, and recovery requirements are unchanged -->

## Impact

- New `src/anim.rs` (sheet overlays, frame capture, animation timeline); `panel.rs` gains ghost fly-out and tile pulse; `main.rs` message loop gains an animation-frame mode.
- No new dependencies (GDI, `UpdateLayeredWindow`, `DwmFlush`, `SetWindowDisplayAffinity`).
- Each switch adds one screen capture of the monitor (~5–15 ms) and up to one display frame of latency before the hides start.
