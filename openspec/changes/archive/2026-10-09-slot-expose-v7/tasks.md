## 1. Snapshots (`src/snap.rs`)

- [x] 1.1 Worker thread + channel + `WM_SNAPSHOT`; `PrintWindow(PW_RENDERFULLCONTENT)` → crop to extended frame bounds → HALFTONE (+ `SetBrushOrgEx`) downscale (≤ 800 px) → alpha 255; skip `!IsWindow`, hung, minimized, invisible/cloaked; re-check visible after printing; drop a batch while busy
- [x] 1.2 Store (main thread): drop results for windows hidden after their batch started (switch counter); prune at batch start; kick-off when the panel timer fires (not bare chord-down: AltGr)
- [x] 1.3 Measure: snapshot time per window and batch for typical apps (log)

## 2. Exposé core (`src/expose.rs`, `gpu.rs` helpers)

- [x] 2.1 Pure `grid(sizes, area, gap) -> Vec<Rect>` with tests (1, 2, 5, 24 windows; wide/tall mixes; never enlarges; no overlap; inside area)
- [x] 2.2 `gpu.rs`: exposé overlays/surfaces/visuals owned by `Gpu` (dropped by `drop_state`, which also clears exposé state); `ensure(monitor)` (device without a sheet), surface from pixels, solid 1×1 surfaces, eased animation helper reused by sheets, rect animation (offset + scale transform), opacity via effect group, exposé overlay per monitor inserted directly behind the panel window
- [x] 2.3 Cards: GDI-rendered icon + title into a DIB → surface; surfaces kept per window across holds (created when a snapshot arrives, replaced by newer, pruned with the store; re-uploaded lazily after a device reset)
- [x] 2.4 State machine with sets per overlay (removal takes only its set; hide overlay when empty): open / swap / collapse / choose, `expose::reap` on `ANIM_TIMER` (armed on open; kill check includes `expose::any()`), 1 s hard limit, `clear_all`; animation-effects off path

## 3. Input (`main.rs`)

- [x] 3.1 `hooks.rs`: chord-up reason in `wparam` (released / tainted / reset); `HOVER` atomic → post coalesced moves while chord + panel; `on_move` runs ghost/drop only while dragging. Hover: `HOVER_TIMER` 300 ms (killed on chord-up, switch, drag start), current-slot lists from panel open (refreshed after a switch and after a drop), hover check once at panel show, swap on another tile, collapse when the cursor is on another monitor off its tiles; never while dragging
- [x] 3.2 Preview hit-testing for highlight; `on_ldown` checks the exposé before panel and drag code; click (down+up on the same preview) chooses; click on dim area (excluding the panel rect) collapses; tile click still jumps (removing the exposé first)
- [x] 3.3 `WM_CHORD_UP` with reason released chooses the highlighted preview, otherwise collapses (animated; panel hide doesn't remove the exposé); non-exposé switches remove it first; display change / Restore all / Exit clear
- [x] 3.4 `run_switch` options: no sheet, keep exposé, focus this window; `win::focus(entry)` restores minimized; choose target rect (frame, `GetWindowRect` fallback, `rcNormalPosition` when minimized, or fade); `choose()` (current-slot window no longer visible → collapse)

## 3b. Drag previews (added after first use)

- [x] 3b.1 Press + 6 px move → preview drag (`hooks::DRAG`); re-parent onto the all-monitors drag overlay (above panels); grab-point scale center (center + offset in one commit); lift animation; offset follows the cursor; `panel::set_drop`; dragged preview excluded from hit tests, highlight hidden
- [x] 3b.2 `core::move_to_slot(.., in_slot)` with tests (hidden → own monitor's current = Visible; own slot = Rejected); drop: group = root + owned from the spread's source list; `do_move`; fly into tile + pulse / fly to relocated rect / spring back (re-parent into the set); dropped preview ignored; collapse when none left; drop the group from the visible list
- [x] 3b.3 Hidden windows in the move path: show when moved to a current slot; `relocate(.., keep_hidden)` → `SW_HIDE` placement first, + `SetWindowPos` with carried-over overhang for maximized
- [x] 3b.4 Cancel on non-tile drop, chord-up (cancel first, never choose), panel hide, display change / Restore all / Exit; `on_move` routes preview drags; drop hold for hover after a drop; backdrop fades on collapse after a drop

## 4. Verify

- [x] 4.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [x] 4.2 Manual: every scenario in `specs/slot-expose` on both monitors; open/collapse/choose look smooth; previews of overlapping windows are unobstructed; hung or minimized windows show cards — signed off by user in use (2026-10-09)
- [x] 4.3 Manual: wheel/tilt/drag/tile click still behave exactly as v0.6 with the exposé feature present; animation effects off — signed off by user in use (2026-10-09)
- [x] 4.4 README: the exposé, choosing, snapshots stay in memory
