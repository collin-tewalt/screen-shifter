## Context

v0.1 (`openspec/specs/monitor-slots`, `window-recovery`) is a single-threaded Rust tray app: pure `core.rs` (State, `switch`, state-file format), `win.rs` (Win32 window ops), `main.rs` (message loop, hotkeys, tray, OSD). All hide/show is `ShowWindowAsync`; a write-ahead state file guarantees recovery. The user drives with a Logitech G502 (wheel + wheel tilt) on 2 monitors and wants to stay on the mouse.

## Goals / Non-Goals

**Goals:**
- Mouse-only slot control while `Ctrl+Alt` is held: wheel/tilt switch, tile click jumps, drag-to-assign.
- See what's in every slot on every monitor at a glance.
- Keep every v0.1 guarantee (never lose a window, one slot per window, write-ahead persistence, no blocking on other apps).
- The mouse must never lag because of us.

**Non-Goals:**
- Tile-to-tile dragging, previews/icons in tiles, configurable bindings, animations.

## Decisions

### 1. Hooks on a dedicated thread
`WH_KEYBOARD_LL` and `WH_MOUSE_LL` run on the thread that installed them, inside its message loop. If that were the main thread, every switch (EnumWindows, file write) would stall the system-wide mouse, and Windows silently drops hooks that exceed `LowLevelHooksTimeout`. So a dedicated hook thread installs both hooks and runs its own `GetMessage` loop. Hook procs only read/write a few atomics, `PostMessageW` to the app window, and return — no allocation, no locks, no Win32 calls on other windows.
*Alternative:* `RegisterRawInput` — can't swallow events, so Ctrl+Alt+wheel would still zoom apps. Rejected.

### 2. The chord
Keyboard hook tracks physical Ctrl and Alt (either side) and a `tainted` flag set when any other key goes down while either is held; cleared when both are released. `CHORD` (atomic bool) = Ctrl && Alt && !tainted. Transitions post `WM_CHORD_DOWN` / `WM_CHORD_UP`. Keys are never swallowed, so `Ctrl+Alt+←/→` hotkeys and other apps' shortcuts are unaffected.
- **Panel:** main thread starts a 250 ms timer on `WM_CHORD_DOWN`; when it fires and `CHORD` is still true, panels are shown and `PANEL` (atomic) set. `WM_CHORD_UP` hides panels, cancels any drag, clears `PANEL`.
- **AltGr** (non-US layouts) arrives as LCtrl+RAlt; typing with it presses another key quickly, which taints the chord. Holding AltGr alone for 250 ms would show the panel — acceptable.

### 3. Mouse handling
While `CHORD` is true the mouse hook swallows (returns 1) and posts: `WM_MOUSEWHEEL`, `WM_MOUSEHWHEEL`, `WM_LBUTTONDOWN`, `WM_LBUTTONUP`. `WM_MOUSEMOVE` is never swallowed; it is posted only while `DRAG` (atomic) is true. All other buttons pass through. The posted message carries the screen point and wheel delta.
- **Wheel:** main thread accumulates delta per gesture; each ±120 = one switch on the monitor under the point. Up (positive) = next, down = previous. Accumulator resets on `WM_CHORD_UP`.
- **Tilt:** horizontal wheel repeats while held. One switch per tilt *burst*: switch on the first event, ignore further same-direction events until none arrive for 400 ms. Right = next, left = previous.
- Switches reuse v0.1's `on_switch` path (write-ahead, hide, show, focus, persist), so the race correction from v0.1 applies unchanged.

### 4. Panels (display only)
One panel window per monitor: `WS_POPUP`, `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED` (alpha ~235), centered horizontally near the bottom of the monitor's work area. 4 tiles; each shows the slot number and up to 5 window titles (GDI, ellipsized), "—" if empty. Current slot outlined; the drop-target tile highlighted during a drag.
Panels are click-through and never take focus: **all interaction comes via the mouse hook**, and the main thread hit-tests the posted point against the panel tile rects it laid out. Panels are recreated/re-laid-out on `WM_DISPLAYCHANGE` and repainted after any switch or move while visible.
Titles: hidden windows via `GetWindowTextW` (reads the stored caption for other processes; doesn't send `WM_GETTEXT`, so it can't hang); the active slot via `visible_windows_on`. Titles are read at paint time, never persisted.

