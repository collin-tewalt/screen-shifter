## Context

The panel opens on Ctrl+Alt (`src/hooks.rs` `keyboard()` → `WM_CHORD_DOWN` → `show_panels()` in `src/main.rs`). While the chord is held, `hooks::mouse()` swallows clicks and wheel input and posts moves for the exposé. On release (`WM_CHORD_UP`, `src/main.rs` ~866), only a preview under the mouse is chosen; everything else collapses. Tile clicks go through `on_ldown`/`on_lup` → `panel::hit` → `jump()`.

Windows 11 maps a three-finger swipe up to Task View by default (Dell precision touchpads follow that). The app cannot veto the shell's action directly, so its detection and any change to the Windows setting need a spike first.

## Goals / Non-Goals

Goals: release over a tile switches; a three-finger swipe opens the same panel; every exit leaves input normal.

Non-goals: a separate swipe-only panel; pinch/rotate gestures; changing the 250 ms Ctrl+Alt delay.

## Decisions

### Release over a tile
On `WM_CHORD_UP` with reason `RELEASED`, when no preview was chosen, look up the tile under the cursor with `panel::hit` and, if it is a real slot other than the current one, call the same code path as a tile click (`jump()`). "+ New" and the current slot do nothing. The release then collapses the spread and hides the panels as today.

### Swipe detection
Register Raw Input for the touchpad (HID usage page 0x0D, usage 0x05) on the app window and count active contacts and vertical travel. Three contacts moving up past a threshold, with no button held, posts `WM_PANEL_GESTURE` (new, `WM_APP + 17`) with wparam = up/down. The spike decides whether WM_POINTER is needed instead. Detection runs on the app's message thread, not in a hook, so it can do real work.

### Swipe mode
A second atomic `PANEL_LATCH` in `hooks.rs`. The mouse hook uses `CHORD || PANEL_LATCH` wherever it currently checks `CHORD`, so clicks and wheel are swallowed the same way. `WM_PANEL_GESTURE` (up) opens the panel immediately by calling `show_panels()` and setting the latch. The latch is cleared and the panel hidden on: Esc (the keyboard hook posts a message when Esc is pressed while the latch is set), a click on empty space, a three-finger swipe down, or a click on a tile or preview (which acts first). The release-based branch of `WM_CHORD_UP` is skipped in this mode because no key-release is involved.

Exits all go through one function (e.g. `end_panel_mode(action)`), which clears the latch, cancels drags and hover, and hides panels. `show_all` (quit, logoff, orderly exit paths) also clears the latch.

Rules for the cases the code otherwise falls through on:

- **Ctrl+Alt released while the latch is set** (`WM_CHORD_UP`, `src/main.rs` ~880): the release handler goes through `end_panel_mode` as well, so the latch is cleared. Without this, a tap of Ctrl+Alt during swipe mode would hide the panels and leave the mouse hook swallowing clicks.
- **Left-down with no panel hit in swipe mode** (`on_ldown`, `src/main.rs` ~682-705): today this falls through to `win::root_at` and starts a window drag. In swipe mode it instead calls `end_panel_mode` and starts no drag, so the click never reaches or drags the window underneath.
- **Mouse-hook reset** (`src/hooks.rs` ~100-106): the `CHORD && !(CTRL && ALT held)` reset stays CHORD-only. Only the swallow checks move to `CHORD || PANEL_LATCH`. Changing the reset to the OR form would post spurious `WM_CHORD_UP` on every latch-mode move.
- **Esc** is not swallowed by the keyboard hook; it still reaches the focused application (for example, to close a dialog). The swipe-mode exit happens in addition to that.
- **Crash path**: the hooks are process-scoped, so Windows removes them when the process dies. Clearing the latch in `show_all` is for orderly exits; the crash protection for input is the OS removing the hooks.

### Windows setting
Spike result (done): the touchpad reports contacts through Raw Input, so the app can detect the swipe, but Windows still opens Task View on it, so the shell's action can't be prevented from the app. The setting route is used.

The setting is `HKCU\Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad`, value `ThreeFingerSlideEnabled` (DWORD). Confirmed by diffing the registry while the owner set Settings > Bluetooth & devices > Touch > Three-finger gestures from "Switch apps and show desktop" to "Nothing": the value went from absent to `0`. Absent means the default (enabled).

- On first need the app shows a one-time in-app prompt. On consent it saves the previous state to `%LOCALAPPDATA%\screen-shifter\touchpad.txt` (either "absent" or the previous DWORD), then sets the value to `0`.
- On Exit, and on the next start if the file is present, it restores that state: "absent" means delete the value, not write `1`.

The original value is written before the change, so a crash leaves a recoverable record for the next start (the same pattern as `hidden.txt`).

## Risks / Trade-offs

- The Windows-setting path changes machine state, so it is gated on explicit consent and restored on Exit. A crash before restore is recovered on next start from `touchpad.txt`.
- Raw Input for touchpads varies by driver. If Dell's driver does not expose contacts as raw HID, the fallback is WM_POINTER; if neither works, the feature is dropped and only the release change ships.
- Clicks on empty space close the panel, which means the click is swallowed; that matches the rule that panel-mode clicks never reach windows.
