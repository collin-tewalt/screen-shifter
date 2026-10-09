## Context

v6 gives us a D3D11 + DirectComposition device (`gpu.rs`), per-monitor click-through overlays, compositor animations with exact begin times (`cubic_coeffs`, `begin_time`), and panel tiles with icons. All input while `Ctrl+Alt` is held arrives through the low-level hooks as posted messages (`WM_HOOK_MOVE` coalesced, `LDOWN`/`LUP` swallowed and paired); overlays never take input. Hidden windows (`SW_HIDE`) are not composed by DWM, so DWM thumbnails of them are blank.

## Goals / Non-Goals

**Goals:** previews of any slot's windows on hover, choosing one, smooth open/close/choose motion, no stall of the UI thread or the switch.

**Non-Goals:** live previews, keyboard navigation, the exposé without the GPU path.

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

### 10. Polish after first use (2026-10-09)

- **Frosted backdrop:** a plain dim didn't make the previews stand out. DirectComposition can't blur what's behind a window, but we can capture the monitor (the v6 Desktop Duplication path, 5–10 ms) and show that picture under the previews with a Gaussian blur whose standard deviation animates 0 → 28 px while a tint fades to 0.3 (0.45 without a backdrop). Collapse animates the blur back to 0 (the picture becomes the live screen again); choose fades the backdrop out instead (the slot underneath changes). Filter effects need the newer composition device, so `gpu.rs` now creates it with `DCompositionCreateDevice3` (`IDCompositionDesktopDevice`, visuals are `IDCompositionVisual2`); sheets are unaffected. Capture failure → plain dim.
- **Sharp landing on choose:** grid previews use a ≤ 800 px picture (crisp when small); flying that up to full size showed a blurry moment before the real window took over. The worker now also keeps the full-resolution frame (≤ 4096 px), both are uploaded to the GPU as they arrive (CPU copies dropped; nothing stored on the CPU any more), and the chosen preview switches to the full-resolution surface as it starts flying. Cost: GPU memory, ~15 MB per captured 2560×1440 window, dropped when the window closes or the device is lost (then cards until the next capture).

### 11. Dragging a preview to a slot (added after first use)

- **Start:** a left-down on a preview remembers it; a move of more than 6 px (scaled) with the button still down starts a preview drag (`hooks::DRAG` is set so switches are blocked, as during a window drag; moves already arrive while the panel is up). Releasing without moving still chooses (§8).
- **Lift and follow:** the preview's scale transform gets its center at the grab point `c` (content pixels). With that center, the screen position of the grab point is `offset + c` whatever the scale, so the scale animates from the cell size to ghost size (fits 360×240, scaled; 150 ms) while every move just sets `offset = cursor − c` statically: the preview shrinks around the cursor and follows it without fighting its own animation. General rect placement with a center: `offset = rect.x − c + c·s`, `s = rect.w / sw` (center 0 reduces to §5).
- **Drag layer:** the spread's overlay sits behind the panel and covers one monitor, so a dragged preview would vanish over the tiles and at the monitor edge. At lift the preview's visual is re-parented (same device) into a **drag overlay**: one more click-through, capture-excluded window spanning the bounding rect of all monitors, kept topmost above the panels while a drag is on. Its offsets are in that rect's coordinates (cursor − union origin − c). Spring-back re-parents it into its set again (in that monitor's client coordinates) and animates from where it is to its cell; a preview flying into a tile stays on the drag layer and is removed when its fade ends (reaped like closing sets).
- **Targets:** panel tiles only, highlighted with the existing `panel::set_drop`. The spread stays open while dragging, even over the other monitor. The dragged preview is excluded from hit testing and the hover highlight is hidden while dragging.
- **Drop on a tile:** `do_move(group, spread monitor, source slot, tile monitor, target)`, where `group` is the window plus its owned windows from the spread's source list (current slot: the visible list; hidden slot: the slot's entries). **`core::move_to_slot` learns where the window is:** `in_slot: Option<usize>` (None = on screen in `from`'s current slot, as for window drags; Some(i) = hidden in `from`'s slot i). It rejects only a drop onto the window's own slot (`from == to && target == in_slot or current`); a hidden window dropped on any current slot, its own monitor's included, is `Visible`. Results:
  - `Hidden(slot)`: the preview flies into the tile (inset), fading out; the tile pulses.
  - `Relocated(rect)`: flies to the window's new rect on the other monitor and fades.
  - `Nothing` (its own slot, or stale): springs back to its cell.
  A dropped preview stays in the set, faded and ignored by hit testing (no reflow of the others); when no live preview is left, the spread collapses. The moved group is removed from the spread monitor's visible list after the drop (the re-enumeration in `do_move` can still see it before the async hide lands).
- **Moving a hidden window** (new for the move path, which so far only moved visible windows):
  - to a monitor's *current* slot: after relocating, `win::show` the group (it's hidden);
  - across monitors into a *hidden* slot: relocation must not show it. `relocate` gets an explicit `keep_hidden` flag (set only for this case; *not* derived from `IsWindowVisible`, which is still false for a window `win::show` just queued, so a derived rule would re-hide shows) that makes `set_placement_async` use `SW_HIDE` (checked before the minimized/maximized cases; it updates `rcNormalPosition` and keeps the min/max state). A maximized hidden window is also moved with `SetWindowPos` (async, no activation, no show) to the target work area plus its current overhang (`GetWindowRect` − source work area, applied to the target's), since its maximized rect isn't re-derived until it's shown.
- **Cancel:** drop anywhere but a tile, `Ctrl+Alt` released (any reason), or the panel hiding → springs back, `hooks::DRAG` cleared, `set_drop(None)`. On `WM_CHORD_UP` the preview drag is cancelled **first**, then the spread collapses; a release during a drag never chooses.
- `on_move` with a preview drag routes to the drag code (not the window-drag ghost).
- **After a drop the spread stays put:** the cursor ends on the drop tile (maybe on the other monitor), which the §7 hover rules would read as "swap" or "collapse". So a drop sets a *drop hold*: until the cursor is back on the spread's monitor away from the tiles, hovering the drop tile counts as the open spread and being on the other monitor doesn't collapse it. Hovering a *different* tile ends the hold and swaps as usual. This keeps several windows draggable out of one spread.
- After any successful drop, collapsing **fades** the backdrop instead of un-blurring it (the frozen picture still shows the moved window).
- Display change / device loss / Restore all / Exit also cancel a preview drag (clear `hooks::DRAG`, drop state), like they clear the exposé.
- A spring-back during the 150 ms lift snaps the scale to the computed current value (from elapsed time and the easing).
- Panel tiles are refreshed by `do_move` as for any move; the visible lists are refreshed from the same enumeration (§7).

## Risks / Trade-offs

- **Stale snapshots** of hidden windows (taken the last time they were visible with `Ctrl+Alt` pressed): accepted; it's what Mission Control does for minimized windows too.
- **`PrintWindow` quirks:** some apps render black or partially (games, video); still recognizable with the title on hover.
- **Privacy:** snapshots stay in memory only, never written to disk, and our overlays are excluded from capture.
- **More input states** (hover timer, swap, choose) in `main.rs`: kept in one `expose` state machine with pure helpers where possible.
