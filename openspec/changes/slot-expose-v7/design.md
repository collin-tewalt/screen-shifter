## Context

v6 gives us a D3D11 + DirectComposition device (`gpu.rs`), per-monitor click-through overlays, compositor animations with exact begin times (`cubic_coeffs`, `begin_time`), and panel tiles with icons. All input while `Ctrl+Alt` is held arrives through the low-level hooks as posted messages (`WM_HOOK_MOVE` coalesced, `LDOWN`/`LUP` swallowed and paired); overlays never take input. Hidden windows (`SW_HIDE`) are not composed by DWM, so DWM thumbnails of them are blank.

## Goals / Non-Goals

**Goals:** previews of any slot's windows on hover, choosing one, smooth open/close/choose motion, no stall of the UI thread or the switch.

**Non-Goals:** live previews, drag between slots from the exposé, keyboard navigation, the exposé without the GPU path.

## Decisions

### 1. Everything is a snapshot (no DWM thumbnails)

Current-slot windows could use live DWM thumbnails, hidden ones can't. Using snapshots for both keeps one rendering path (DirectComposition visuals with compositor animations); a peek lasting a second doesn't need live content. Thumbnails need a destination HWND and per-frame `DwmUpdateThumbnailProperties` to animate, which is the CPU-stepped kind of motion v6 removed.

### 2. Snapshots (`snap.rs`)

- **When:** when the panel timer fires (Ctrl+Alt held alone for 250 ms, i.e. as the panel appears), the main thread lists the visible, non-minimized root windows on every monitor (the current slots) and hands them to the snapshot thread. Not on bare chord-down: on many layouts AltGr *is* Ctrl+Alt, and every AltGr character would cost a capture batch. Hovering takes another 300 ms, so snapshots are normally ready; one that arrives while the exposé is open replaces its card.
- **How (worker thread, one, long-lived):** skip if `!IsWindow`, `IsHungAppWindow`, or not visible / cloaked; `PrintWindow(PW_RENDERFULLCONTENT)` into a 32-bpp DIB of the window rect; check visible-and-not-cloaked **again** afterwards and discard if not (a wheel switch can hide the window mid-batch, and a hidden window prints black); crop to the visible frame (`DWMWA_EXTENDED_FRAME_BOUNDS`; both rects are physical pixels in our per-monitor-v2 process); downscale (`HALFTONE` + `SetBrushOrgEx`) so the longer side is ≤ 800 px; force alpha to 255. Result `(hwnd, w, h, pixels)` goes back over an `mpsc` channel plus `PostMessage(app, WM_SNAPSHOT)`.
- **A stuck target can't block the app:** the main thread never waits on the worker. If the worker is still busy when a new chord-down comes, the new batch is dropped (the previous one is still running). `PrintWindow` on a window that stops responding mid-call can block the worker; that only delays later snapshots (cards are shown meanwhile).
- **Store (main thread):** `HashMap<hwnd, Snap { w, h, pixels, batch }>`. The main thread also drops a result if that window was hidden by a switch after its batch started (each hide bumps a switch counter that results are checked against), so a bad capture never replaces a good older one. Pruned at each batch start to windows that still exist. Each batch is a full refresh (no freshness skip: "drop the batch while busy" already throttles). Memory bound: ≤ 800×800×4 ≈ 2.6 MB per window (RAM, plus the same in VRAM for its surface); typical ~1.3 MB.
- **Minimized windows** and windows without a snapshot get a **card**: dark rounded rect with the app icon (large) and title, rendered with GDI into a DIB and uploaded.

### 3. Who is shown

The hovered tile's slot on that tile's monitor: current slot → the visible list taken when the panel opened (`show_panels` already enumerates it; refreshed for a monitor after a switch on it, and for all monitors after a drop, from the enumeration `do_move` already does), not a fresh `visible_windows_on` per open/swap (tens of ms); hidden slot → `State::slots(monitor)[i]`. Root windows only (owned dialogs belong to their owner), in z-order (topmost first), all of them (the grid shrinks to fit). "+ New" and empty slots: no exposé.

