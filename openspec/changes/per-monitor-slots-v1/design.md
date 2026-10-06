## Context

Greenfield Rust app for a Windows 11 machine with 2 monitors, keyboard + mouse (no touchpad). Windows has no supported per-monitor virtual desktop API, and the undocumented `IVirtualDesktopManagerInternal` COM interface changes GUIDs across builds, so we fake per-monitor desktops by hiding and showing top-level windows ourselves. The developer knows C#/C++ and is learning Rust; code should be idiomatic but plain.

This is a productivity tool that hides other apps' windows, so the overriding constraint is: **never lose a window**, whatever happens to the process.

## Goals / Non-Goals

**Goals:**
- Independent window slots per monitor, switched by hotkey on the monitor under the cursor.
- Slot logic is pure Rust, fully unit-tested with `cargo test`, no Win32 types.
- Every hidden window is recoverable after a crash, panic, kill, logoff, or Explorer restart.
- Structure that grows into "approach B" (live tracking, move-window-to-slot, named slots) without rewriting the core.

**Non-Goals:**
- Animations, Win+Tab / taskbar / native virtual desktop integration.
- Touchpad gestures, autostart, settings UI, configurable hotkeys.
- Live window tracking via WinEvent hooks (B).
- Managing elevated windows.

## Decisions

### 1. Functional core, imperative shell
```
src/core.rs   pure: State, switch() -> Plan, restore_all(), should_restore(), state-file format.
              No unsafe, no windows crate.
src/win.rs    unsafe Win32: enumerate/filter windows, hide/show, monitors, file I/O.
src/main.rs   process setup, message loop: hotkeys, tray, OSD, session/display events.
```
`core::switch(state, monitor, dir, visible_topmost_first) -> Plan { hide, show }`. Slots hold `Entry { id: u64, pid: u32, iconic: bool, class: String }` — the same struct the state file serializes — so `show` and `should_restore` have the identity recorded at hide time. Monitor ids are `String`. All plain values, so tests use fake data. Slot count is a single `const SLOTS: usize = 4` in the core. No traits/mocks: the shell gathers data, the core decides, the shell executes.
*Alternative:* trait-abstracted Win32 for mocking — rejected; one implementation, adds indirection, the data-in/plan-out split already isolates the logic.
*B path:* later additions (`on_moved`, `move_to_slot`, named slots) are new pure functions over the same `State`.

### 2. Snapshot-on-switch, one slot per window
When leaving a slot, the windows currently visible on that monitor *are* that slot. New windows, windows dragged between monitors, and closed windows are handled for free. **Invariant:** a window id is in at most one slot across all monitors — capturing a snapshot removes those ids from every other slot. Without this, a window dragged from monitor A to B and hidden in B's slot would reappear when A returns to its old slot.
*Alternative:* WinEvent hook tracking (approach B) — deferred.

