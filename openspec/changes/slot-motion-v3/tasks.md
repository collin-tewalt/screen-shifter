## 1. Spikes (throwaway; only our own windows)

- [x] 1.1 Create a layered topmost window of our own with `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` over part of the screen, `BitBlt` the screen DC, and check whether the window appears in the capture. Record the result in design §4/§5 (chaining and overlay exclusion vs. fallbacks)
- [x] 1.2 Time a full-monitor capture and a full-monitor `UpdateLayeredWindow` on both monitors; note per-frame cost in design §9

## 2. Animation core (`src/anim.rs`)

- [x] 2.1 Pure helpers with unit tests: `ease_out(t)`, `progress(start, now, duration)`, sheet geometry `clip(sheet_x, sheet_w, monitor) -> Option<(screen_x, width, src_x)>` for both directions, chained-remaining-time rule (≤ 80 ms), 1 s hard limit, one-new-sheet-per-monitor-per-frame rule
- [x] 2.2 `capture(monitor_rect) -> Option<Frame>` (32-bpp DIB, alpha forced to 255; None if `BitBlt` fails) and `Sheet::new(frame, dir, scale)` adding the premultiplied trailing-edge shadow
- [x] 2.3 Sheet overlay window (layered, topmost, no-activate, click-through, `WDA_EXCLUDEFROMCAPTURE`), `UpdateLayeredWindow` per frame with clipping; destroy on finish
- [x] 2.4 Animation list on the main thread: `start_sheet(monitor, rect, dir)`, `step(now) -> bool active`, `clear_all()`; chaining: new sheet inserted below existing sheets on that monitor, existing ones shortened (or destroyed per spike 1.1 fallback); re-assert topmost for panels/OSD/ghost; `WDA_EXCLUDEFROMCAPTURE` on panels, OSD, ghost (or hide-capture-reshow fallback)

## 3. Integration (`src/main.rs`, `src/panel.rs`)

- [x] 3.1 Main loop: animation-frame mode (`PeekMessage` drain incl. `WM_QUIT`, step, `DwmFlush` with 1 ms sleep if it returns instantly) when anything is animating, `GetMessage` otherwise; backstop `SetTimer` that calls `clear_all` past the 1 s limit; `clear_all` + `MODAL` flag around `TrackPopupMenu`
- [x] 3.2 `run_switch`: when motion is enabled (and not `MODAL`), capture → sheet → `DwmFlush` → switch; direction from slot numbers before/after; the sheet uses the monitor's rect looked up by name (tile `jump()` currently passes `RECT::default()`); `clear_all()` before `ghost_start`
- [x] 3.3 `relocate` returns the root's new screen rect for the soul's target (workspace→screen for normal windows; target work area for maximized)
- [x] 3.4 Ghost keeps its snapshot; `ghost_fly(to_rect)` animation (scale + glide + fade, 180 ms) for drop-into-tile, drop-onto-other-monitor (target rect from 3.3), and cancel (back to window rect); a new drag ends a flying soul first
- [x] 3.5 Tile pulse: record landing, paint blend over 300 ms, keep frames running while pulsing
- [x] 3.6 `SPI_GETCLIENTAREAANIMATION` check per switch/drop; off → v0.2 behavior
- [x] 3.7 `clear_all` inside `restore_all()` (covers Restore all, Exit, `WM_QUERYENDSESSION`) and on `WM_DISPLAYCHANGE`

## 4. Verify

- [x] 4.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 Manual run-through of every `specs/slot-motion` scenario on both monitors, including fast flicks and the neighbor-monitor check
- [ ] 4.3 Mouse never lags while sheets run; turning Animation effects off restores v0.2 behavior
- [x] 4.4 README: animations, the Animation-effects switch, and that overlays don't appear in screenshots/screen sharing
