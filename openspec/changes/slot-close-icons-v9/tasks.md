## 1. Core: closing a slot

- [ ] 1.1 `State::close_slot(monitor, i, now_ms) -> Option<Closed>` (`Closed { plan, receiver }`) in `src/core.rs` per design §1 (neighbor left, right for the first; fold; active handling; renumber; extends a fresh `last`'s `show` instead of overwriting it; never creates a monitor)
- [ ] 1.2 Tests: middle slot into current, hidden into hidden (order), leftmost folds right, closing the current slot, single slot is None, out of range and unknown monitor are None (no monitor created), prune invariants hold, switch then close then switch within the race window keeps the first plan's `hide` and sees the shown windows, an empty-plan close leaves `last` untouched, state-file round trip

## 2. Panel: the x

- [ ] 2.1 `src/panel.rs`: x rect per tile (top-right, ~20 px × scale), drawn when the monitor has 2+ slots and never on "+ New"; hover highlight state
- [ ] 2.2 `panel::hit_close(pt)` and `panel::tile_hit(pt)` (`hit` with x zones removed; `hit` itself unchanged for hover); tests for both on pure geometry helpers (the panel's queries read thread-local state), including no x on "+ New" and none on a single-slot monitor
- [ ] 2.3 `src/main.rs`: `PRESSED_CLOSE` in `on_ldown` (checked after the `preview_at` and `dim_at` checks and before the `panel::hit` block), confirm on the same x in `on_lup` (its own slot in the early-return chain: PREVIEW_DRAG, DRAGGING, PRESSED_PREVIEW, PRESSED_CLOSE, PRESSED); tile presses and their confirmation use `tile_hit`; clear `PRESSED_CLOSE` wherever `PRESSED` is cleared (panel hide, swipe-mode exit, `WM_DISPLAYCHANGE` rebuild)
- [ ] 2.4 `close_slot_ui(monitor, i)` per design §2: skip while dragging, `stop_hover()`, clear `DROP_HOLD`, reset the x hover, `expose::collapse_monitor` for this monitor only, write-ahead persist, show, append what `win::show` returned to `VISIBLE` for the monitor (de-duplicated by id; a test or assert that the already-visible windows survive), persist, `gpu::switched`, `snap::start_batch` for the shown windows, set `CLOSED_BY_X`, refocus only if the foreground window is no longer visible, `panel::update`, `panel::pulse(monitor, receiver)`; panel stays up
- [ ] 2.5 Exclusions: `release_on_tile`, window drags, `drop_preview` and `set_drop` use `tile_hit`; `release_on_tile` does nothing (and consumes the flag) while `CLOSED_BY_X` is set, which `panel::show`, `hide`, `end_panel_mode` and the first cursor-moving mouse move clear
- [ ] 2.6 `on_hover` updates the x highlight before any early return (`DROP_HOLD`, swipe branches); the x highlight resets in `panel::show`, `hide` and `update`; hover-spread keeps using `hit` (the x is part of the tile)

## 3. Icons

- [ ] 3.1 `src/win.rs`: process image path for a window (limited query, no handle leaks) and the window's AppUserModelID; `Cargo.toml` features, checked with `cargo check` (expect `Win32_System_Com`, `Win32_System_Com_StructuredStorage`, `Win32_UI_Shell_PropertiesSystem`, `Win32_Storage_EnhancedStorage`, possibly `Win32_System_Variant`)
- [ ] 3.2 `src/icons.rs`: worker thread (COM STA, `CoUninitialize` on exit) resolving AUMID then image path, `IShellItemImageFactory::GetImage` always at 256 px, real size read from the bitmap, alpha premultiplication verified empirically on a known icon, `HBITMAP` deleted; fallback draws the `HICON` the UI thread resolves with `win::icon_of` when it builds the request from each slot's entries (via `GetIconInfo` + `GetDIBits`, mask applied); bounded request queue, `IsHungAppWindow` windows skipped; session cache capped at 64 keys; results carry `(id, key, picture)` even on cache hits; request at panel open for every window in every slot; `expose::icons_arrived` called from the `WM_SNAPSHOT` handler
- [ ] 3.3 `src/expose.rs`: one shared GPU surface per key (box-filtered to the largest drawn size, map capped at 64 and cleared with the GPU reset; sets keep their own references); one icon container per set above all previews via `stack()`, each icon with its own opacity group at its cell's bottom-left (size per design §5, omitted under 120 px); `Tx::new` takes a delay via `begin_time(.., delay_ms)`; `Set` records the open time, fade-in starts at its end, late icons fade in at once; container fades out before the fold on collapse/choose; the dragged preview's icon is hidden and restored on `Land::Back`, which also re-raises the container above the re-added preview; removed with a `gone` preview; no fades with motion off
- [ ] 3.4 Pure parts (icon size/inset from a cell, omission threshold) as a tested function

## 4. Verify and finish

- [ ] 4.1 `cargo test` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 README: the x on panel tiles and the preview icons; AGENTS.md code map: `src/icons.rs`
- [ ] 4.3 Owner manual test: x on each kind of tile, folding left/right/into the current slot, closing the current slot, no x on a single slot, release and drop over an x, icons for Claude, Cursor, a UWP app, and a PWA, and a hung app
- [ ] 4.4 **Before finishing this change (before bumping the version and archiving): ASK THE OWNER whether to add a backing plate behind the preview icons** (decided against for now because dark icons on dark windows are accepted; revisit after living with it)
