## ADDED Requirements

### Requirement: Each monitor has independent slots
The system SHALL maintain 4 slots per connected monitor, each monitor with its own active slot index starting at slot 1. Switching slots on one monitor MUST NOT hide, show, or move windows on any other monitor.

#### Scenario: Switching one monitor leaves the other alone
- **WHEN** monitor A is on slot 1 with window X and monitor B is on slot 1 with window Y, and the user switches monitor A to slot 2
- **THEN** window X is hidden and window Y remains visible and unchanged

### Requirement: Hotkey switches the monitor under the cursor
The system SHALL register `Ctrl+Alt+Right` to move to the next slot and `Ctrl+Alt+Left` to move to the previous slot, applied to the monitor containing the mouse cursor at the time of the keypress.

#### Scenario: Next slot on cursor monitor
- **WHEN** the cursor is on monitor B (slot 1) and the user presses `Ctrl+Alt+Right`
- **THEN** monitor B switches to slot 2

#### Scenario: Hotkey registration fails
- **WHEN** another application already owns `Ctrl+Alt+Left` or `Ctrl+Alt+Right` at startup
- **THEN** the system shows an error message naming the conflicting hotkey and exits

#### Scenario: Rapid presses
- **WHEN** the user presses `Ctrl+Alt+Right` three times quickly on a monitor at slot 1
- **THEN** the monitor ends on slot 4, and pressing `Ctrl+Alt+Left` three times quickly returns to slot 1 with exactly the windows it had before

### Requirement: Slots clamp at the ends
The system SHALL NOT wrap around: switching previous from slot 1 or next from slot 4 MUST do nothing.

#### Scenario: Previous from first slot
- **WHEN** a monitor is on slot 1 and the user presses `Ctrl+Alt+Left` on it
- **THEN** no windows are hidden or shown and the active slot remains 1

### Requirement: Slot contents are captured at switch time
When leaving a slot, the system SHALL record every participating window currently visible on that monitor, in z-order, as that slot's contents, replacing what was previously recorded for it.

#### Scenario: Newly opened window joins current slot
- **WHEN** the user opens a new window on monitor A while on slot 1, then switches to slot 2 and back to slot 1
- **THEN** the new window is visible again on slot 1

#### Scenario: Window dragged to another monitor
- **WHEN** window X was in monitor A's slot 1, the user drags it to monitor B, and then switches monitor A to slot 2
- **THEN** window X stays visible on monitor B

### Requirement: A window belongs to at most one slot
The system SHALL ensure a window is recorded in at most one slot across all monitors; capturing a slot's contents MUST remove those windows from every other slot.

#### Scenario: App re-shows its own hidden window on another monitor
- **WHEN** window X is hidden in monitor A's inactive slot 1, X's app shows it again and the user moves it to monitor B, B switches to slot 2 (capturing X into B's slot 1), and then A switches back to slot 1
- **THEN** window X stays hidden with monitor B's slot 1 and does not reappear

### Requirement: Switching hides current windows and shows target windows
The system SHALL hide the current slot's windows so they do not appear on the screen or the taskbar, and SHALL show the target slot's windows in their original z-order, skipping any window that no longer exists or whose process or class no longer matches what was recorded.

#### Scenario: Round trip restores windows
- **WHEN** slot 1 has windows X (top) and Y, the user switches to slot 2 and back to slot 1
- **THEN** X and Y are visible, X is above Y, and neither appeared on the taskbar while on slot 2

#### Scenario: Window closed while hidden
- **WHEN** a window in an inactive slot is closed by its process, and the user switches to that slot
- **THEN** the remaining windows are shown and no error occurs

#### Scenario: Empty target slot
- **WHEN** the user switches to a slot with no windows
- **THEN** the monitor shows only the desktop

#### Scenario: Hung application
- **WHEN** a window in the current slot belongs to an application that is not responding
- **THEN** the switch completes without the tool freezing, and the hung window is left visible

#### Scenario: Window that cannot be hidden
- **WHEN** a window in the current slot cannot be hidden (e.g. it belongs to an elevated process)
- **THEN** it stays visible, and switching back to its slot does not affect it

### Requirement: Dialogs move with their owner
Visible windows owned by a participating window (dialogs, popups) SHALL be hidden and shown together with their root owner, on the owner's monitor.

#### Scenario: Open dialog is hidden with its app
- **WHEN** an app on monitor A has a "Save As" dialog open and the user switches monitor A to slot 2
- **THEN** both the app window and the dialog are hidden, and both reappear when returning to slot 1

### Requirement: Keyboard focus never stays on a hidden window
After each switch the system SHALL move focus to the topmost restored window; if no window was restored, focus SHALL move to the topmost visible participating window on any monitor, or to the desktop if there is none. The system MUST NOT focus a window that lives on a different native virtual desktop.

#### Scenario: Round trip focus
- **WHEN** slot 1 has windows X (top) and Y and the user returns to slot 1
- **THEN** X has keyboard focus

#### Scenario: Switch to empty slot
- **WHEN** the focused window is on monitor A and the user switches A to an empty slot while monitor B shows window Z
- **THEN** Z has keyboard focus and typing does not reach any hidden window

### Requirement: Only real application windows participate
The system SHALL only manage top-level, visible, unowned windows (plus their owned windows) that are not tool windows, not DWM-cloaked, not shell windows (desktop, taskbars), and not owned by this application. Windows SHALL be assigned to the monitor they overlap most; minimized windows to the monitor of their restored position. Assignment MUST be correct on monitors with different DPI scaling.

#### Scenario: Taskbar is never hidden
- **WHEN** the user switches slots on the monitor containing the taskbar
- **THEN** the taskbar remains visible

#### Scenario: Minimized window belongs to its monitor
- **WHEN** a window restored on monitor B is minimized and the user switches monitor B to another slot
- **THEN** that window is hidden as part of monitor B's slot

#### Scenario: Window straddling both monitors
- **WHEN** a window overlaps both monitors with most of its area on monitor A
- **THEN** it belongs to monitor A

### Requirement: Slot indicator
After each successful switch the system SHALL display the new slot as "N / 4" centered on the affected monitor for about 600 ms, without taking focus or intercepting mouse input.

#### Scenario: Indicator on switch
- **WHEN** the user switches monitor A to slot 3
- **THEN** "3 / 4" briefly appears on monitor A and focus remains on the restored window

### Requirement: Monitor changes reset slots
When the set of connected monitors changes, the system SHALL restore all hidden windows and reset every monitor to slot 1. Display changes that leave the set of monitors intact (e.g. resolution or scaling) MUST NOT reset slots. Any window restored to a position no longer on any monitor SHALL be moved onto the primary monitor.

#### Scenario: Monitor unplugged
- **WHEN** monitor B is disconnected while it has windows hidden in slot 2
- **THEN** those windows become visible on a connected monitor and all monitors are on slot 1

#### Scenario: Resolution change
- **WHEN** the user changes monitor A's scaling while monitor B has windows hidden
- **THEN** those windows stay hidden and both monitors keep their active slots
