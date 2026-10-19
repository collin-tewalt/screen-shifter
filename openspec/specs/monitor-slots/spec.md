# monitor-slots Specification

## Purpose
TBD - created by archiving change per-monitor-slots-v1. Update Purpose after archive.
## Requirements
### Requirement: Each monitor has independent slots
The system SHALL maintain an ordered list of slots per connected monitor, starting with one slot, each monitor with its own active slot. A monitor SHALL have at most 8 slots. Switching slots on one monitor MUST NOT hide, show, or move windows on any other monitor, and MUST NOT change any other monitor's slots, except removing a slot left empty because its windows were captured elsewhere.

#### Scenario: Switching one monitor leaves the other alone
- **WHEN** monitor A is on slot 1 of 2 with window X and monitor B is on slot 1 with window Y, and the user switches monitor A to slot 2
- **THEN** window X is hidden and window Y remains visible and unchanged

#### Scenario: Fresh start
- **WHEN** the app starts with no saved layout
- **THEN** every monitor has exactly one slot and is on it

### Requirement: Hotkey switches the monitor under the cursor
The system SHALL register `Ctrl+Alt+Right` to move to the next slot and `Ctrl+Alt+Left` to move to the previous slot, applied to the monitor containing the mouse cursor at the time of the keypress.

#### Scenario: Next slot on cursor monitor
- **WHEN** the cursor is on monitor B (slot 1 of 2) and the user presses `Ctrl+Alt+Right`
- **THEN** monitor B switches to slot 2

#### Scenario: Hotkey registration fails
- **WHEN** another application already owns `Ctrl+Alt+Left` or `Ctrl+Alt+Right` at startup
- **THEN** the system shows an error message naming the conflicting hotkey and exits

#### Scenario: Rapid presses
- **WHEN** a monitor is on slot 1 of 4, every slot holds windows, and the user presses `Ctrl+Alt+Right` three times quickly
- **THEN** the monitor ends on slot 4, and pressing `Ctrl+Alt+Left` three times quickly returns to slot 1 with exactly the windows it had before

### Requirement: Slots clamp at the ends
The system SHALL NOT wrap around and SHALL NOT create slots when switching by direction: switching previous from the first slot or next from the last slot MUST do nothing.

#### Scenario: Previous from first slot
- **WHEN** a monitor is on slot 1 and the user presses `Ctrl+Alt+Left` on it
- **THEN** no windows are hidden or shown and the active slot remains 1

#### Scenario: Next from last slot
- **WHEN** a monitor is on slot 2 of 2 and the user presses `Ctrl+Alt+Right` on it
- **THEN** no windows are hidden or shown, no slot is created, and the active slot remains 2

### Requirement: Slot contents are captured at switch time
When leaving a slot, the system SHALL record every participating window currently visible on that monitor, in z-order, as that slot's contents, replacing what was previously recorded for it.

#### Scenario: Newly opened window joins current slot
- **WHEN** the user opens a new window on monitor A while on slot 1, then switches to slot 2 and back to slot 1
- **THEN** the new window is visible again on slot 1

#### Scenario: Window dragged to another monitor
- **WHEN** window X was in monitor A's slot 1, the user drags it to monitor B, and then switches monitor A to slot 2
- **THEN** window X stays visible on monitor B

### Requirement: A window belongs to at most one slot
The system SHALL ensure a window is recorded in at most one slot across all monitors; capturing a slot's contents MUST remove those windows from every other slot, and a slot emptied this way is removed.

