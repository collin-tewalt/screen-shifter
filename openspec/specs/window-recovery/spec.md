# window-recovery Specification

## Purpose
TBD - created by archiving change per-monitor-slots-v1. Update Purpose after archive.
## Requirements
### Requirement: Hidden windows are persisted before they are hidden
On every switch, before hiding or showing anything, the system SHALL write to `%LOCALAPPDATA%\screen-shifter\hidden.txt` every window that is currently hidden or about to be hidden, including enough identity (window handle, process id, window class, minimized state) to verify each entry later. After the switch completes it SHALL rewrite the file with exactly the windows that remain hidden. The file MUST be replaced atomically so it is never partially written.

#### Scenario: State file reflects hidden windows
- **WHEN** the user switches away from a slot containing windows X and Y
- **THEN** the state file lists X and Y

#### Scenario: Crash between persisting and hiding
- **WHEN** the process dies after writing the state file but before all windows are hidden, and the app is started again
- **THEN** every window that was hidden is visible again

#### Scenario: Crash before the target slot is shown
- **WHEN** the user switches from slot 1 to slot 2 (which holds hidden window Z), and the process dies after hiding slot 1 but before showing Z, and the app is started again
- **THEN** Z and slot 1's windows are all visible

#### Scenario: Crash while writing the file
- **WHEN** the process dies while writing the state file
- **THEN** the previous complete state file remains intact

### Requirement: Restore on startup
On startup the system SHALL show every window listed in the state file that still exists and whose process id and class still match, then clear the state file.

#### Scenario: Recovery after crash
- **WHEN** the app was killed while windows X and Y were hidden, and the app is started again
- **THEN** X and Y become visible

#### Scenario: Recovery after a monitor was removed
- **WHEN** the app crashed while window X was hidden on monitor B, monitor B is then disconnected, and the app is started again
- **THEN** X becomes visible on a connected monitor with its minimized/maximized state preserved

#### Scenario: Reused handle is not shown
- **WHEN** a listed handle now belongs to a different process or window class
- **THEN** that window is left untouched

#### Scenario: Unreadable state file
- **WHEN** the state file has an unknown version or malformed lines
- **THEN** the app starts normally, restores the lines it can parse, and logs the rest

### Requirement: Restore on abnormal termination
The system SHALL restore all hidden windows when it panics, and when the user logs off or Windows shuts down, before the session ends.

#### Scenario: Panic
- **WHEN** a bug causes the app to panic while windows are hidden
- **THEN** all hidden windows are visible before the process terminates

#### Scenario: Logoff with unsaved work
- **WHEN** a hidden window has unsaved changes and the user logs off
- **THEN** the window is visible so its "save changes?" prompt can be seen

#### Scenario: Logoff cancelled
- **WHEN** the user cancels the logoff after windows were restored
- **THEN** all monitors are on slot 1 and the hotkeys keep working normally

### Requirement: Single instance
The system SHALL allow only one running instance; a second launch MUST exit immediately without touching any windows or the state file.

#### Scenario: Launched twice
- **WHEN** the app is running with windows hidden and the user launches it again
- **THEN** the second instance exits and the hidden windows remain hidden and recoverable by the first

### Requirement: Restore on exit
When the user chooses Exit, the system SHALL show all windows it has hidden, clear the state file, unregister its hotkeys, and remove its tray icon before terminating.

#### Scenario: Clean exit
- **WHEN** windows are hidden in inactive slots and the user chooses Exit
- **THEN** all those windows are visible after the app closes

### Requirement: Tray icon with recovery controls
The system SHALL show a notification-area icon whose context menu offers "Restore all" and "Exit". "Restore all" SHALL show every hidden window and reset all monitors to slot 1 while the app keeps running. The icon MUST reappear if Explorer restarts.

#### Scenario: Restore all
- **WHEN** windows are hidden on both monitors and the user chooses "Restore all"
- **THEN** all hidden windows become visible, both monitors are on slot 1, and the hotkeys still work

#### Scenario: Explorer restarts
- **WHEN** Explorer crashes or is restarted while the app is running
- **THEN** the tray icon reappears with its menu working

