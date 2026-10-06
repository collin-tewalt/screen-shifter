## 1. Setup

- [ ] 1.1 Install Rust (`winget install Rustlang.Rustup`), confirm MSVC linker, verify `cargo --version`
- [ ] 1.2 `cargo init --name screen-shifter`, add `windows` crate with needed features, add `.gitignore` for `target/`
- [ ] 1.3 `#![windows_subsystem = "windows"]` for release builds only, so debug builds keep a console for logging
- [ ] 1.4 First lines of `main`: `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`; verify with a debug print of a window rect on each monitor

## 2. Spikes (de-risk Win32 behavior before building on it; throwaway code)

- [ ] 2.1 Enumerate windows with the design §4 filter (roots + owned) and print hwnd/class/title/monitor/reason; check against Explorer, a browser, VS Code, a UWP app (Settings), a minimized window, a maximized window, an app with an open dialog, and any app whose main window is owned (check for `WS_EX_APPWINDOW`). Record surprises in design.md
- [ ] 2.2 `SW_HIDE` then `SW_SHOWNA` a minimized window and a UWP app (Settings) — confirm minimized state survives and UWP re-shows cleanly; update design §3 if not. Also hide a normal, a minimized, and a maximized window on monitor B, unplug B, re-show them: note where they land and confirm the guard keeps their show state
- [ ] 2.3 Hide and re-show an app with an owned dialog open; confirm both come back and the dialog stays above its owner. Also confirm `SetForegroundWindow` right after `ShowWindowAsync(SW_SHOWNA)` reliably focuses the shown window

## 3. Pure core (`src/core.rs`)

- [ ] 3.1 Define `const SLOTS`, `Entry { id, pid, iconic, class }`, `State` (monitor name → active index + slots of `Entry`), `Dir`, `Plan { hide, show }`
- [ ] 3.2 Implement `switch(state, monitor, dir, visible_topmost_first, now_ms) -> Plan`: snapshot correction from the last plan if < 1 s old (design §6), snapshot-on-switch, clamping, and removing captured ids from every other slot
- [ ] 3.3 Implement `restore_all(state) -> Vec<Entry>` (returns all hidden entries, resets every monitor to slot 1, clears every last plan) and `reset_if_monitors_changed(state, names) -> Option<Vec<Entry>>`
- [ ] 3.4 Unit tests: monitor independence, clamp at both ends, round-trip order, new window joins current slot, dragged window stays on its monitor, empty slot, unknown monitor initialised on first use, window re-shown on other monitor is in only one slot, rapid Right×3/Left×3 with a stale snapshot restores the original slot exactly, a switch > 1 s later trusts the raw snapshot, a switch right after `restore_all` applies no stale correction, `restore_all`, monitor-set change vs unchanged
- [ ] 3.5 Property-style test: over a random sequence of switches, every `show` id was previously hidden, no id is in two slots, and every id ever hidden is either in some slot or in `restore_all`'s output (never-lose-a-window)
- [ ] 3.6 State file format: `serialize` / `parse` (`v1` header, `hwnd pid iconic class`, bad version or lines → skipped and reported) and `should_restore(entry, live_pid, live_class)`; round-trip and malformed-input tests

## 4. Win32 shell (`src/win.rs`)

- [ ] 4.1 Monitor helpers: cursor monitor device name, monitor rect by name, current set of monitor names
- [ ] 4.2 `visible_windows_on(monitor)`: roots + owned windows, topmost first, `MonitorFromWindow` for normal windows and `rcNormalPosition` (workspace→screen) for iconic ones; debug log of accept/reject reasons
- [ ] 4.3 `hide(entries)`: skip `IsHungAppWindow`, `ShowWindowAsync(SW_HIDE)`. No post-hide visibility check (async — see design §3)
- [ ] 4.4 `show(entries)`: verify pid + class via `should_restore`, `ShowWindowAsync` with the right show command for iconic windows; off-screen guard per design §5 (same monitor-mapping helper as 4.2 with DEFAULTTONULL; skip hung; clamp `rcNormalPosition` in workspace coords; explicit `showCmd`; `SetWindowPlacement` with `WPF_ASYNCWINDOWPLACEMENT`)
- [ ] 4.5 `focus_after_switch(shown, just_hidden)`: topmost shown and not cloaked → else topmost visible participating window not in `just_hidden` → else `GetShellWindow()`
- [ ] 4.6 State file I/O under `%LOCALAPPDATA%\screen-shifter\`: atomic write (tmp + `fs::rename`), read; "clear" = atomically write an empty `v1` file

## 5. App (`src/main.rs`)

- [ ] 5.1 Single-instance named mutex `Local\screen-shifter`; second instance exits silently
- [ ] 5.2 Panic hook: restore from the state file only (never touch the `RefCell`), idempotent, then run the default hook
- [ ] 5.3 Startup order: DPI → mutex → panic hook → restore from state file + clear → create the app window (normal top-level, never shown, NOT `HWND_MESSAGE`) → register hotkeys (`MOD_NOREPEAT`; message box naming the failed hotkey, then exit)
- [ ] 5.4 Thread-local `RefCell<State>`; on `WM_HOTKEY`: gather → `core::switch` → persist old ∪ new hidden → hide → show → persist exact new hidden → focus → OSD. Never hold the borrow across `MessageBoxW`/`TrackPopupMenu`/`SetForegroundWindow`
- [ ] 5.5 Tray icon with "Restore all" and "Exit"; re-add on the `TaskbarCreated` message; Exit restores, clears the file, unregisters hotkeys, removes the icon
- [ ] 5.6 `WM_QUERYENDSESSION`: run the same path as "Restore all" (show, reset state, clear file) and return TRUE
- [ ] 5.7 `WM_DISPLAYCHANGE`: `reset_if_monitors_changed`; restore only if the monitor set changed
- [ ] 5.8 OSD: layered, topmost, no-activate, click-through window showing "N / SLOTS" centered on the monitor, hidden after ~600 ms

## 6. Verify

- [ ] 6.1 `cargo test` and `cargo clippy -- -D warnings` clean
- [ ] 6.2 Manual run-through of every scenario in `specs/monitor-slots` and `specs/window-recovery` on the 2-monitor setup
- [ ] 6.3 Failure drills: kill via Task Manager → relaunch restores; debug-only hotkey that panics → windows restored; restart Explorer → tray returns; launch a second copy → no effect; sign out with a hidden Notepad holding unsaved text → prompt visible
- [ ] 6.4 README: hotkeys, recovery behaviour, known limitations (elevated windows are sticky; slots are global across native virtual desktops)