### 4. Layout (pure, tested)

`grid(sizes: &[(f32, f32)], area: Rect, gap: f32) -> Vec<Rect>`: area = the monitor's work area above the panel, inset by a margin. Try every column count `c = 1..=n`, rows `r = ceil(n/c)`; cell = area split into `c×r` with gaps; each window scaled to fit its cell, keeping aspect, never above 1.0 (no enlarging past real size); pick the `c` with the largest total preview area. Rows centered vertically in the area, the last (shorter) row centered horizontally, each preview centered in its cell. Order: reading order = z-order.

### 5. Rendering (`expose.rs` on `gpu.rs`)

One more overlay per monitor (same styles as the sheet overlay: click-through, topmost, excluded from capture), inserted **directly behind the panel's window** (`SetWindowPos(overlay, panel_hwnd)`) each time it's shown, so it's above normal windows and never above the panel. The GPU device is no longer created only by sheets: `gpu::ensure(monitor)` (adapter check + lazy init) serves both. Visual tree:

```
root
 ├─ dim        1×1 black surface, scale transform → monitor size, opacity 0 → 0.45
 ├─ preview i  content = snapshot/card surface (sw×sh)
 │             offset (x, y) + scale transform (w/sw, h/sh): any rect, animatable
 │             effect group opacity
 └─ highlight  accent 1×1 surface scaled to the hovered preview's rect + 4 px (behind it)
```

A rect animation = four eased animations (offset x/y, scale x/y) built by the v6 helpers; opacity via `IDCompositionEffectGroup::SetOpacity`. Everything uses one absolute begin time per transition (`begin_time`).

### 6. Transitions

| Transition | Previews | Dim | Duration |
|---|---|---|---|
| Open (current slot) | from each window's real visible rect to its grid rect | 0 → 0.45 | 250 ms |
| Open (hidden slot) | from the hovered tile's rect (shrunk) to grid rects, opacity 0 → 1 | 0 → 0.45 | 250 ms |
| Collapse | reverse of open (to real rects / back into the tile), then removed | → 0 | 180 ms |
| Swap (another tile) | old set collapses, new set opens, overlapping; when it's the other monitor, each runs on its own overlay | | |
| Choose | chosen preview → its window's real rect; others fade to 0; dim → 0 | | 220 ms |

Removal after a transition rides the existing `ANIM_TIMER` (as v6 sheets): the timer is armed when the exposé opens, `expose::reap(now)` runs on each tick, and its kill check becomes `!anim::any() && !expose::any()`. Hard limit: a closed spread older than 1 s is removed.

**Surfaces live as long as their snapshot**: created when a snapshot arrives and kept across Ctrl+Alt holds, keyed by window (replaced when a newer snapshot for it arrives), so a hidden slot's previews (captured in an earlier hold) are ready to open and sweeping between tiles never re-uploads. Pruned together with the snapshot store; cards are made on first use and kept the same way.

**Ownership:** the exposé overlays, surfaces, and visuals live inside `gpu.rs`'s `Gpu` state next to the sheets, so `drop_state` (display change, `DEVICE_REMOVED`/`RESET`) destroys them too and the exposé state is cleared with it; the snapshot pixels in `snap.rs` survive and are re-uploaded lazily by the rebuilt device.

**Several sets per overlay:** each open creates a *set* (dim + previews + highlight) under the overlay root. Collapse/choose removal takes only that set's visuals; the overlay window is hidden only when no set remains on it (same rule as sheets). So opening again during a 220 ms choose fade on the same monitor just adds a new set. With "Animation effects" off: no motion, the spread appears and disappears at once.

### 7. Hover and input (main thread)

