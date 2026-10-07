## 1. Spikes (throwaway; only touch windows whose process the spike launched, matched by PID)

- [ ] 1.1 Ghost: `DwmRegisterThumbnail` of a spawned window into a topmost tool popup sized to the thumbnail with opacity ~60%; confirm it looks translucent (no opaque background) and tracks the cursor smoothly. If not, try `DwmExtendFrameIntoClientArea`, else fall back to `PrintWindow` + `UpdateLayeredWindow`. Record in design §6
- [ ] 1.2 Cross-monitor move: `SetWindowPlacement(WPF_ASYNCWINDOWPLACEMENT)` to the other monitor then `ShowWindowAsync(SW_HIDE)`, then `SW_SHOWNA` — confirm it hides and reappears on the new monitor, for normal and maximized windows. Record in design §7
- [ ] 1.3 Hooks: dedicated thread with `WH_KEYBOARD_LL` + `WH_MOUSE_LL`, logging Ctrl/Alt transitions, wheel, and G502 tilt messages; confirm tilt arrives as `WM_MOUSEHWHEEL` and swallowing works (browser doesn't zoom)

## 2. Core (`src/core.rs`)

- [ ] 2.1 `switch_to(monitor, slot, visible, now_ms) -> Plan`; make `switch(dir)` a wrapper; no-op for current slot
- [ ] 2.2 `move_to_slot(entries, to_monitor, to_slot) -> bool` (true = hide): remove ids from every slot and every monitor's last plan; record unless target is that monitor's active slot
- [ ] 2.3 Expose read-only slot contents for the panel: `slots(monitor) -> (active, [&[Entry]; SLOTS])`
- [ ] 2.4 Tests: `switch_to` jump/no-op/independence; each row of the design §7 move table; move then quick switch keeps the window in its new slot; owned entries move together; extend the randomized never-lose test with random moves

## 3. Win32 shell (`src/win.rs`)

- [ ] 3.1 `title_of` made public; `root_of_point(pt) -> Option<Vec<Entry>>`: `WindowFromPoint` → `GA_ROOT` → participating root plus its visible owned windows, topmost first
- [ ] 3.2 `relocate(entry, from_monitor, to_monitor)`: shift `rcNormalPosition` by the work-area offset, clamp, `SetWindowPlacement` async with the current show state
- [ ] 3.3 `monitor_rects() -> Vec<(String, RECT work_area)>` for panel layout

## 4. Hook thread (`src/hooks.rs`)

- [ ] 4.1 Atomics `CHORD`, `PANEL`, `DRAG`; app `HWND` set once before start; custom messages `WM_CHORD_DOWN/UP`, `WM_HOOK_WHEEL/HWHEEL/LDOWN/LUP/MOVE` carrying point and delta
- [ ] 4.2 Keyboard hook: track Ctrl/Alt (either side), taint on any other keydown, post transitions; never swallow
- [ ] 4.3 Mouse hook: while `CHORD`, swallow and post wheel, hwheel, left down/up; post (don't swallow) moves only while `DRAG`
- [ ] 4.4 Thread start/stop: own `GetMessage` loop; `PostThreadMessageW(WM_QUIT)` + join on exit; unhook on that thread

## 5. App (`src/main.rs` + `src/panel.rs`)

- [ ] 5.1 Panel windows per monitor (layered, topmost, no-activate, click-through), layout of 4 tiles near the bottom of each work area, tile rects kept for hit-testing; rebuild on `WM_DISPLAYCHANGE`
- [ ] 5.2 Panel paint: slot number, up to 5 ellipsized titles or "—", current outline, drop-target highlight
- [ ] 5.3 `WM_CHORD_DOWN` → 250 ms timer → show panels if still held; `WM_CHORD_UP` → hide panels, cancel drag, reset wheel accumulator
- [ ] 5.4 Wheel accumulator (±120 per switch, up = next) and tilt bursts (one switch, 400 ms quiet gap, right = next) → `on_switch` on the monitor under the point; repaint panels
- [ ] 5.5 Tile click → `switch_to` path (shared with `on_switch`: write-ahead, hide, show, persist, focus)
- [ ] 5.6 Drag: start on left-down over a managed window, ghost with DWM thumbnail (per spike 1.1), move on posted moves, highlight drop tile; drop → move flow (design §7); cancel paths
- [ ] 5.7 Suppress `show_osd` while `PANEL` is set
- [ ] 5.8 Debug log of every hook message received (for G HUB / tilt diagnosis)

## 6. Verify

- [ ] 6.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [ ] 6.2 Manual run-through of every scenario in `specs/slot-panel` and the new/modified `specs/monitor-slots` scenarios, with the G502
- [ ] 6.3 Mouse never lags: drag and scroll quickly while a switch is in progress; normal scrolling and `Ctrl+Alt+T`-style shortcuts unaffected
- [ ] 6.4 README: new gestures
