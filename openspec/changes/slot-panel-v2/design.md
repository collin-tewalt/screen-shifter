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
`WH_KEYBOARD_LL` and `WH_MOUSE_LL` run on the thread that installed them, inside its message loop. The hook thread runs at `THREAD_PRIORITY_TIME_CRITICAL` (it does almost no work) so CPU load elsewhere can't delay input. If that were the main thread, every switch (EnumWindows, file write) would stall the system-wide mouse, and Windows silently drops hooks that exceed `LowLevelHooksTimeout`. So a dedicated hook thread installs both hooks and runs its own `GetMessage` loop. Hook procs only read/write a few atomics, `PostMessageW` to the app window, and return — no allocation, no locks, no Win32 calls on other windows.
*Alternative:* `RegisterRawInput` — can't swallow events, so Ctrl+Alt+wheel would still zoom apps. Rejected.

### 2. The chord
Keyboard hook tracks physical Ctrl and Alt (either side) and a `tainted` flag set when any other key goes down while either is held; cleared when both are released. `VK_LEFT`/`VK_RIGHT` do **not** taint (so arrow switching keeps the panel up), and injected keys (`LLKHF_INJECTED`, e.g. our own §10 focus nudge) are ignored. `CHORD` (atomic bool) = Ctrl && Alt && !tainted. Transitions post `WM_CHORD_DOWN` / `WM_CHORD_UP`. Keys are never swallowed, so `Ctrl+Alt+←/→` hotkeys and other apps' shortcuts are unaffected.
- **Panel:** main thread starts a 250 ms timer on `WM_CHORD_DOWN`; when it fires and `CHORD` is still true, panels are shown and `PANEL` (atomic) set. `WM_CHORD_UP` hides panels, cancels any drag, clears `PANEL`.
- **AltGr** (non-US layouts) arrives as LCtrl+RAlt; typing with it presses another key quickly, which taints the chord. Holding AltGr alone for 250 ms would show the panel — acceptable.

### 3. Mouse handling
While `CHORD` is true the mouse hook swallows (returns 1) and posts: `WM_MOUSEWHEEL`, `WM_MOUSEHWHEEL`, `WM_LBUTTONDOWN`. `WM_MOUSEMOVE` is never swallowed. All other buttons pass through. The posted message carries the screen point and wheel delta.
- **Stuck-chord guard (must-have):** the LL keyboard hook misses key-ups on the secure desktop (Ctrl+Alt+Del, UAC, lock screen), while an elevated window has focus, or if another hook eats them. So on *every* mouse event while `CHORD` is set, the mouse hook checks `GetAsyncKeyState(VK_CONTROL) < 0 && GetAsyncKeyState(VK_MENU) < 0`; if either is up it clears `CHORD` **and the keyboard hook's own Ctrl/Alt-down flags** (atomics shared by both hooks), posts `WM_CHORD_UP`, and passes the event through, except that the pairing rule below takes precedence for a `WM_LBUTTONUP`. Errs toward "not held", the safe side.
- **Paired button swallowing:** a `SWALLOWED_DOWN` atomic is set when a `WM_LBUTTONDOWN` is swallowed. `WM_LBUTTONUP` is swallowed (and posted) **iff** `SWALLOWED_DOWN` is set, regardless of `CHORD`, then the flag clears. Apps never see an unmatched down or up, including when Ctrl+Alt is pressed or released while the button is held.
- **Move coalescing:** while `DRAG` is set, the hook stores the latest point in an atomic and posts `WM_HOOK_MOVE` only if `MOVE_PENDING` is clear (main clears it when handling), so a 1000 Hz mouse can't flood the queue.
- **Wheel:** main thread accumulates delta per gesture; each ±120 = one switch on the monitor under the point. Up (positive) = next, down = previous. Accumulator resets on `WM_CHORD_UP`.
- **Tilt:** horizontal wheel repeats while held. One switch per tilt *burst*: switch on the first event, ignore further same-direction events until none arrive for 400 ms. Right = next, left = previous.
- Switches reuse v0.1's `on_switch` path (write-ahead, hide, show, focus, persist), so the race correction from v0.1 applies unchanged.
- **No switching mid-drag:** while `DRAG` is set, wheel, tilt, tile clicks and arrow hotkeys are ignored, so the dragged window can't be hidden under the drag.

