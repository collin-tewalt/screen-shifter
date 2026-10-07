## 1. Spikes (throwaway; only touch windows whose process the spike launched, matched by PID)

- [x] 1.1 Ghost: `DwmRegisterThumbnail` of a spawned window into a topmost tool popup sized to the thumbnail with opacity ~60%; confirm it looks translucent (no opaque background) and tracks the cursor smoothly. If not, try `DwmExtendFrameIntoClientArea`, else fall back to `PrintWindow` + `UpdateLayeredWindow`. Record in design §6 — resolved by design: built as a PrintWindow snapshot (design §6) instead of spiking DWM thumbnails
- [ ] 1.2 Cross-monitor move: `SetWindowPlacement(WPF_ASYNCWINDOWPLACEMENT)` to the other monitor then `ShowWindowAsync(SW_HIDE)`, then `SW_SHOWNA` — confirm it hides and reappears on the new monitor, for normal and maximized windows. Record in design §7 — folded into manual verification 6.2 (needs the G502 and a real cross-monitor drag)
- [ ] 1.3 Hooks: dedicated thread with `WH_KEYBOARD_LL` + `WH_MOUSE_LL`, logging Ctrl/Alt transitions, wheel, and G502 tilt messages; confirm tilt arrives as `WM_MOUSEHWHEEL` and swallowing works (browser doesn't zoom). Also: does `SetForegroundWindow` succeed when triggered from a hook-posted message? If not, verify the `SendInput(0xE8)` nudge (design §10) — folded into manual verification 6.2 (needs the G502 and a real cross-monitor drag)

## 2. Core (`src/core.rs`)

- [x] 2.1 `switch_to(monitor, slot, visible, now_ms) -> Plan`; make `switch(dir)` a wrapper; no-op for current slot
- [x] 2.2 `move_to_slot(entries, from_monitor, to_monitor, to_slot, now_ms) -> bool` (true = hide): remove ids from every slot and last plan; record unless target is active; no-op returns before touching state; normalize missing/stale `last` to an empty plan, then add to `from_monitor`'s `last.hide` and, for another monitor's active slot, to `to_monitor`'s `last.show` (design §7)
- [x] 2.3 Expose read-only slot contents for the panel: `slots(monitor) -> (active, [&[Entry]; SLOTS])`
- [x] 2.4 Tests: `switch_to` jump/no-op/independence; each row of the design §7 move table; move then quick (stale-snapshot) switch on the source monitor keeps the window in its new slot; cross-monitor move to an active slot then quick switch on the target captures it; a move onto a monitor whose last plan is old does not revive that plan's stale entries; same-monitor active-slot drop changes nothing; owned entries move together; extend the randomized never-lose test with random moves

## 3. Win32 shell (`src/win.rs`)

- [x] 3.1 `title_of` made public; `root_of_point(pt) -> Option<Vec<Entry>>`: `WindowFromPoint` → `GA_ROOTOWNER` → participating root plus its visible owned windows, topmost first
- [x] 3.2 `relocate(entry, from_monitor, to_monitor)`: shift `rcNormalPosition` by the work-area offset, clamp, `SetWindowPlacement` async with a no-activate show command (as `bring_on_screen`)
- [x] 3.3 `monitor_rects() -> Vec<(String, RECT work_area)>` for panel layout

## 4. Hook thread (`src/hooks.rs`)

- [x] 4.1 Atomics `CHORD`, `PANEL`, `DRAG`, `SWALLOWED_DOWN`, `MOVE_PENDING`, latest move point; app `HWND` set once before start; custom messages `WM_CHORD_DOWN/UP`, `WM_HOOK_WHEEL/HWHEEL/LDOWN/LUP/MOVE` carrying point and delta
- [x] 4.2 Keyboard hook: track Ctrl/Alt (either side), taint on any other keydown except `VK_LEFT`/`VK_RIGHT` and injected keys, post transitions; never swallow
- [x] 4.3 Mouse hook: stuck-chord guard via `GetAsyncKeyState` on every event while `CHORD` (also resets the shared Ctrl/Alt flags; pairing rule wins for left-up); while `CHORD`, swallow and post wheel, hwheel, left down (set `SWALLOWED_DOWN`); swallow and post left up iff `SWALLOWED_DOWN`; coalesced move posts while `DRAG`, never swallowing moves
- [x] 4.4 Thread start/stop: own `GetMessage` loop; `PostThreadMessageW(WM_QUIT)` + join on Exit only (not on `WM_QUERYENDSESSION`); unhook on that thread

## 5. App (`src/main.rs` + `src/panel.rs`)

- [x] 5.1 Panel windows per monitor (layered, topmost, no-activate, click-through), layout of 4 tiles near the bottom of each work area, tile rects kept for hit-testing; rebuild on `WM_DISPLAYCHANGE`
- [x] 5.2 Panel paint: slot number, up to 5 ellipsized titles or "—", current outline, drop-target highlight
- [x] 5.3 `WM_CHORD_DOWN` → 250 ms timer → show panels if still held; `WM_CHORD_UP` → hide panels, cancel drag, reset wheel accumulator
- [x] 5.4 Wheel accumulator (±120 per switch, up = next) and tilt bursts (one switch, 400 ms quiet gap, right = next) → `on_switch` on the monitor under the point; repaint panels
- [x] 5.5 Tile click → `switch_to` path (shared with `on_switch`: write-ahead, hide, show, persist, focus); apply the §10 foreground nudge for hook-driven switches if spike 1.3 shows it's needed
- [x] 5.6 Drag: start on left-down over a managed window, ghost with DWM thumbnail (per spike 1.1), move on posted moves, highlight drop tile; drop → move flow (design §7); cancel paths
- [x] 5.7 Suppress `show_osd` while `PANEL` is set
- [x] 5.8 Debug log of every hook message received (for G HUB / tilt diagnosis)

## 6. Verify

- [x] 6.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [ ] 6.2 Manual run-through of every scenario in `specs/slot-panel` and the new/modified `specs/monitor-slots` scenarios, with the G502
- [ ] 6.3 Stuck-input drills: Ctrl+Alt+Del and lock/unlock while holding the chord, then click normally; press a title bar then Ctrl+Alt then release; release Ctrl+Alt mid-drag. Mouse never lags: drag and scroll quickly while a switch is in progress; normal scrolling and `Ctrl+Alt+T`-style shortcuts unaffected
- [x] 6.4 README: new gestures
