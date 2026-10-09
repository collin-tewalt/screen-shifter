## MODIFIED Requirements

### Requirement: Switching slides the old slot off the screen
On every successful switch (wheel, tilt, arrow hotkeys, tile click, or "+ New"), when Windows animation effects are enabled, the affected monitor SHALL show a still picture of its previous contents that holds still for about 100 ms while the windows underneath finish changing, then slides off the monitor in about 400 ms, with an ease-out motion and a soft shadow on its trailing edge, revealing the new slot. It SHALL slide left when the user moved toward the end of the slot list (next, a later tile, or "+ New") and right when the user moved toward the start, even if removing an empty slot renumbers the slots. Where the graphics hardware supports it, the motion SHALL be driven by the system compositor so that it advances on every display refresh regardless of what the app itself is doing; otherwise it falls back to the app drawing each frame. The picture MUST stay within the affected monitor, MUST NOT take focus or intercept input, and MUST NOT affect the other monitor.

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

#### Scenario: No windows popping during the slide
- **WHEN** the slot being left has several overlapping windows and one maximized window
- **THEN** the revealed area shows the new slot already settled, with no windows disappearing or appearing as the sheet moves

#### Scenario: Smooth while the app is busy
- **WHEN** a sheet is sliding and the app is busy (e.g. refreshing the panel or enumerating windows)
- **THEN** the sheet keeps moving without a visible pause

#### Scenario: Fallback
- **WHEN** the monitor can't be captured by the GPU path (e.g. a rotated monitor, an unsupported display format, or a Remote Desktop session)
- **THEN** the sheet still slides off, drawn the older way

#### Scenario: Neighbor monitor untouched
- **WHEN** monitor A switches while monitor B is beside it
- **THEN** nothing is drawn on monitor B

#### Scenario: Input during the motion
- **WHEN** the user clicks or types while a sheet is still moving
- **THEN** the input goes to the windows of the new slot as if no animation were playing
