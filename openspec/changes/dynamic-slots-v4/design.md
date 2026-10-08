## Context

`core::State` keeps, per monitor, `active` and `slots: [Vec<Entry>; 4]`; the active slot's vec is always empty because its windows are on screen. `main::run_switch` detects a no-op by comparing `active` before and after, and picks the slide direction the same way. The state file (`hidden.txt`, `v1`) lists only *which* windows are hidden; startup, panic, Exit, logoff, and monitor-set changes all show everything in it and clear it.

## Goals / Non-Goals

**Goals:** per-monitor slot lists that grow on request and shrink on their own; a "+ New" tile; the layout surviving an app restart without ever leaving windows hidden while the app isn't running.

**Non-Goals:** surviving reboot/relaunch of the *other* apps (handles change), naming/reordering slots, deleting non-empty slots, configurable limit.

## Decisions

### 1. Model: `Vec` slots, one invariant

`slots: Vec<Vec<Entry>>`, never empty, `active < slots.len() <= MAX_SLOTS (8)`. A new monitor gets one slot. **Invariant after every `State` mutation: no inactive slot is empty.** One private `prune(monitor)` enforces it (drop empty inactive slots, recompute `active`'s index), called at the end of `switch_to` and `move_to_slot` for every monitor touched (the one-slot-per-window `retain` can empty slots on *any* monitor, so prune all monitors there).

Consequences, all wanted:
- Leaving an empty slot removes it (it's inactive once you've left).
- A hidden slot whose windows were all captured elsewhere disappears.
- A slot whose windows all closed while hidden survives until visited (its entries aren't known to be dead until `show` skips them); once left empty, it goes. No liveness polling.

### 2. Targets: `Slot(i)` or `New`, resolved at call time

`core::Target { Slot(usize), New }`. `switch_to(monitor, Target)` and `move_to_slot(.., Target)`:
- `New` (if `len < MAX_SLOTS`) pushes an empty slot and uses its index. A move into it is always hidden (a new slot is never active).
- `Slot(i)` with `i >= len` is **rejected** (stale index from a panel drawn before a prune). It never means "create".

`panel::hit` returns `Tile::Slot(i)` / `Tile::New`, so a "+ New" tile always means "new at the end" however stale the panel is; a stale slot tile at worst hits a neighbor or is rejected. Panels are refreshed whenever counts change (§4), so the window for staleness is a few ms.

`move_to_slot` returns `core::MoveResult::{Rejected, Hidden(final_index), Visible}` (main.rs already has a `Moved` enum) instead of `bool`; `do_move` treats `Rejected` as nothing (no relocate, ghost goes home). `Hidden` carries the slot's index after pruning, which `on_lup` needs for the pulse and fly target (a `New` target's index isn't known to the caller, and prune can renumber a `Slot(i)`). A move to the from-monitor's own active slot stays a pure no-op (`Rejected`: no `retain`, no `last` change), as today. Today's `false` conflates "rejected" with "lands in the visible slot", which becomes a bug once rejection is possible.

`switch(Dir::Next)` still clamps at `len - 1`: only `New` creates.

Clicking "+ New" while the current slot is empty: the empty slot is pruned as it's left and a new one appended; net count unchanged. Fine.

### 3. No-op detection and direction

With pruning, "slot 2 → slot 2" can be a real switch (leave empty slot 2 for 3, which renumbers to 2). So `switch`/`switch_to` return `Option<Plan>` (`None` = nothing happened) and `run_switch` uses that instead of `before == after`. The caller already knows the direction: `Dir` for wheel/tilt/hotkeys; for a jump, `Next` iff `New` or `Slot(i)` with `i >` the pre-switch active index. `run_switch` takes the direction alongside `pick`. The OSD/panel show post-prune numbers.

### 4. Indicator and panel

- OSD: "N / count", count after pruning (`OSD_SLOT` becomes `(n, count)`; `osd_proc` no longer reads `SLOTS`).
- After a switch or move, the switched/target monitor's panel is rebuilt from data in hand (as today). If **any other monitor's slot count changed** (the one-slot `retain` + prune can remove slots anywhere; rare), just `panel::update(panel_data())` for all: one enumeration, only in that case.
- Drop on "+ New": the fly-in target rect is read **after** `panel::update` resizes the panel, so the ghost lands on the new tile.
- Panel: `count` slot tiles, then a "+ New" tile (dashed outline, "+ New" text, no titles) if `count < MAX_SLOTS`. Tiles keep their size unless `tiles × (w + gap) + gap` exceeds 90% of the monitor width, in which case tile size and gap scale down uniformly to fit (at 2560 px / 125%, 8 tiles are ~2335 px: fits; only higher scaling shrinks).
- `hit` returns slot `0..=count` (`count` = "+ New"); drop highlight works on "+ New" too. `PanelData.titles.len()` *is* the count, so no new field.
- Panel updates after a switch replace the panel's tile count; a panel whose size changes is resized in place.

### 5. State file v2 carries the layout

```
v2 <boot>
m <monitor> <active>
w <monitor> <slot> <hwnd> <pid> <iconic> <class>
```

`<boot>` is the boot time in ms since the Unix epoch (now − uptime): the file now outlives Exit and logoff, so long-lived pid+class pairs (Explorer, Chrome) make handle reuse plausible over days. Boot times more than 30 s apart → a different boot → nothing is adopted, everything valid is shown. (Tolerance, not rounding: clock corrections and rounding boundaries must not lose the layout.) (Also cleanly excludes reboot survival, a non-goal.) One `m` line per monitor with state; one `w` line per recorded window, topmost-first within a slot. Monitor names are GDI device names (`\\.\DISPLAY1`), no spaces; class stays last so it may contain spaces. `serialize(&State)` / `parse -> Layout { active: Vec<(monitor, usize)>, windows: Vec<(monitor, usize, Entry)> }`. `parse` still accepts `v1` (every entry becomes "show", no layout) so a crash under v0.3 is still recovered by v0.4.

**Write-ahead with layout:** before acting, the file is the *new* state's layout plus, for each window in the old hidden set but not the new one (i.e. about to be shown), a `w` line with its monitor's new **active** slot. After acting, exactly the new state. Recovery reads "a window recorded in its monitor's active slot" as "belongs on screen" (§6), so a crash mid-switch shows the to-be-shown windows and re-hides the to-be-hidden ones, both correct.

### 6. Startup: adopt, don't just restore

```
read file ──► for each w line:
                live? (IsWindow, pid, class match)      no ─► skip
                monitor still connected?                no ─► show
                window's monitor == recorded monitor?   no ─► show   (user moved it while we were off)
                slot == that monitor's active slot?     yes ─► show
                otherwise                                ─► ADOPT
core::adopt(layout, adopted) ─► State (pruned; monitor_set = current names)
write_state(new State + show set tagged with its monitor's NEW active slot)   ◄ write-ahead
show(the "show" set); hide(adopted); focus_after_switch(&[], &adopted)
write_state(new State)          (exact)
```

- **Adoption runs last**, after the app window, hotkeys, tray, and hooks are all up. The single-instance mutex check stays first and its early return touches nothing (no file read, no show: the running instance owns those windows). Any *later* early exit (e.g. a hotkey owned by another app) first runs the panic hook's "show everything the file lists, keep the file" routine, so a failed start never leaves windows hidden with no app running, whether they were hidden by a crash or would have been re-hidden by adoption.
- **Startup is a switch and uses the same write-ahead shape.** The show set can contain windows that are hidden right now (recorded on a now-disconnected monitor; or tagged "active" by a switch write-ahead that crashed before showing them). If the file held only the new State, a crash before `show` lands would leave them hidden with no record. Tagging uses the *post-adoption* active index (pruning can move it; a stale index could get the window adopted instead of shown). Windows of a disconnected monitor are tagged under their old monitor name with no `m` line → next start sees "monitor not connected" → show. Every path through §6 maps a show-set window back to "show".
- Focus: re-hiding an adopted window that has focus would strand focus on a hidden window; if `has_focus(&adopted)`, `focus_after_switch` moves it, as after any switch. Otherwise focus is left alone.
- Owner groups: the decision (adopt/show) is made per root-owner group (`GA_ROOTOWNER`), using the root's monitor, so a dialog never ends up hidden while its owner is shown or vice versa.
- Windows of a disconnected monitor are tagged slot 0 in the write-ahead, so a monitor reconnected before the next start shows them (its active defaults to 0) rather than adopting them.
- `monitor_set` is stored sorted (as `reset_if_monitors_changed` compares sorted names), or the first `WM_DISPLAYCHANGE` would wipe the restored layout.
- `adopt` is defensive about the file: duplicate handles keep the first occurrence; entries with a slot index ≥ `MAX_SLOTS` go to the show set; `w` lines for a monitor with no `m` line get active 0 and as many slots as the indexes need (then pruned); an `m` active index beyond the slots that survive maps to the last slot.
- Monitors still connected keep their layout even if another recorded monitor is gone (unlike the runtime monitor-set reset, which can't know the user's intent mid-session; at startup the missing monitor's windows are simply shown).
- A hung window (`IsHungAppWindow`) can't be hidden, so an adopted hung window stays visible while recorded in its slot; same exposure as at runtime, resolved by the next capture.
- `panic = "abort"` in both profiles: a panic on the hooks thread currently runs the show-from-file hook but leaves the process running with State out of sync. With the file now outliving the process, abort makes the next start the single recovery path.
- Adopted entries are re-read live (current `iconic`), not taken from the file: the user may have minimized/restored them while the app was off.
- Hiding an already-hidden window (after a crash) is a no-op, so a crash restart never flashes. After a clean Exit the adopted windows were visible and disappear again: that's option B's price.
- "Window's monitor" uses the same rule as classification (most overlap; minimized → restored position), via the existing `monitor_of`.
- `core::adopt` is pure and unit-tested: slot indexes compacted after pruning, active index mapped through pruning, monitors without an `m` line default to one slot.

### 7. Who forgets the layout

| Event | Shows everything | State file |
|---|---|---|
| Exit, logoff (`WM_QUERYENDSESSION`) | yes | **kept** (layout for next start) |
| Panic hook | yes (from file) | kept |
| Restore all (tray) | yes | cleared |
| Monitor set changed | yes | cleared |
| Startup | the "show" set (§6) | rewritten from adopted state |

Exit/logoff call a `show_all_keep_layout()` = `State::restore_all()` (so State is back to one slot per monitor, as the logoff-cancelled scenario requires) + `show`, without writing the file; `restore_all()` keeps today's meaning (show + clear). The panic hook shows what the file lists without clearing it (the next start re-adopts). Logoff then cancelled: State is reset (one slot, as today) but the file still holds the old layout until the next switch overwrites it; acceptable.

## Risks / Trade-offs

- **Exit no longer means "forget"**: a user who exits, rearranges by hand, and restarts gets windows re-hidden. Mitigated by the "still on its recorded monitor" check and by Restore all clearing the layout. → README note.
- **Handle reuse**: unchanged guard (pid + class), now applied to hiding as well as showing; a reused handle that happens to match pid+class is the same app's new window and gets hidden in the old slot — rare, recoverable, same exposure as v1's show path.
- **Pruning renumbers under the user**: slot 3 becomes slot 2 after you leave an empty slot 2. Inherent to the model; the panel always shows current numbers.
- **Cross-monitor move, then crash before the async relocation lands**: the window is recorded on B but still physically on A, so the next start shows it (wrong-monitor rule). Not lost, just not kept. Tolerated.
- **Panel width at 8 slots**: shrink-to-fit keeps it on screen; tiles get small only at high DPI scaling.

## Migration

v1 files are read as "show these"; the first write produces v2. Nothing else persists. **Downgrade hazard:** v0.3 and earlier reject a v2 file and then clear it, so a crash under v4 followed by running an older build loses the record of hidden windows. README: choose Restore all before running an older build.