**Chord-up must say why:** `WM_CHORD_UP` today is posted for a real release, for a third key tainting the chord (`Ctrl+Alt+Del`, an AltGr character), and for the stuck-key reset. The hook passes the reason in `wparam` (`RELEASED` / `TAINTED` / `RESET`); only `RELEASED` may choose, the others collapse.

**Moves must reach us:** today `hooks.rs` posts `WM_HOOK_MOVE` only while dragging. A new atomic `HOVER` (set by main when panels show, cleared when they hide) makes the hook also post coalesced moves while the chord is held and the panel is up. `on_move` runs the ghost/drop code only while dragging; otherwise the moves drive the exposé.

- `on_move` (panel up, no drag): `panel::hit(pt)` → slot tile `(monitor, i)`:
  - same as the open exposé → nothing;
  - another tile while an exposé is open → swap immediately;
  - a tile with no exposé open → (re)start `HOVER_TIMER` (300 ms); on fire, if the cursor is still on that tile, open.
- Cursor on a monitor other than the open exposé's (and not on a tile there) → collapse. Over the exposé's own monitor anywhere else (dim area, previews, panel gaps, "+ New") → stays open; over a preview → highlight + label.
- `on_ldown` checks, in order: **exposé** (preview → remember it as pressed; dim area → collapse; either way return: the overlays are click-through, so falling through would hit the real window underneath and start a drag), then panel tiles, then the drag code (as today). `on_lup` on the same preview → choose. The exposé's pressed state is cleared on chord-up like `PRESSED`.
- `WM_CHORD_UP`: if the reason is `RELEASED` and a preview is highlighted → choose it, else collapse (animated); then hide panels. `HOVER_TIMER` is killed on chord-up, on any switch, and when a drag starts, so a late fire can't open a spread.
- When the panel appears, the hover check runs once at the current cursor position, so resting on a tile as the panel shows needs no nudge.
- Dim-area hit test excludes the panel's rect: a tile click on the spread's monitor still jumps. **Hiding the panel does not remove the exposé**: its collapse or choose transition finishes on its own and `ANIM_TIMER` reaps it.
- A switch *not* from the exposé (wheel, tilt, hotkeys, tile click) removes the exposé instantly first, so its overlay never sits under a sheet. The exposé's own choose path skips this (it's already animating out).

### 8. Choosing

`choose(monitor, slot, window)`: start the choose transition; if `slot` isn't current, `run_switch` with **no sheet and no exposé removal, focusing the chosen window** instead of the topmost one (new options; avoids a second window flashing forward first); otherwise just focus it, if it's still visible (a backstop against a stale list): if not, just collapse. `win::focus(entry)`: restore it first if minimized (`SW_RESTORE`), then `SetForegroundWindow` with the existing 0xE8 nudge fallback. The choose target rect is the window's visible frame (`DWMWA_EXTENDED_FRAME_BOUNDS`, which may be meaningless for an `SW_HIDE` window: fall back to `GetWindowRect`); for a minimized window, its restored position (`rcNormalPosition`, workspace → screen), or if unknown the preview just fades. The chosen preview lands on the window as it appears underneath; the overlay is removed after 220 ms.

### 9. Failure and limits

- GPU path unavailable (no device, overlay creation fails): hover does nothing (logged once).
- Monitor change / Restore all / Exit: exposé removed immediately (`expose::clear_all`, alongside `anim::clear_all`). Panel hide: not removed (it finishes its own transition, §7).
- Uploads happen when snapshots arrive (spread over the batch), so opening only builds visuals.

## Risks / Trade-offs

- **Stale snapshots** of hidden windows (taken the last time they were visible with `Ctrl+Alt` pressed): accepted; it's what Mission Control does for minimized windows too.
- **`PrintWindow` quirks:** some apps render black or partially (games, video); still recognizable with the title on hover.
- **Privacy:** snapshots stay in memory only, never written to disk, and our overlays are excluded from capture.
- **More input states** (hover timer, swap, choose) in `main.rs`: kept in one `expose` state machine with pure helpers where possible.
