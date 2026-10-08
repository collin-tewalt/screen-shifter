# window-recovery Specification

## Purpose
TBD - created by archiving change per-monitor-slots-v1. Update Purpose after archive.
## Requirements
### Requirement: Hidden windows are persisted before they are hidden
On every switch or move, before hiding or showing anything, the system SHALL write to the state file (`%LOCALAPPDATA%\screen-shifter-pkg\hidden.txt` when running as the installed package, `%LOCALAPPDATA%\screen-shifter\hidden.txt` otherwise, so the two never share a file) every window that is currently hidden or about to be hidden, including enough identity (window handle, process id, window class, minimized state) to verify each entry later, together with the layout: each hidden window's monitor and slot, and each monitor's active slot. Windows about to be shown SHALL be recorded as belonging to their monitor's active slot. After the operation completes it SHALL rewrite the file with exactly the windows that remain hidden and the current layout. The file MUST be replaced atomically so it is never partially written.

#### Scenario: State file reflects hidden windows
- **WHEN** the user switches away from monitor A's slot 1 containing windows X and Y to slot 2
- **THEN** the state file lists X and Y in A's slot 1 and A's active slot as 2

#### Scenario: Crash between persisting and hiding
- **WHEN** the process dies after writing the state file but before all windows are hidden, and the app is started again
- **THEN** every window being hidden ends up hidden in its new slot, every window being shown is visible, and no window is lost

#### Scenario: Crash before the target slot is shown
- **WHEN** the user switches from slot 1 to slot 2 (which holds hidden window Z), and the process dies after hiding slot 1 but before showing Z, and the app is started again
- **THEN** Z is visible, slot 1's windows are hidden in slot 1, and the monitor is on slot 2

#### Scenario: Crash while writing the file
- **WHEN** the process dies while writing the state file
- **THEN** the previous complete state file remains intact

### Requirement: Restore on startup
On startup the system SHALL read the state file and, for each listed window that still exists and whose process id and class still match: if its recorded monitor is connected, the window is on that monitor, and its recorded slot is not that monitor's active slot, the system SHALL take it back into that slot, keeping it hidden (hiding it if visible) without showing it first; otherwise it SHALL show it. Each monitor SHALL return to its recorded active slot, with slots left empty by windows that were not taken back removed. The state file SHALL then describe the resulting state. Before showing or hiding anything at startup, the state file SHALL be rewritten so that every window about to be shown and every window taken back is recorded (the former in its monitor's active slot), so a crash during startup loses nothing. A window taken back while it has keyboard focus SHALL lose focus as after a switch. Monitors that are still connected keep their layout even if other recorded monitors are gone. A window and its owned windows SHALL be taken back or shown together. Windows SHALL NOT be taken back if Windows has restarted since the file was written. Windows SHALL be taken back only once the app has fully started; if startup fails, every listed window SHALL be shown and the file kept. A state file from an earlier version without layout SHALL have every valid window shown.

#### Scenario: Restart keeps the layout
- **WHEN** monitor A was on slot 2 of 3 with X hidden in slot 1 and Y in slot 3, the user chooses Exit, and starts the app again
- **THEN** X and Y are hidden again in slots 1 and 3, A is on slot 2 of 3, and switching to slot 1 shows X

#### Scenario: Recovery after crash
- **WHEN** the app was killed while windows X and Y were hidden in monitor A's slot 2, and the app is started again
- **THEN** X and Y stay hidden in slot 2 without appearing on screen

#### Scenario: Window moved while the app was off
- **WHEN** X was hidden in monitor A's slot 2, the user chose Exit, dragged X to monitor B, and started the app again
- **THEN** X stays visible on monitor B

#### Scenario: Window closed while the app was off
- **WHEN** X was the only window in monitor A's slot 3, the user chose Exit, closed X, and started the app again
- **THEN** A has no slot for X and the remaining slots keep their order

#### Scenario: Recovery after a monitor was removed
- **WHEN** the app crashed while window X was hidden on monitor B, monitor B is then disconnected, and the app is started again
- **THEN** X becomes visible on a connected monitor with its minimized/maximized state preserved

#### Scenario: Crash during startup
- **WHEN** the app starts, rewrites the state file, and dies before showing the hidden windows of a disconnected monitor, and is started again
- **THEN** those windows become visible and the kept layout is unchanged

