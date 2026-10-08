## MODIFIED Requirements

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

### Requirement: Tray icon with recovery controls
The system SHALL show a notification-area icon whose context menu offers "Restore all", "Start with Windows", and "Exit". "Restore all" SHALL show every hidden window, forget the saved layout, and reset every monitor to a single slot while the app keeps running. The icon MUST reappear if Explorer restarts.

#### Scenario: Restore all
- **WHEN** windows are hidden on both monitors and the user chooses "Restore all"
- **THEN** all hidden windows become visible, each monitor has one slot, the hotkeys still work, and a later restart takes nothing back

#### Scenario: Explorer restarts
- **WHEN** Explorer crashes or is restarted while the app is running
- **THEN** the tray icon reappears with its menu working

## ADDED Requirements

### Requirement: Closed by Windows for an update or uninstall
When Windows asks the app to close (to update or remove its package, to end the session, or with a close request to its window), the system SHALL show every hidden window, keep the saved layout, and exit cleanly. It SHALL register to be relaunched after being closed for an update, where Windows supports that; the next launch, automatic or not, takes the saved windows back into their slots. The restart registration MUST NOT relaunch the app after a crash or a reboot.

#### Scenario: Update while windows are hidden
- **WHEN** windows are hidden in slots and a newer version of the package is installed while the app runs
- **THEN** either the update waits until the app exits, or the windows become visible before the app closes; when the new version starts (by itself if Windows relaunches it, else at the next launch or sign-in) any of those windows that still exist return to their slots (unless Windows restarted in between)

#### Scenario: Close request
- **WHEN** windows are hidden in slots and something sends the app's window a close request (e.g. `taskkill` without `/f`)
- **THEN** the windows become visible and the app exits

#### Scenario: Uninstall while windows are hidden
- **WHEN** windows are hidden in slots and Windows asks the app to close before removing the package
- **THEN** all those windows are visible after the app is gone

#### Scenario: Stopped without being asked
- **WHEN** the app is terminated without any close request (e.g. `taskkill /f`, or an uninstall that doesn't ask)
- **THEN** its windows stay hidden until the app runs again, which takes them back; after an uninstall it never runs again, so the documentation tells users to choose Exit before uninstalling

#### Scenario: No restart loop
- **WHEN** the app crashes
- **THEN** the restart registration does not relaunch it
