## 1. Core (`src/core.rs`, unit-tested)

- [ ] 1.1 `slots: Vec<Vec<Entry>>` (start with one), `MAX_SLOTS = 8`; replace `SLOTS` uses; `slots(monitor)` returns the real count
- [ ] 1.2 `prune` (drop empty inactive slots, remap `active`), run on every monitor at the end of `switch_to` and `move_to_slot`; tests: leaving empty slot, slot emptied by capture elsewhere, slot emptied by move, active empty slot kept
- [ ] 1.3 `Target { Slot(i), New }` for `switch_to` and `move_to_slot`: `New` creates (respect `MAX_SLOTS`), `Slot(i >= len)` rejected; `move_to_slot -> MoveResult { Rejected, Hidden(final_index), Visible }` (same-monitor active-slot move stays a pure no-op → `Rejected`); `switch(Dir)` still clamps at `len - 1`; tests: create+switch, create by move (hidden, active unchanged), limit, clamp, stale index rejected
- [ ] 1.4 `switch`/`switch_to` return `Option<Plan>` (`None` = no-op); test: leaving empty slot 2 for 3 returns `Some` though the number stays 2
- [ ] 1.5 State file v2 (header `v2 <boot>`; boot passed in, so core stays pure): `serialize(&State, extra_show)` and `parse -> Layout` (`m`/`w` lines, v1 still read as show-all, bad lines reported); write-ahead form (to-be-shown windows tagged with the monitor's new active slot); round-trip and bad-input tests
- [ ] 1.6 Pure `adopt(layout, names, keep: impl Fn(&monitor, &Entry) -> Option<Entry>) -> (State, show)`: entries in an active slot, on a missing monitor, or rejected go to `show`; duplicates keep the first; `w` lines without `m` get active 0; slots compacted by `prune`; active remapped (clamped); `monitor_set` = current names, sorted; boot times > 30 s apart → adopt nothing; slot index ≥ `MAX_SLOTS` → show; disconnected-monitor windows tagged slot 0; plus the startup write-ahead text (new State + show set tagged with the **new** active slot, or under their old monitor name if it's gone); tests incl. "window closed while off", "slot == active → show", duplicates, sorted monitor_set, other boot, and parse(write-ahead) → adopt again yields the same State with the show set shown
- [ ] 1.7 Extend the randomized never-lose-a-window simulation with create (`Target::New`), stale targets, pruning, and a serialize → parse → adopt round trip mid-run

## 2. Shell (`win.rs`, `main.rs`)

- [ ] 2.1 `win::monitor_name_of(hwnd)` using the classification rule (overlap; minimized → restored position), applied to the root owner; `win::live_entry(hwnd)` re-reading pid/class/iconic; `win::boot_time()` (now − uptime)
- [ ] 2.2 `write_state` takes the State (+ write-ahead extras); `run_switch` takes the direction from its caller and uses `Option<Plan>` for no-ops; `do_move` handles `MoveResult::Rejected` as nothing; OSD "N / count" (`OSD_SLOT` carries the count)
- [ ] 2.3 Startup, **after** app window, hotkeys, tray, and hooks succeed (the mutex check stays first and its return touches nothing; every later early return runs the file-only show routine, keeping the file): read file → validate per root-owner group (IsWindow, pid+class, monitor connected, root on recorded monitor) → `adopt` → write-ahead (State + tagged show set) → `show(show set)` → `hide(adopted)` → `focus_after_switch(&[], &adopted)` only if `has_focus(&adopted)` → write exact State; replaces `restore_from_file` + `reset_if_monitors_changed` at startup
- [ ] 2.4 `show_all_keep_layout()` = `State::restore_all()` + show, no file write, used by Exit and `WM_QUERYENDSESSION`; `restore_from_file` becomes `show_from_file` (no clear), used by the panic hook and startup-failure returns, with no remaining caller of a clearing variant; `panic = "abort"` in both Cargo profiles; Restore all and monitor-set change: show + clear (unchanged)

## 3. Panel (`src/panel.rs`)

- [ ] 3.1 Tile count from `titles.len()`, plus "+ New" tile (dashed outline, "+ New") when `< MAX_SLOTS`; `hit` returns `Tile::Slot(i)` / `Tile::New`; drop highlight on "+ New"
- [ ] 3.2 Shrink tiles and gaps uniformly when the panel would exceed 90% of the monitor width
- [ ] 3.3 `update` resizes a panel whose tile count changed; after a switch or move, if any other monitor's slot count changed, `panel::update(panel_data())`; drop pulse/fly target uses `MoveResult::Hidden(index)` and is read after the update

## 4. Verify

- [ ] 4.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 Manual: every scenario in the changed specs on both monitors (create by click and drop, prune on leave, limit at 8, panel fit, wheel clamps at last slot)
- [ ] 4.3 Manual restart loop: Exit → start (layout back, adopted windows re-hidden); kill the process → start (no flash, layout back); move a window across monitors while off; Restore all → restart takes nothing back
- [ ] 4.4 README: dynamic slots, "+ New", restart keeps the layout, Restore all forgets it, Restore all before running an older build
- [ ] 4.5 Manual: hide windows in slots, Exit, have another app own `Ctrl+Alt+Left`, start → error shown, all those windows visible; launch a second instance while one runs → nothing changes