#### Scenario: Startup fails
- **WHEN** windows are saved in hidden slots and the app is started while another application owns `Ctrl+Alt+Left`
- **THEN** the error is shown and every saved window is visible after the app exits

#### Scenario: After a reboot
- **WHEN** the state file was written before Windows restarted
- **THEN** no window is taken back

#### Scenario: Reused handle is not shown
- **WHEN** a listed handle now belongs to a different process or window class
- **THEN** that window is left untouched

#### Scenario: Unreadable state file
- **WHEN** the state file has an unknown version or malformed lines
- **THEN** the app starts normally, uses the lines it can parse, and logs the rest

### Requirement: Restore on abnormal termination
The system SHALL show all hidden windows when it panics, and when the user logs off or Windows shuts down, before the session ends, keeping the saved layout for the next start.

#### Scenario: Panic
- **WHEN** a bug causes the app to panic while windows are hidden
- **THEN** all hidden windows are visible before the process terminates, and the next start takes them back into their slots

#### Scenario: Logoff with unsaved work
- **WHEN** a hidden window has unsaved changes and the user logs off
- **THEN** the window is visible so its "save changes?" prompt can be seen

#### Scenario: Logoff cancelled
- **WHEN** the user cancels the logoff after windows were shown
- **THEN** every monitor has one slot and the hotkeys keep working normally (the saved layout is replaced at the next switch; restarting before then takes the old layout back)

### Requirement: Single instance
The system SHALL allow only one running instance; a second launch MUST exit immediately without touching any windows or the state file.

#### Scenario: Launched twice
- **WHEN** the app is running with windows hidden and the user launches it again
- **THEN** the second instance exits and the hidden windows remain hidden and recoverable by the first

### Requirement: Restore on exit
When the user chooses Exit, the system SHALL show all windows it has hidden, keep the saved layout for the next start, unregister its hotkeys, and remove its tray icon before terminating.

#### Scenario: Clean exit
- **WHEN** windows are hidden in inactive slots and the user chooses Exit
- **THEN** all those windows are visible after the app closes

### Requirement: Tray icon with recovery controls
The system SHALL show a notification-area icon whose context menu offers "Restore all", "Start with Windows", and "Exit". "Restore all" SHALL show every hidden window, forget the saved layout, and reset every monitor to a single slot while the app keeps running. The icon MUST reappear if Explorer restarts.

#### Scenario: Restore all
- **WHEN** windows are hidden on both monitors and the user chooses "Restore all"
- **THEN** all hidden windows become visible, each monitor has one slot, the hotkeys still work, and a later restart takes nothing back

#### Scenario: Explorer restarts
- **WHEN** Explorer crashes or is restarted while the app is running
- **THEN** the tray icon reappears with its menu working

### Requirement: Closed by Windows for an update or uninstall
When Windows asks the app to close (to update or remove its package, to end the session, or with a close request to its window), the system SHALL show every hidden window, keep the saved layout, and exit cleanly. It SHALL register to be relaunched after being closed for an update; the relaunched app takes the saved windows back into their slots. The restart registration MUST NOT relaunch the app after a crash or a reboot.

#### Scenario: Update while windows are hidden
- **WHEN** windows are hidden in slots and a newer version of the package is installed while the app runs
- **THEN** either the update is refused until the app exits (the windows stay in their slots), or the windows become visible, the app closes, the new version starts by itself, and the windows return to their slots (unless Windows restarted in between)

#### Scenario: Close request
- **WHEN** windows are hidden in slots and something sends the app's window a close request (`WM_CLOSE`)
- **THEN** the windows become visible and the app exits

#### Scenario: Uninstall while windows are hidden
- **WHEN** windows are hidden in slots and the package is uninstalled while the app runs (from Settings or PowerShell)
- **THEN** all those windows are visible after the app is gone

#### Scenario: Stopped without being asked
- **WHEN** the app is terminated without any close request (e.g. `taskkill /f`, Task Manager's End task)
- **THEN** its windows stay hidden until the app runs again, which takes them back

#### Scenario: No restart loop
- **WHEN** the app crashes
- **THEN** the restart registration does not relaunch it

