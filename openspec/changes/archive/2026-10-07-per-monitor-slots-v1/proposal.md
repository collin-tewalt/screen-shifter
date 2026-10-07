## Why

Windows virtual desktops are global: switching desktops switches every monitor at once. On a two-monitor setup that means you can't keep reference material parked on one screen while cycling through work on the other, the way macOS "Displays have separate Spaces" allows. A small tray app that gives each monitor its own independent set of window "slots" fixes this without fighting the OS.

## What Changes

- New Rust Windows tray app (`screen-shifter`) that gives each monitor 4 independent window slots.
- `Ctrl+Alt+←/→` switches the slot on the monitor under the mouse cursor; the other monitor is untouched.
- Switching hides the current slot's windows (`SW_HIDE`, which also removes them from the taskbar) and shows the target slot's windows. No animation.
- Slot membership is captured by snapshot at switch time — whatever is visible on that monitor when you leave a slot belongs to it. No background window tracking.
- A brief "2 / 4" on-screen indicator on the affected monitor.
- Crash safety: hidden windows are written to disk *before* they are hidden and restored on startup, exit, panic, logoff/shutdown, and via a tray "Restore all" item. Only one instance runs at a time.
- Non-goals: animations, Win+Tab / taskbar / native virtual desktop integration, touchpad gestures, autostart, live window tracking (deferred to a later "own the illusion" change).

## Capabilities

### New Capabilities
- `monitor-slots`: per-monitor slot model, hotkey switching on the cursor's monitor, which windows participate, hide/show behavior, focus, and the slot indicator.
- `window-recovery`: guaranteeing hidden windows are never lost — persistence, restore on startup/exit, and the tray "Restore all" / "Exit" controls.

### Modified Capabilities
<!-- none: greenfield project -->

## Impact

- New Rust crate at repo root; dependencies: `windows` (official Win32 bindings). Tray via raw `Shell_NotifyIcon` unless it proves painful.
- Writes a state file under `%LOCALAPPDATA%\screen-shifter\`.
- Registers global hotkeys `Ctrl+Alt+Left` / `Ctrl+Alt+Right`; conflicts with any other app using them.
- Requires the Rust toolchain (not yet installed on the dev machine) plus the MSVC linker.
