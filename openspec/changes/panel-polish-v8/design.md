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
Raw Input for the touchpad (HID usage page 0x0D, usage 0x05) is registered on the app window (`src/touchpad.rs`, `register`). `on_raw_input` parses each contact report (report ID 3: up to five contacts of flags, X, Y, and a contact count). `Tracker` follows three tip-down contacts from touchdown and reports `Update::Begin`, then `Vertical(dy)` as the fingers move, and a one-time `Horizontal(dx)`. The first axis to pass `PANEL_UNITS` is the swipe's axis. The spike decided against WM_POINTER: Raw Input delivers the contacts directly.

### Live phases
`touchpad::target(start, dy)` maps the vertical travel from the screen state at touchdown to a phase: closed, panel, or exposé. The panel comes in at `PANEL_UNITS` (100) of travel and the exposé at `EXPOSE_UNITS` (250). Reversing past a threshold undoes the phase it crossed, so the screen follows the fingers, and releasing leaves it where it is. `goto` in `src/main.rs` steps the screen to the target phase, at most two steps. Left and right switch the slot under the cursor once, when the horizontal travel passes `PANEL_UNITS`. Earlier versions committed on lift; the live model replaced that after testing.

### Swipe mode
A second atomic `PANEL_LATCH` in `hooks.rs`. The mouse hook uses `CHORD || PANEL_LATCH` wherever it currently checks `CHORD`, so clicks and wheel are swallowed the same way. A panel opened by a swipe sets the latch and calls `show_panels()`. The latch is cleared and the panel hidden on: Esc (the keyboard hook posts `WM_PANEL_ESC` when Esc is pressed while the latch is set), a click on empty space, a swipe down past `PANEL_UNITS` from the panel, or a click on a tile or preview (which acts first). The release-based branch of `WM_CHORD_UP` is skipped in this mode because no key-release is involved.

### Exposé on every monitor
In the Ctrl+Alt exposé one spread is open at a time, and moving to another monitor collapses it. A swipe up to the exposé opens one spread per monitor, each showing its current slot (`open_all_spreads`). In that mode hovering a tile re-spreads only that monitor, and moving between monitors keeps the others open. `expose::State.open` therefore holds one `Set` per monitor, and previews, hover, choose, and drag are keyed by window id, which is unique across monitors. Choosing a preview fades every spread. `open_expose` collapses the spreads first only outside swipe mode, which keeps the Ctrl+Alt rule.

### Collapse z-order
Previews are created topmost window first, and `stack` adds each new visual above the last, which left them upside down. `expose::restack` puts them in window order (topmost on top) before a fold or choose, so the previews fly back over one another as the real windows overlap. Without it, the wrong preview sat on top during the collapse, and the windows then snapped into their true order.

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

- On first start the app shows a one-time Yes/No prompt (`touchpad::startup`, `ask_owner`). The answer is recorded in `%LOCALAPPDATA%\screen-shifter\touchpad.txt` as `consent=yes|no`. On Yes it saves the previous value as `prior=absent` or `prior=<DWORD>` and then sets the value to `0`.
- On Exit (`shutdown`), on a panic (the panic hook), and on the next start if the record still shows a change, `touchpad::restore` puts the saved state back. "absent" means delete the value, not write `1`.
- The tray item "Three-finger swipe opens panel" (`set_enabled`) turns the feature and the setting on or off, and its checked state follows `ENABLED`.

The record is written before the change, so a crash leaves a recoverable record for the next start (the same pattern as `hidden.txt`).

## Risks / Trade-offs

- The Windows-setting path changes machine state, so it is gated on explicit consent and restored on Exit. A crash before restore is recovered on next start from `touchpad.txt`.
- Raw Input for touchpads varies by driver. If Dell's driver does not expose contacts as raw HID, the fallback is WM_POINTER; if neither works, the feature is dropped and only the release change ships.
- Clicks on empty space close the panel, which means the click is swallowed; that matches the rule that panel-mode clicks never reach windows.

## Decisions added during verification

- **Gap between tiles is empty space.** In swipe mode a click in the gap between tiles on the panel closes the panel, the same as a click on empty space (`on_ldown`). Ctrl+Alt mode is unchanged.
- **Switch keeps the exposé, per monitor.** A left/right switch in swipe mode closes only the switched monitor's spread (`expose::collapse_monitor`) and spreads its new slot again (`spread_monitor`). Other monitors' spreads are untouched. Ctrl+Alt arrow switches still clear everything.
- **Fast swipe waits for pictures.** A swipe-up exposé waits for the snapshot batch (`snap::busy`) before opening, for at most 300 ms (`EXPOSE_TIMER`). `snap` posts a final `WM_SNAPSHOT` when a batch ends, so the wait ends promptly. A reversal or the panel closing cancels the wait.
- **Dimmed area is empty space too.** In swipe mode a click on the dimmed screen around the exposé's previews (including the gaps between them) ends swipe mode: the exposé and the panel close. It used to close only the exposé, which left the panel up. Ctrl+Alt mode still closes only the exposé.