### 5. Click to jump
`WM_LBUTTONDOWN` over a tile records it; `WM_LBUTTONUP` over the same tile = jump. Core gains `switch_to(monitor, slot, visible, now_ms) -> Plan`; `switch(dir)` becomes a thin wrapper computing the target. Clicking the current slot is a no-op.

### 6. Drag to assign
- `WM_LBUTTONDOWN` not over a tile: `WindowFromPoint` → `GetAncestor(GA_ROOT)` → if it participates (v0.1 filter, its root owner), start a drag: set `DRAG`, show the ghost. Otherwise ignore (the click is still swallowed while the chord is held).
- **Ghost:** a top-level `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT` popup with a `DwmRegisterThumbnail` of the dragged window, opacity ~60%, sized to fit 360×240 keeping aspect ratio, offset so the cursor sits at its top-left quarter. Moved on each posted `WM_MOUSEMOVE` (`SetWindowPos` with `SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER`). The real window does not move.
- **Drop** (`WM_LBUTTONUP`): over a tile → move (§7); anywhere else → cancel. `WM_CHORD_UP` mid-drag → cancel. Ghost thumbnail unregistered and hidden in all cases.
- Dragging an owned dialog drags its root owner (whole app moves together, consistent with v0.1).

### 7. Moving a window to a slot
Core gains `move_to_slot(entry, to_monitor, to_slot) -> bool` (true = must be hidden): removes the id from every slot and from every monitor's last plan (so the rapid-press correction can't resurrect it into the wrong slot), then pushes it to the top of `to_slot` unless that is `to_monitor`'s active slot. Owned windows of the dragged root move with it (the shell passes them as entries too).
Shell sequence, reusing write-ahead:
1. `old = hidden()`; core update; `new = hidden()`; write `old ∪ new`.
2. If target monitor ≠ window's monitor: relocate via `GetWindowPlacement` → shift `rcNormalPosition` by the offset between the two monitors' work areas, clamp into the target work area → `SetWindowPlacement` with `WPF_ASYNCWINDOWPLACEMENT` and the current show state (keeps maximized/minimized).
3. If must hide: `ShowWindowAsync(SW_HIDE)` — posted after the placement, so the target thread processes them in order.
4. Write `new`. Repaint panels. If the moved window had focus, run `focus_after_switch(&[], moved)`.

| Drop on | Effect |
|---|---|
| same monitor, active slot | no-op |
| same monitor, other slot | hide, record in slot |
| other monitor, active slot | relocate, stays visible |
| other monitor, other slot | relocate, hide, record in slot |

### 8. Indicator
`show_osd` is skipped while `PANEL` is true; arrow-hotkey switches without the chord held still show "N / 4". (When the arrows are pressed, Ctrl+Alt are held, so the panel may already be up; then the highlight serves as the indicator.)

### 9. Threading and shutdown
Main ↔ hook thread share only atomics (`CHORD`, `PANEL`, `DRAG`) and the app `HWND` (stored once before the hook thread starts). On Exit/`WM_QUERYENDSESSION`, main posts `WM_QUIT` to the hook thread (`PostThreadMessageW`) and joins it; hooks are unhooked by that thread. The panic hook is unchanged (file-only restore); a panic on the hook thread is caught by the same global hook.

## Risks / Trade-offs

- [Ctrl+Alt+left-click is swallowed while the chord is held, even on things that aren't windows] → intended; it only happens while deliberately holding the chord.
- [Logitech G HUB may remap tilt] → the debug log prints every posted mouse message; adjust mapping if tilt doesn't arrive as `WM_MOUSEHWHEEL`.
- [DWM thumbnail opacity vs. our window's own background] → the ghost window is sized exactly to the thumbnail; if the background still shows, spike 1.1 decides between `DwmExtendFrameIntoClientArea` and a `PrintWindow` snapshot with `UpdateLayeredWindow`.
- [Placement then hide ordering across a cross-monitor drop] → both are posted to the target thread in order; spike 1.2 confirms the window hides on the new monitor and reappears there.
- [Elevated windows] → hooks still see the mouse, but hiding/relocating them fails silently as in v0.1 — they simply don't move.
- [Hook removed by Windows after a stall] → hooks do no work, so this shouldn't happen; if `CHORD` never triggers after a long system hang, restarting the app fixes it. Acceptable for v0.2.

## Open Questions

- Panel placement: bottom-center of each monitor feels right; revisit after use.
- Does the ghost look right on mixed-DPI drags between monitors? (Thumbnail size is in physical pixels; probably fine.)
