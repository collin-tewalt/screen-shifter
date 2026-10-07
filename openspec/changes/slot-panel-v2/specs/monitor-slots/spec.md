## ADDED Requirements

### Requirement: A window can be moved into any slot on any monitor
The system SHALL support moving a managed window (with its owned windows) into a given slot of a given monitor. The window MUST be removed from any slot it was recorded in. If the target is another monitor, the window SHALL be moved onto that monitor keeping its minimized/maximized state. If the target slot is not that monitor's current slot, the window SHALL be hidden and recorded in the target slot, persisted before hiding exactly as for a switch.

#### Scenario: Move to a hidden slot on the same monitor
- **WHEN** window X is visible on monitor A (slot 1) and is moved to A's slot 3
- **THEN** X is hidden, recorded only in A's slot 3, and listed in the state file

#### Scenario: Move to another monitor's current slot
- **WHEN** maximized window X on monitor A is moved to monitor B's current slot
- **THEN** X is visible and maximized on monitor B and is not recorded in any slot

#### Scenario: Move to another monitor's hidden slot
- **WHEN** window X on monitor A is moved to monitor B's slot 2 while B is on slot 1, and then B switches to slot 2
- **THEN** X appears on monitor B

#### Scenario: Quick switch after a move
- **WHEN** window X was shown by a switch on monitor A, is moved to A's slot 4 within a second, and A is then switched again within a second
- **THEN** X remains recorded only in A's slot 4

## MODIFIED Requirements

### Requirement: Slot indicator
After each successful switch the system SHALL display the new slot as "N / 4" centered on the affected monitor for about 600 ms, without taking focus or intercepting mouse input. While the slot panel is visible, the indicator SHALL NOT be shown; the panel's highlight shows the current slot instead.

#### Scenario: Indicator on switch
- **WHEN** the user switches monitor A to slot 3
- **THEN** "3 / 4" briefly appears on monitor A and focus remains on the restored window

#### Scenario: No indicator while the panel is visible
- **WHEN** the panel is visible and the user switches monitor A with the wheel
- **THEN** no "N / 4" indicator appears and A's panel highlights the new slot