### 3. Hide/show mechanics
- Hide with `ShowWindowAsync(SW_HIDE)`: removes from the taskbar and Alt+Tab, and never blocks on a hung app. Windows where `IsHungAppWindow` is true are skipped (left visible).
- **No post-hide visibility check.** `ShowWindowAsync` only posts to the target thread, so `IsWindowVisible` immediately afterwards is still true for nearly every cross-process window; checking it would drop real windows from the record and lose them. Every planned window stays recorded as hidden. A window that truly can't be hidden (elevated, UIPI) just stays visible; showing it later is a no-op, and the next snapshot of its monitor moves it into that slot via the one-slot invariant.
- Show with `ShowWindowAsync(SW_SHOWNA)` (or `SW_SHOWMINNOACTIVE` if spike 2.2 shows minimized state is lost). Hidden windows keep their z-order position, so showing preserves stacking; no reordering is done.
- Before every show, verify the id still exists and its pid + class match what was recorded (guards against in-session HWND reuse).
- **Focus:** after a switch, `SetForegroundWindow` the topmost shown window unless it is DWM-cloaked (lives on another native virtual desktop — focusing it would switch the user's native desktop). If nothing was shown, focus the topmost visible participating window on any monitor **excluding `plan.hide`** (they may not have processed the async hide yet), else the shell window (`GetShellWindow`), so keystrokes never go to a hidden window. `SetForegroundWindow` right after an async show relies on the target thread processing the show first — verify in spike 2.3.

### 4. Which windows participate
A **root** window participates iff: top-level, `IsWindowVisible`, no owner, not `WS_EX_TOOLWINDOW`, not DWM-cloaked (`DWMWA_CLOAKED` — this also excludes windows on other native virtual desktops), not our own process, not a shell window (`Progman`, `WorkerW`, `Shell_TrayWnd`, `Shell_SecondaryTrayWnd`).

**Owned windows** (dialogs, popups) do NOT follow a hidden owner. Any visible top-level window whose root owner (`GetAncestor(GA_ROOTOWNER)`) participates is included in the same snapshot, in z-order, and hidden/shown with it.

**Monitor mapping:** `MonitorFromWindow(MONITOR_DEFAULTTONEAREST)` (largest overlap, settles boundary windows) for normal and maximized windows; for minimized windows (`IsIconic`, live rect at -32000) use `GetWindowPlacement().rcNormalPosition`, converted from workspace to screen coordinates, → `MonitorFromRect`. Owned windows map to their root owner's monitor.

**DPI:** the process calls `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` first thing so rects are not virtualized on mixed-DPI setups.

### 5. Monitor identity
Slots keyed by the monitor's device name (`MONITORINFOEXW.szDevice`). On `WM_DISPLAYCHANGE`, compare the set of device names to the previous set; restore all and reset only if it changed. Resolution/DPI changes and most RDP reconnects leave the set intact and keep slots. (DisplayPort monitors may drop out of the set when they sleep; that counts as a change and resets — safe, just less convenient.)

**Off-screen guard:** after any show (switch, Restore all, startup restore, reset), map the window to a monitor with the **same helper as §4** (`rcNormalPosition` for iconic windows — their live rect is always at -32000) but with `MONITOR_DEFAULTTONULL`. If no monitor: skip if `IsHungAppWindow` (it stays visible in the taskbar; Win+Shift+Arrow recovers it), otherwise `GetWindowPlacement`, clamp `rcNormalPosition` into `(0,0)–(work width, work height)` — it is in **workspace coordinates** (relative to the primary work area), so add no screen offset — set `showCmd` explicitly from `IsIconic`/`IsZoomed` (never `SW_HIDE`), and `SetWindowPlacement` with `WPF_ASYNCWINDOWPLACEMENT` so it never blocks. One call path for normal, minimized, and maximized windows.

### 6. Hotkeys
`RegisterHotKey` with `MOD_CONTROL | MOD_ALT | MOD_NOREPEAT` for `VK_LEFT`/`VK_RIGHT`. Clamped at 1 and `SLOTS`, no wrap. On registration failure, show a message box naming the hotkey and exit. Everything runs on the one message-loop thread; with `ShowWindowAsync` a switch never waits on other processes.

**Rapid presses vs async hide/show:** a second switch can arrive before the first one's hides/shows have been processed, so the snapshot would see pending-hide windows as visible and pending-show windows as invisible, scrambling slot contents. Fix in the core: each monitor remembers its last `Plan` and the time it was made (`now_ms` is passed into `switch`, keeping it pure). If the previous switch on that monitor was under 1 s ago, the snapshot is corrected before capture: remove `last.hide` ids, add `last.show` entries not already present, appended at the bottom of the z-order. Dead entries are harmless — `show` verifies every entry first. Outside that window the raw snapshot is trusted, so an app re-showing its own window is still absorbed normally. `restore_all` and the monitor-set reset clear every monitor's last plan.

### 7. Persistence (the state file is the recovery source of truth)
- File: `%LOCALAPPDATA%\screen-shifter\hidden.txt`. First line `v1`, then one line per hidden window: `hwnd pid iconic class` (class last since it may contain spaces).
- **Write-ahead with union:** on every switch, (1) write **old hidden ∪ new hidden** — this keeps the target slot's windows listed until they are actually shown; (2) hide; (3) show; (4) always rewrite the exact new hidden set. A crash at any step leaves the file listing a superset of what is hidden; restoring an already-visible window is a no-op. *Do not* write only the new set first: the target slot's windows would then be in neither the file nor live state if the process died before showing them.
- **Clearing** the file means atomically writing an empty `v1` file, never deleting it mid-operation.
- **Atomic:** write `hidden.txt.tmp`, then `std::fs::rename` over `hidden.txt`.
- Pure functions in the core: `serialize`, `parse` (unknown version → treat as empty, log), and `should_restore(entry, live_pid, live_class) -> bool`.
- Plain text, no serde dependency. The version line lets B add slot membership later without breaking crash restore.

### 8. Process lifetime and failure handling
- **Single instance:** named mutex `Local\screen-shifter`; if it already exists, exit immediately (a second copy would restore and delete the first copy's file).
- **App window:** a normal top-level window that is created but never shown — **not** a message-only (`HWND_MESSAGE`) window. Message-only windows don't receive broadcasts, so they'd never get `WM_QUERYENDSESSION`, `WM_DISPLAYCHANGE`, or `TaskbarCreated`. Hotkey, tray, and session messages all go to it.
- **Startup:** set DPI awareness → take mutex → restore everything in the state file → clear it → create app window → register hotkeys → tray.
- **Panic:** `std::panic::set_hook` restores from the state file (write-ahead makes it authoritative) then delegates to the default hook. Since Rust 1.81 the hook runs before the abort caused by a panic escaping an `extern "system"` wndproc, so no `catch_unwind` is needed. The hook may run twice (the abort itself can re-enter it), so restore must be idempotent — it is. The hook MUST NOT touch the `RefCell` state (it may be borrowed) — file only.
- **Re-entrancy:** `ShowWindowAsync` doesn't pump messages, but `MessageBoxW`, `TrackPopupMenu`, and `SetForegroundWindow` can re-enter the wndproc. App state lives in a `RefCell` (thread-local); never hold a borrow across those calls — copy out what's needed, drop the borrow, then call.
- **Logoff/shutdown:** on `WM_QUERYENDSESSION` do exactly "Restore all" (show everything, reset state, clear file) and return TRUE, so "save changes?" prompts are visible and a cancelled logoff leaves consistent state.
- **Explorer restart:** register `TaskbarCreated` and re-add the tray icon when it arrives.
- **Exit / Restore all:** restore all, clear file (Exit also unregisters hotkeys and removes the tray icon).

### 9. Tray + OSD with raw Win32
Tray: `Shell_NotifyIconW` with a right-click popup menu (Restore all, Exit). OSD: one layered, topmost, `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW` window, "N / 4" drawn with GDI, centered on the affected monitor, hidden by a ~600 ms `SetTimer`. No fade in v1. The OSD and hidden message window belong to our process and are excluded by the filter.
*Alternative:* `tray-icon` crate — fine fallback if raw tray code gets ugly.

### 10. Diagnostics
Debug builds keep a console and log, per switch, every window the filter accepted/rejected (hwnd, class, title, monitor, reason). This is the main tool for tuning §4.

## Risks / Trade-offs

- [Elevated windows can't be hidden (UIPI)] → they stay visible; their stale record is harmless and is cleaned up by the next snapshot. Documented in README. Running elevated would fix it but isn't worth it for v1.
- [Hidden minimized window may show un-minimized] → spike 2.2; `iconic` is recorded either way.
- [Filter lets junk through or drops real windows (UWP frames, Electron)] → debug log (§10), tune with real apps.
- [App shows its own window while its slot is inactive] → it appears on the current slot and is absorbed by the next snapshot. Acceptable.
- [Hard kill (Task Manager, power loss)] → no hook runs; next startup restores from the write-ahead file. After a reboot the windows are gone anyway.
- [Slots are global across native virtual desktops] → cloaked windows (on other native desktops) are excluded from snapshots and never focused. If the user switches native desktops while slots are hidden, hidden windows reappear wherever their native desktop is. Acceptable since the user doesn't use native desktops; documented.
- [Hotkey conflict with another app] → message box naming the hotkey; change the constant and rebuild.

## Migration Plan

N/A — new app. Rollback = exit the app (restores all windows).

## Open Questions

- Does `SW_SHOWNA` preserve minimized state after `SW_HIDE`? (Spike 2.2.)
- Do owned popups (e.g. a find dialog) re-show cleanly with their owner, or need ordering after it? (Spike 2.3.)
- Some Delphi/WinForms apps make their main window owned by an invisible window, so the root owner never qualifies and the app is never managed (it stays visible — not lost). If spike 2.1 finds real cases, treat owned windows with `WS_EX_APPWINDOW` as roots.
- Does Windows reposition hidden windows when a monitor is removed? The off-screen guard covers it either way.
