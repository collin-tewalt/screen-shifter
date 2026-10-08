## MODIFIED Requirements

### Requirement: Switching slides the old slot off the screen
On every successful switch (wheel, tilt, arrow hotkeys, tile click, or "+ New"), when Windows animation effects are enabled, the affected monitor SHALL show a still picture of its previous contents that slides off the monitor in about 300 ms, measured from when it first starts moving, with an ease-out motion and a soft shadow on its trailing edge, revealing the new slot. It SHALL slide left when the user moved toward the end of the slot list (next, a later tile, or "+ New") and right when the user moved toward the start, even if removing an empty slot renumbers the slots. The picture MUST stay within the affected monitor, MUST NOT take focus or intercept input, and MUST NOT affect the other monitor.

#### Scenario: Next slides left
- **WHEN** monitor A is on slot 1 of 2 and the user scrolls up with `Ctrl+Alt` held
- **THEN** A's previous contents slide off to the left and slot 2 is visible when the motion ends

#### Scenario: Previous slides right
- **WHEN** monitor A is on slot 3 and the user presses `Ctrl+Alt+←`
- **THEN** A's previous contents slide off to the right

#### Scenario: Tile jump
- **WHEN** monitor B has 4 slots, is on slot 1, and the user clicks tile 4 on B's panel
- **THEN** one sheet slides off B to the left

#### Scenario: Leaving an empty slot forward
- **WHEN** monitor A is on an empty slot 2 of 3 and the user switches next
- **THEN** the sheet slides off to the left, even though A is now on slot 2 of 2

#### Scenario: Neighbor monitor untouched
- **WHEN** monitor A switches while monitor B is beside it
- **THEN** nothing is drawn on monitor B

#### Scenario: Input during the motion
- **WHEN** the user clicks or types while a sheet is still moving
- **THEN** the input goes to the windows of the new slot as if no animation were playing

### Requirement: Fast switching chains sheets
When a switch happens on a monitor while a sheet is still moving there, the new switch SHALL get its own sheet showing the slot just left, beneath the moving one, and the moving sheet SHALL finish faster. When the motion ends, the screen MUST show the final slot with no still picture left behind.

#### Scenario: Flick through three slots
- **WHEN** a monitor has 4 slots that all hold windows, is on slot 1, and the user scrolls three notches quickly
- **THEN** sheets peel off one after another, each showing a slot that was actually on screen, and the monitor ends on slot 4 showing its live windows

#### Scenario: The panel is not part of the picture
- **WHEN** the user switches with the wheel while the panel is visible
- **THEN** the sliding picture does not contain a copy of the panel; only the live panel is visible
