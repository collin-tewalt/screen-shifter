## 1. Spikes (throwaway; only our own windows)

- [ ] 1.1 Create a layered topmost window of our own with `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` over part of the screen, `BitBlt` the screen DC, and check whether the window appears in the capture. Record the result in design §4 (chaining approach vs. fallback)
- [ ] 1.2 Time a full-monitor capture and a full-monitor `UpdateLayeredWindow` on both monitors; note per-frame cost in design §9

## 2. Animation core (`src/anim.rs`)

- [ ] 2.1 Pure helpers with unit tests: `ease_out(t)`, `progress(start, now, duration)`, sheet geometry `clip(sheet_x, sheet_w, monitor) -> Option<(screen_x, width, src_x)>` for both directions, chained-remaining-time rule (≤ 80 ms), 1 s hard limit
- [ ] 2.2 `capture(monitor_rect) -> Frame` (32-bpp DIB, alpha forced to 255) and `Sheet::new(frame, dir, scale)` adding the premultiplied trailing-edge shadow
- [ ] 2.3 Sheet overlay window (layered, topmost, no-activate, click-through, `WDA_EXCLUDEFROMCAPTURE`), `UpdateLayeredWindow` per frame with clipping; destroy on finish
- [ ] 2.4 Animation list on the main thread: `start_sheet(monitor, rect, dir)`, `step(now) -> bool active`, `clear_all()`; chaining: new sheet inserted below existing sheets on that monitor, existing ones shortened (or destroyed per spike 1.1 fallback); re-assert topmost for panels/OSD/ghost

## 3. Integration (`src/main.rs`, `src/panel.rs`)

- [ ] 3.1 Main loop: animation-frame mode (`PeekMessage` drain, step, `DwmFlush`) when anything is animating, `GetMessage` otherwise
- [ ] 3.2 `run_switch`: when motion is enabled, capture → sheet → `DwmFlush` → switch; direction from slot numbers before/after
- [ ] 3.3 Ghost keeps its snapshot; `ghost_fly(to_rect)` animation (scale + glide + fade, 180 ms) for drop-into-tile, drop-onto-other-monitor (target rect from `relocate`), and cancel (back to window rect)
- [ ] 3.4 `relocate` returns the root's new screen rect for the soul's target
- [ ] 3.5 Tile pulse: record landing, paint blend over 300 ms, keep frames running while pulsing
- [ ] 3.6 `SPI_GETCLIENTAREAANIMATION` check per switch/drop; off → v0.2 behavior
- [ ] 3.7 `clear_all` on `WM_DISPLAYCHANGE`, Restore all, Exit

## 4. Verify

- [ ] 4.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 Manual run-through of every `specs/slot-motion` scenario on both monitors, including fast flicks and the neighbor-monitor check
- [ ] 4.3 Mouse never lags while sheets run; turning Animation effects off restores v0.2 behavior
- [ ] 4.4 README: animations and the Animation-effects switch