### 4. Panels (display only)
One panel window per monitor: `WS_POPUP`, `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED` (alpha ~235), centered horizontally near the bottom of the monitor's work area. 4 tiles; each shows the slot number and up to 5 window titles (GDI, ellipsized), "—" if empty. Current slot outlined; the drop-target tile highlighted during a drag.
Panels are click-through and never take focus: **all interaction comes via the mouse hook**, and the main thread hit-tests the posted point against the panel tile rects it laid out. Panels are recreated/re-laid-out on `WM_DISPLAYCHANGE` and repainted after any switch or move while visible.
Titles: hidden windows via `GetWindowTextW` (reads the stored caption for other processes; doesn't send `WM_GETTEXT`, so it can't hang); the active slot via `visible_windows_on`. Titles are read at paint time, never persisted.

### 5. Click to jump
Tiles are hit-tested only while `PANEL` is set, against the whole panel rect: a press anywhere on a panel (tile or gap) never starts a drag. `WM_LBUTTONDOWN` over a tile records it; `WM_LBUTTONUP` over the same tile = jump. Hook-driven switches use the point carried in the posted message (not a fresh cursor read) to pick the monitor. Core gains `switch_to(monitor, slot, visible, now_ms) -> Plan`; `switch(dir)` becomes a thin wrapper computing the target. Clicking the current slot is a no-op.

### 6. Drag to assign
- `WM_LBUTTONDOWN` not over a tile: `WindowFromPoint` → `GetAncestor(GA_ROOTOWNER)` (same as v0.1 `classify`) → if it participates, start a drag carrying the root plus its visible owned windows as entries: set `DRAG`, show the ghost. Otherwise ignore (the click is still swallowed while the chord is held).
- **Ghost:** a top-level `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT` popup with a `DwmRegisterThumbnail` of the dragged window, opacity ~60%, sized to fit 360×240 keeping aspect ratio, offset so the cursor sits at its top-left quarter. Moved on each posted `WM_MOUSEMOVE` (`SetWindowPos` with `SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER`). The real window does not move.
- **Drop** (posted `WM_LBUTTONUP`, always delivered to us when the down was swallowed, see §3): over a tile → move (§7); anywhere else → cancel. `WM_CHORD_UP` mid-drag → cancel (the eventual up is still swallowed by the pairing rule). Ghost thumbnail unregistered and hidden in all cases.
- Dragging an owned dialog drags its root owner (whole app moves together, consistent with v0.1).

