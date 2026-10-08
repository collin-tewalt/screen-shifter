## Context

v0.2 (`openspec/specs/monitor-slots`, `slot-panel`, `window-recovery`): single main thread running the message loop and all switching (`run_switch`, `do_move`); a time-critical hook thread only posts messages; overlays (`panel.rs`: panels, ghost; OSD) are layered, topmost, no-activate, click-through windows in our own process, so they vanish if the process dies. Hide/show is `ShowWindowAsync`. The user runs two side-by-side OLED monitors with HDR off.

## Goals / Non-Goals

**Goals:**
- Directional sheet slide-off on every switch, per monitor, with chaining for fast scrolling.
- Flying-soul drop, spring-back cancel, tile pulse.
- Never stall switching or input; an animation can never leave a frozen picture on screen.

**Non-Goals:**
- Push (old and new side by side), zoom-to-tile, HDR-correct capture, configurable timing, a settings UI.

## Decisions

### 1. The illusion
```
run_switch (animations on):
  1. capture the monitor rect           ─ BitBlt from the screen DC (~5–15 ms)
  2. show a sheet overlay with it        ─ covers the monitor exactly
  3. DwmFlush()                          ─ the sheet is composited before anything changes
  4. the usual switch: write-ahead, hide, show, persist, focus
  5. sheet slides off: next → left, prev → right (tile jumps: higher slot = next)
```
Step 3 matters: the hides are async in other apps' threads and can land within a millisecond; without waiting for one composition, the half-switched desktop could flash before the sheet appears.

### 2. Sheet overlay
One overlay window per sheet: `WS_POPUP`, `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT`, `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` (see §4). Content is a 32-bpp top-down DIB section: the captured frame (alpha forced to 255) plus a 24 px (DPI-scaled) shadow strip on the trailing edge — the right side when sliding left, the left side when sliding right — a black gradient from ~35% to 0% alpha, premultiplied. Drawn with `UpdateLayeredWindow(..., ULW_ALPHA)` and `AC_SRC_ALPHA`.
**Clipping:** each frame computes the sheet's screen x, intersects the sheet with the monitor rect, and passes the visible width as the window size and the clipped-off amount as `pptSrc`, so a sheet never draws onto the neighboring monitor.
Our overlays are excluded by the window filter (own pid), so they never become slot contents.

### 3. Timeline and the frame loop
Duration 220 ms, ease-out cubic (`1 - (1-t)^3`). Animations live in a main-thread list (`anim.rs`). The main loop becomes:
```
loop {
  if animations active: drain messages with PeekMessage/Dispatch, step every animation
                        (UpdateLayeredWindow / SetWindowPos), DwmFlush() to pace to vsync
  else:                 GetMessage / Dispatch (blocks as today)
}
```
Running at the top of the loop (never nested inside a wndproc) avoids re-entrancy. A switch arriving mid-animation is handled within one frame. `DwmFlush` paces frames to the display refresh without timers; if it returns in under 1 ms (nothing to compose), the loop sleeps 1 ms so it can't spin a core. The `PeekMessage` path exits on `WM_QUIT` like `GetMessage`.
**Modal loops and blocking calls:** frames only advance at the top of our loop, so anything that runs its own loop or blocks would freeze a sheet mid-screen. Therefore: `clear_all()` runs before `TrackPopupMenu` (tray menu) and a `MODAL` flag disables motion while it is open; `ghost_start` calls `PrintWindow`, which can take 50–200 ms on Chrome/Electron, so `clear_all()` runs before it (a drag started mid-slide ends the slide).
**Hard limit:** every animation is destroyed 1 s after it starts, whatever its state. Enforced twice: in `step`, and by a backstop `SetTimer` on the app window (`WM_TIMER` is dispatched even inside modal loops) that calls `clear_all` once the oldest animation is over 1 s old. `restore_all()` calls `clear_all()` first, which covers Restore all, Exit, and `WM_QUERYENDSESSION`; `WM_DISPLAYCHANGE` calls it too.

### 4. Chaining
**One new sheet per monitor per frame.** GDI `BitBlt` from the screen reads the last *composited* frame. Several switches handled before the next composition (`on_wheel` with a 240+ delta, or several queued wheel messages drained in one frame) would all capture the same stale picture. So if a monitor's newest sheet was created since the last frame-loop `DwmFlush`, a further switch on that monitor in the same frame happens with no new sheet (the sheet above already hides it).
When a switch happens on a monitor that still has a sheet moving (from an earlier frame):
1. capture the monitor — the existing sheet has `WDA_EXCLUDEFROMCAPTURE`, so the capture sees the live slot underneath (spike 1.1 confirms this holds for GDI `BitBlt`);
2. create the new sheet **directly below** the existing sheets in z-order (still above every app window);
3. switch underneath;
4. the existing sheets' remaining time shrinks to at most 80 ms; the new sheet starts its full slide immediately.
Visually: sheets peel off back to back, the top one fastest.
*Fallback if spike 1.1 fails:* finish existing sheets instantly (destroy), `DwmFlush`, then capture (the flush is needed because `BitBlt` reads the last composited frame). Fast scrolling then shows small pops instead of a clean stack, but nothing is ever wrong on screen.

