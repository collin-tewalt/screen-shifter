## ADDED Requirements

### Requirement: Switching slides the old slot off the screen
On every successful switch (wheel, tilt, arrow hotkeys, or tile click), when Windows animation effects are enabled, the affected monitor SHALL show a still picture of its previous contents that slides off the monitor in about 300 ms, measured from when it first starts moving, with an ease-out motion and a soft shadow on its trailing edge, revealing the new slot. It SHALL slide left when moving to a higher-numbered slot and right when moving to a lower-numbered slot. The picture MUST stay within the affected monitor, MUST NOT take focus or intercept input, and MUST NOT affect the other monitor.

#### Scenario: Next slides left
- **WHEN** monitor A is on slot 1 and the user scrolls up with `Ctrl+Alt` held
- **THEN** A's previous contents slide off to the left and slot 2 is visible when the motion ends

#### Scenario: Previous slides right
- **WHEN** monitor A is on slot 3 and the user presses `Ctrl+Alt+←`
- **THEN** A's previous contents slide off to the right

#### Scenario: Tile jump
- **WHEN** monitor B is on slot 1 and the user clicks tile 4 on B's panel
- **THEN** one sheet slides off B to the left

#### Scenario: Neighbor monitor untouched
- **WHEN** monitor A switches while monitor B is beside it
- **THEN** nothing is drawn on monitor B

#### Scenario: Input during the motion
- **WHEN** the user clicks or types while a sheet is still moving
- **THEN** the input goes to the windows of the new slot as if no animation were playing

### Requirement: Fast switching chains sheets
When a switch happens on a monitor while a sheet is still moving there, the new switch SHALL get its own sheet showing the slot just left, beneath the moving one, and the moving sheet SHALL finish faster. When the motion ends, the screen MUST show the final slot with no still picture left behind.

#### Scenario: Flick through three slots
- **WHEN** the user scrolls three notches quickly on a monitor at slot 1
- **THEN** sheets peel off one after another, each showing a slot that was actually on screen, and the monitor ends on slot 4 showing its live windows

#### Scenario: The panel is not part of the picture
- **WHEN** the user switches with the wheel while the panel is visible
- **THEN** the sliding picture does not contain a copy of the panel; only the live panel is visible

### Requirement: Dropped windows fly to their destination
When animation effects are enabled, after a drag ends the ghost SHALL continue for about 180 ms and fade out:
- dropped on a tile for a hidden slot, it SHALL shrink and glide into that tile, and the tile SHALL briefly pulse;
- dropped on the other monitor's current slot, it SHALL glide to where the window appears and fade into it;
- when the drag is cancelled, it SHALL glide back to the window and fade.
The window itself MUST be moved exactly as without animation.

#### Scenario: Into a tile
- **WHEN** the user drops Notepad onto tile 3 of monitor A's panel
- **THEN** the ghost shrinks into tile 3, tile 3 pulses, and Notepad is in slot 3

#### Scenario: Onto the other monitor
- **WHEN** the user drops Notepad onto monitor B's current tile
- **THEN** the ghost glides to Notepad's new position on monitor B and fades as Notepad appears there

#### Scenario: Cancel
- **WHEN** the user releases `Ctrl+Alt` mid-drag
- **THEN** the ghost glides back over the window and fades, and the window is unchanged

### Requirement: Motion respects the system setting
When Windows "Animation effects" is turned off, the system SHALL switch and move windows with no sheet, flight, or pulse.

#### Scenario: Animation effects off
- **WHEN** animation effects are off and the user switches slots
- **THEN** the switch happens instantly as before, with only the usual indicator

### Requirement: A still picture never stays on screen
Every animation SHALL be removed no later than 1 second after it starts, and immediately when monitors change, when "Restore all windows" or Exit is chosen, or when the app ends.

#### Scenario: Monitor change mid-motion
- **WHEN** a monitor is disconnected while a sheet is moving
- **THEN** all sheets disappear at once and the screen shows live windows

#### Scenario: Tray menu open
- **WHEN** the user switches with the arrow hotkeys while the tray menu is open
- **THEN** the switch happens without motion and no still picture appears