### 7. Moving a window to a slot
Core gains `move_to_slot(entries, from_monitor, to_monitor, to_slot, now_ms) -> bool` (true = must be hidden). It removes the ids from every slot and every monitor's last plan, then pushes them to the top of `to_slot` unless that is `to_monitor`'s active slot. The no-op case (target is the window's own monitor's active slot) returns `false` before touching anything. Otherwise it then **feeds the existing race correction** rather than just wiping it, because the async hide/placement won't have landed before a quick next snapshot. A monitor's `last` is first normalized: if it is missing or at least `RACE_WINDOW_MS` old it is replaced by an empty `Plan` (never re-stamp an old plan, or its stale entries would be trusted again); then:
- add the entries to `from_monitor`'s `last.hide` (so a stale snapshot there doesn't recapture them) and set its timestamp to `now_ms` only if the plan was just normalized (a still-fresh plan keeps its timestamp, so old entries aren't trusted for longer);
- if the target is another monitor's active slot, add them to `to_monitor`'s `last.show` (so a stale snapshot there still counts them), with the same timestamp rule.
`entries` is the root plus its owned windows.
Shell sequence, reusing write-ahead:
1. `old = hidden()`; core update; `new = hidden()`; write `old ∪ new`.
2. If target monitor ≠ window's monitor: relocate **every entry** (root and owned windows, each by the same offset) via `GetWindowPlacement` → shift `rcNormalPosition` by the offset between the two monitors' work areas, clamp into the target work area → `SetWindowPlacement` with `WPF_ASYNCWINDOWPLACEMENT` and a **no-activate** show command derived like `bring_on_screen` (`SW_SHOWMINNOACTIVE` / `SW_SHOWMAXIMIZED` / `SW_SHOWNOACTIVATE`), keeping maximized/minimized.
3. If must hide: `ShowWindowAsync(SW_HIDE)` — posted after the placement, so the target thread processes them in order.
4. Write `new`. Repaint panels. If the moved window had focus, run `focus_after_switch(&[], moved)`.

| Drop on | Effect |
|---|---|
| same monitor, active slot | no-op |
| same monitor, other slot | hide, record in slot |
| other monitor, active slot | relocate, stays visible |
| other monitor, other slot | relocate, hide, record in slot |

### 8. Indicator
`show_osd` is skipped while `PANEL` is true. Arrows don't taint the chord (§2), so if the panel is already up during an arrow switch its highlight is the indicator; if the arrow is pressed within the 250 ms before the panel appears, "N / 4" shows as in v0.1.

### 9. Threading and shutdown
Main ↔ hook thread share only atomics (`CHORD`, `PANEL`, `DRAG`) and the app `HWND` (stored once before the hook thread starts). Only on Exit does main post `WM_QUIT` to the hook thread (`PostThreadMessageW`) and join it; hooks are unhooked by that thread. `WM_QUERYENDSESSION` leaves hooks running (a cancelled logoff must keep everything working; process exit removes hooks anyway). The panic hook is unchanged (file-only restore); a panic inside an `extern "system"` hook proc aborts the process after the panic hook has restored windows.

### 10. Foreground rights for hook-driven switches
v0.1's `SetForegroundWindow` works because `WM_HOTKEY` grants the foreground right. Switches triggered from swallowed mouse input may not have it; then focus could stay on a just-hidden window, breaking the baseline focus requirement. Spike 1.3 checks this. If `SetForegroundWindow` fails from a hook-driven switch, `SendInput` a single unassigned virtual key (`0xE8`, unassigned in WinUser.h, unlike F24 which remappers use) down/up immediately before it; the keyboard hook ignores injected keys so the chord isn't tainted. After a move, focus is only fixed when the moved window was focused and became hidden (not for the "other monitor, active slot" case).

## Risks / Trade-offs

- [Ctrl+Alt+left-click is swallowed while the chord is held, even on things that aren't windows] → intended; it only happens while deliberately holding the chord.
- [Logitech G HUB may remap tilt] → the debug log prints every posted mouse message; adjust mapping if tilt doesn't arrive as `WM_MOUSEHWHEEL`.
- [DWM thumbnail opacity vs. our window's own background] → the ghost window is sized exactly to the thumbnail; if the background still shows, spike 1.1 decides between `DwmExtendFrameIntoClientArea` and a `PrintWindow` snapshot with `UpdateLayeredWindow`.
- [`SW_SHOWMAXIMIZED` in a relocation activates the window briefly before it is hidden] → accepted for v0.2; focus is fixed afterwards.
- [Placement then hide ordering across a cross-monitor drop] → both are posted to the target thread in order; spike 1.2 confirms the window hides on the new monitor and reappears there.
- [Elevated windows] → hooks still see the mouse, but hiding/relocating them fails silently as in v0.1 — they simply don't move.
- [Hook removed by Windows after a stall] → hooks do no work, so this shouldn't happen. If the keyboard hook is lost, the §3 guard keeps `CHORD` false so input is never swallowed; the panel simply stops working until restart.

## Open Questions

- Panel placement: bottom-center of each monitor feels right; revisit after use.
- Does the ghost look right on mixed-DPI drags between monitors? (Thumbnail size is in physical pixels; probably fine.)