### 5. Z-order with other overlays, and keeping them out of captures
All our overlays (panels, OSD, ghost, sheets) get `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`, so a captured frame never contains the panel or indicator (otherwise a stale copy of the panel would slide out from under the live one on nearly every wheel switch). *If spike 1.1 shows GDI capture ignores the affinity:* hide panels/OSD/ghost, `DwmFlush`, capture, re-show them (one extra frame; the panel blinks). Side effect of the affinity: our overlays don't appear in the user's own screenshots or screen shares (documented).
Sheets are created topmost after the other overlays, so after creating a sheet, re-assert `HWND_TOPMOST` on the visible panels, the OSD, and the ghost (`SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE`). The panel stays readable while the sheet slides under it.

### 6. Flying soul
The ghost already holds a scaled snapshot. `panel.rs` keeps that bitmap after `ghost_start`, and a ghost animation re-scales it per frame (`StretchBlt` into a small DIB) with constant alpha, then `UpdateLayeredWindow`. ~180 ms, ease-out.
| Outcome | From → to | Alpha |
|---|---|---|
| Drop on a hidden slot | ghost rect → target tile rect, inset (shrinks) | 150 → 0 |
| Drop on the other monitor's current slot | ghost rect → the window's new screen rect (from `relocate`'s computed target; grows) | 150 → 0 |
| Cancel (drop elsewhere / chord released), or drop on its own monitor's current tile (no-op move) | ghost rect → the root window's current `GetWindowRect` | 150 → 0 |
The real hide/relocate happens at drop time exactly as in v0.2; the soul only plays on top. Panels may hide (chord released) while a soul is still flying; it simply finishes. A new drag starting while a soul is flying ends the soul first (one ghost window). The "other monitor" target rect comes from `relocate`: for normal windows, its new `rcNormalPosition` converted from workspace to screen coordinates (add the primary work area origin); for maximized windows, the target monitor's work area.

### 7. Tile pulse
When a window lands in a tile, `panel.rs` records `(monitor, slot, start)`; the panel paints that tile's background blended toward the accent color, fading over 300 ms, and the frame loop invalidates panels while a pulse is active.

### 8. Motion settings
Checked on each switch/drop: `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)`. Off → no sheet, no soul (ghost hides instantly), no pulse — exactly v0.2. No tray toggle (YAGNI; Windows' setting is the switch).

### 9. Cost and latency
Per switch: one capture (~5–15 ms at 1440p–4K) + ≤ 1 frame wait (step 3) before hides start. Per frame: one `UpdateLayeredWindow` of the visible part of the sheet (a few ms at 4K), done on the main thread between message dispatches. The hook thread is unaffected, so the mouse never lags.

## Risks / Trade-offs

- [DRM video / hardware overlays capture as black] → a black rectangle slides away for 220 ms. Harmless; accepted.
- [HDR on] → the captured frame is tone-mapped and slightly dimmer than live; accepted (user has HDR off); documented.
- [`UpdateLayeredWindow` of a full 4K sheet may miss frames on slow GPUs] → frames are paced by `DwmFlush`; a dropped frame only shortens the motion. DirectComposition is the upgrade path.
- [Frame captured before the previous slot finished painting during chaining] → the sheet may show a partly drawn window for a few frames. Accepted.
- [An animation never ends due to a bug] → 1 s hard limit destroys it (in `step` and via the backstop timer).
- [Capture fails (secure desktop, UAC, lock screen)] → `BitBlt` failure skips the sheet; the switch happens without motion.
- [Ctrl+Alt+click during a slide] → grabs the live window under the picture (clicks pass through sheets), not the one drawn on it. Accepted; documented.

## Spike Findings

- 1.1: `WDA_EXCLUDEFROMCAPTURE` **is honored by GDI `BitBlt`** from the screen DC, for both normal and layered windows (our pixel read back the desktop, not the overlay). Chaining and overlay exclusion use the main path; the fallbacks are not needed.
- 1.2 (2560×1440 ×2, dev machine): full-monitor `BitBlt` capture ≈ 27–30 ms; forcing alpha on the frame ≈ 1 ms; `UpdateLayeredWindow` of a half-monitor sheet ≈ 3.5 ms per frame. So a switch gains ~30 ms + ≤ 1 frame before hides start; per-frame cost is comfortably under a 60 Hz budget.

## Open Questions

- Is 220 ms the right feel? Tune after use.