#### Scenario: App re-shows its own hidden window on another monitor
- **WHEN** window X is the only window hidden in monitor A's inactive slot 1 (A on slot 2 of 2), X's app shows it again and the user moves it to monitor B, and B switches to slot 2 (capturing X into B's slot 1)
- **THEN** A's slot 1 is removed, A has one slot, and X stays hidden with monitor B's slot 1

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
After each successful switch the system SHALL display the new slot as "N / M", where M is that monitor's slot count after the switch, centered on the affected monitor for about 600 ms, without taking focus or intercepting mouse input. While the slot panel is visible, the indicator SHALL NOT be shown; the panel's highlight shows the current slot instead.

#### Scenario: Indicator on switch
- **WHEN** monitor A has 3 slots and the user switches it to slot 3
- **THEN** "3 / 3" briefly appears on monitor A and focus remains on the restored window

#### Scenario: No indicator while the panel is visible
- **WHEN** the panel is visible and the user switches monitor A with the wheel
- **THEN** no "N / M" indicator appears and A's panel highlights the new slot

### Requirement: Monitor changes reset slots
When the set of connected monitors changes, the system SHALL restore all hidden windows, forget the saved layout, and reset every monitor to a single slot. Display changes that leave the set of monitors intact (e.g. resolution or scaling) MUST NOT reset slots. Any window restored to a position no longer on any monitor SHALL be moved onto the primary monitor.

#### Scenario: Monitor unplugged
- **WHEN** monitor B is disconnected while it has windows hidden in slot 2
- **THEN** those windows become visible on a connected monitor and every monitor has one slot

#### Scenario: Resolution change
- **WHEN** the user changes monitor A's scaling while monitor B has windows hidden
- **THEN** those windows stay hidden and both monitors keep their slots and active slots

### Requirement: A window can be moved into any slot on any monitor
The system SHALL support moving a managed window (with its owned windows) into a given existing slot of a given monitor, or into a new slot created at the end of that monitor. The window MUST be removed from any slot it was recorded in, and a slot left empty by that removal is removed. If the target is another monitor, the window SHALL be moved onto that monitor keeping its minimized/maximized state. If the target slot is not that monitor's current slot, the window SHALL be hidden and recorded in the target slot, persisted before hiding exactly as for a switch. A target slot that no longer exists SHALL be ignored with no change.

#### Scenario: Move to a hidden slot on the same monitor
- **WHEN** monitor A has 3 slots and window X is visible on A (slot 1) and is moved to A's slot 3
- **THEN** X is hidden, recorded only in A's slot 3, and listed in the state file

#### Scenario: Move to another monitor's current slot
- **WHEN** maximized window X on monitor A is moved to monitor B's current slot
- **THEN** X is visible and maximized on monitor B and is not recorded in any slot

#### Scenario: Move to another monitor's hidden slot
- **WHEN** window X on monitor A is moved to monitor B's slot 2 while B is on slot 1 of 2, and then B switches to slot 2
- **THEN** X appears on monitor B

#### Scenario: Quick switch after a move
- **WHEN** monitor A has 4 slots, window X was shown by a switch on A, is moved to A's slot 4 within a second, and A is then switched again within a second
- **THEN** X remains recorded only in A's slot 4

#### Scenario: Quick switch on the target monitor after a cross-monitor move
- **WHEN** window X is moved from monitor A to monitor B's current slot, and B is switched away within a second
- **THEN** X is hidden with the B slot it was on, not left visible or lost

#### Scenario: Stale target
- **WHEN** a drop targets monitor B's slot 3 but B's slot 3 was removed a moment earlier
- **THEN** nothing is moved, hidden, or relocated

### Requirement: Slots can be created at the end
The system SHALL let the user create a slot on a monitor that has fewer than 8 slots, appended after its last slot. Creating by switching SHALL leave the monitor on the new, empty slot. Creating by moving a window SHALL put that window (with its owned windows) in the new slot, hidden, and leave the active slot unchanged.

#### Scenario: Create and switch
- **WHEN** monitor A has 2 slots, is on slot 1, and the user creates a slot by switching
- **THEN** monitor A has 3 slots, is on slot 3, slot 1's windows are hidden, and the monitor shows only the desktop

#### Scenario: Create by moving a window
- **WHEN** monitor A has 1 slot with windows X and Y and the user moves X into a new slot on A
- **THEN** A has 2 slots, A is still on slot 1 showing Y, and X is hidden in slot 2

#### Scenario: Limit
- **WHEN** a monitor has 8 slots
- **THEN** no further slot can be created on it

### Requirement: Empty slots are removed
The system SHALL remove any slot that is empty and is not the monitor's active slot, renumbering the slots after it, whenever slot contents change. In particular, leaving a slot with no windows SHALL remove it. The slide animation direction SHALL follow the direction the user moved, regardless of renumbering.

#### Scenario: Leaving an empty slot
- **WHEN** monitor A has slots 1 (window X), 2 (empty, active), and 3 (window Y), and the user switches next
- **THEN** A has 2 slots, is on slot 2 showing Y, slot 1 still holds X, and the old screen slides off to the left

#### Scenario: Slot emptied by a move
- **WHEN** monitor A's hidden slot 2 holds only window X and X is moved to monitor B's current slot
- **THEN** A's slot 2 is removed and later slots renumber

#### Scenario: Active empty slot stays
- **WHEN** the user creates a slot and is on it with no windows
- **THEN** the slot remains until the user leaves it

### Requirement: A slot can be closed, folding its windows into a neighbor
Closing slot `i` of a monitor with two or more slots SHALL remove it and move its windows into the slot to its left, or into the slot to its right if `i` is the first. The slots after it SHALL renumber. Closing SHALL never close, minimize, or hide any window, and a monitor SHALL always keep at least one slot (closing the only slot does nothing). If the receiving slot is the monitor's current slot, the folded windows SHALL appear on screen. If the closed slot was the monitor's current slot, its windows SHALL stay on screen, the receiving neighbor SHALL become the current slot, and the neighbor's hidden windows SHALL be shown with them. The state file SHALL be written before any window is shown. No empty slot other than the current one SHALL result.

#### Scenario: Close a hidden slot next to the current slot
- **WHEN** a monitor has slots 1, 2, 3 (slot 1 current) with windows hidden in 2 and 3, and slot 2 is closed
- **THEN** slot 2's windows appear on screen (slot 1 is its left neighbor and is current), and old slot 3 is now slot 2

#### Scenario: Close a hidden slot next to another hidden slot
- **WHEN** a monitor has slots 1, 2, 3 (slot 1 current) and slot 3 is closed
- **THEN** slot 3's windows move into slot 2, still hidden, and the monitor has two slots

#### Scenario: Close the leftmost slot folds right
- **WHEN** a monitor has slots 1, 2 (slot 2 current, slot 1 holding hidden windows) and slot 1 is closed
- **THEN** slot 1's windows appear on screen with slot 2's, and the monitor has one slot

#### Scenario: Close the current slot
- **WHEN** a monitor has slots 1, 2 (slot 2 current, showing windows A and B) with window C hidden in slot 1, and slot 2 is closed
- **THEN** A and B stay where they are, C appears, and slot 1 is the only slot

#### Scenario: Last slot cannot be closed
- **WHEN** a monitor has one slot and a close of it is requested
- **THEN** nothing changes

#### Scenario: Restart after closing
- **WHEN** a slot is closed and the app is closed or killed immediately afterwards
- **THEN** on the next start every window the state file lists is shown or placed in a slot, none left hidden

### Requirement: A closed window is forgotten from its slot
Whenever the system notices that a window recorded in a hidden slot no longer exists (when a close requested from the exposé finishes, and when the panel opens), it SHALL remove that window, and any recorded owned windows that no longer exist, from its slot and from the state file. A hidden slot this empties SHALL be removed like any empty slot. A window that still exists MUST NOT be removed, wherever it now is. Removing dead windows MUST NOT affect any other entry.

#### Scenario: Last window closed
- **WHEN** slot 3 holds one hidden window and it is closed from its preview
- **THEN** slot 3 is removed and the slots after it renumber

#### Scenario: Other windows remain
- **WHEN** a hidden slot holds two windows and one is closed
- **THEN** the slot holds the other and keeps its number

#### Scenario: A pending close and a switch
- **WHEN** a close was requested for a window, and before it finishes the monitor switches slots so the window is hidden
- **THEN** the window stays recorded in its slot until it no longer exists

