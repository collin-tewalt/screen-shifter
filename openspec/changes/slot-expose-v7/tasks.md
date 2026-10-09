## 1. Snapshots (`src/snap.rs`)

- [ ] 1.1 Worker thread + channel + `WM_SNAPSHOT`; `PrintWindow(PW_RENDERFULLCONTENT)` → crop to extended frame bounds → HALFTONE (+ `SetBrushOrgEx`) downscale (≤ 800 px) → alpha 255; skip `!IsWindow`, hung, minimized, invisible/cloaked; re-check visible after printing; drop a batch while busy
- [ ] 1.2 Store (main thread): drop results for windows hidden after their batch started (switch counter); prune at batch start; kick-off when the panel timer fires (not bare chord-down: AltGr)
- [ ] 1.3 Measure: snapshot time per window and batch for typical apps (log)

## 2. Exposé core (`src/expose.rs`, `gpu.rs` helpers)

- [ ] 2.1 Pure `grid(sizes, area, gap) -> Vec<Rect>` with tests (1, 2, 5, 24 windows; wide/tall mixes; never enlarges; no overlap; inside area)
- [ ] 2.2 `gpu.rs`: exposé overlays/surfaces/visuals owned by `Gpu` (dropped by `drop_state`, which also clears exposé state); `ensure(monitor)` (device without a sheet), surface from pixels, solid 1×1 surfaces, eased animation helper reused by sheets, rect animation (offset + scale transform), opacity via effect group, exposé overlay per monitor inserted directly behind the panel window
- [ ] 2.3 Cards: GDI-rendered icon + title into a DIB → surface; surfaces kept per window across holds (created when a snapshot arrives, replaced by newer, pruned with the store; re-uploaded lazily after a device reset)
- [ ] 2.4 State machine with sets per overlay (removal takes only its set; hide overlay when empty): open / swap / collapse / choose, `expose::reap` on `ANIM_TIMER` (armed on open; kill check includes `expose::any()`), 1 s hard limit, `clear_all`; animation-effects off path

## 3. Input (`main.rs`)

- [ ] 3.1 `hooks.rs`: chord-up reason in `wparam` (released / tainted / reset); `HOVER` atomic → post coalesced moves while chord + panel; `on_move` runs ghost/drop only while dragging. Hover: `HOVER_TIMER` 300 ms (killed on chord-up, switch, drag start), current-slot lists from panel open (refreshed after a switch and after a drop), hover check once at panel show, swap on another tile, collapse when the cursor is on another monitor off its tiles; never while dragging
- [ ] 3.2 Preview hit-testing for highlight; `on_ldown` checks the exposé before panel and drag code; click (down+up on the same preview) chooses; click on dim area (excluding the panel rect) collapses; tile click still jumps (removing the exposé first)
- [ ] 3.3 `WM_CHORD_UP` with reason released chooses the highlighted preview, otherwise collapses (animated; panel hide doesn't remove the exposé); non-exposé switches remove it first; display change / Restore all / Exit clear
- [ ] 3.4 `run_switch` options: no sheet, keep exposé, focus this window; `win::focus(entry)` restores minimized; choose target rect (frame, `GetWindowRect` fallback, `rcNormalPosition` when minimized, or fade); `choose()` (current-slot window no longer visible → collapse)

## 4. Verify

- [ ] 4.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 Manual: every scenario in `specs/slot-expose` on both monitors; open/collapse/choose look smooth; previews of overlapping windows are unobstructed; hung or minimized windows show cards
- [ ] 4.3 Manual: wheel/tilt/drag/tile click still behave exactly as v0.6 with the exposé feature present; animation effects off
- [ ] 4.4 README: the exposé, choosing, snapshots stay in memory
