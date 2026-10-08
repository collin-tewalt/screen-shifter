## MODIFIED Requirements

### Requirement: Holding Ctrl+Alt shows the slot panel on every monitor
When `Ctrl` and `Alt` are held with no other key for about 250 ms, the system SHALL show a panel on every connected monitor with one tile per slot for that monitor, followed by a "+ New" tile if that monitor has fewer than 8 slots. Each slot tile SHALL show its slot number and the titles of up to 5 windows in that slot (or "—" if empty), with the monitor's current slot highlighted. The panel MUST fit within its monitor's width, shrinking its tiles if needed. When any monitor's slots change while the panel is visible (including slots removed on a monitor other than the one switched), its panel SHALL update to the new tiles. A "+ New" tile MUST always mean "a new slot at the end", even if slots changed since it was drawn. Releasing `Ctrl` or `Alt` SHALL hide all panels. Panels MUST NOT take keyboard focus.

#### Scenario: Panel appears on both monitors
- **WHEN** monitor A has 3 slots and monitor B has 1, and the user holds `Ctrl+Alt` for half a second
- **THEN** A's panel shows 3 slot tiles and "+ New", B's shows 1 slot tile and "+ New", each with its current slot highlighted

#### Scenario: Tiles list hidden windows
- **WHEN** monitor A's slot 2 contains hidden windows "Teams" and "Outlook" and the user holds `Ctrl+Alt`
- **THEN** monitor A's tile 2 lists "Teams" and "Outlook"

#### Scenario: No "+ New" at the limit
- **WHEN** a monitor has 8 slots and the panel is shown
- **THEN** its panel shows 8 slot tiles, no "+ New" tile, and fits on the monitor

#### Scenario: Other shortcuts don't trigger the panel
- **WHEN** the user presses `Ctrl+Alt+T` in an application
- **THEN** no panel appears and the application receives the shortcut

#### Scenario: Release hides the panel
- **WHEN** the panel is visible and the user releases `Alt`
- **THEN** all panels disappear and keyboard focus is where it was before

### Requirement: Wheel and tilt switch slots while Ctrl+Alt is held
While `Ctrl+Alt` is held, on the monitor under the mouse cursor, the system SHALL switch to the next slot for each wheel-up notch or wheel tilt to the right, and to the previous slot for each wheel-down notch or wheel tilt to the left. These wheel and tilt events MUST NOT reach applications. A tilt held down SHALL cause exactly one switch. Without `Ctrl+Alt` held, wheel and tilt behave normally.

#### Scenario: Wheel up switches next
- **WHEN** the cursor is on monitor B (slot 1 of 2) and the user holds `Ctrl+Alt` and scrolls up one notch
- **THEN** monitor B switches to slot 2 and monitor A is unaffected

#### Scenario: Smooth-scrolling wheel
- **WHEN** a high-resolution wheel reports one notch as several small deltas
- **THEN** exactly one switch happens

#### Scenario: Held tilt switches once
- **WHEN** the user holds `Ctrl+Alt` and holds the wheel tilted left for two seconds on a monitor at slot 3
- **THEN** that monitor switches to slot 2 only

#### Scenario: Applications don't zoom
- **WHEN** the cursor is over a browser and the user scrolls with `Ctrl+Alt` held
- **THEN** the browser does not zoom or scroll

#### Scenario: Normal scrolling unaffected
- **WHEN** the user scrolls without holding `Ctrl+Alt`
- **THEN** the application under the cursor scrolls as usual

### Requirement: Clicking a tile jumps to that slot
While the panel is visible, clicking a slot tile SHALL switch that tile's monitor directly to that slot. Clicking the current slot's tile SHALL do nothing. Clicking "+ New" SHALL create a slot at the end of that monitor and switch to it.

#### Scenario: Jump on the other monitor
- **WHEN** the cursor is on monitor A and the user clicks tile 3 on monitor B's 3-slot panel
- **THEN** monitor B switches to slot 3 and monitor A is unaffected

#### Scenario: Click "+ New"
- **WHEN** monitor A has 2 slots, is on slot 1, and the user clicks A's "+ New" tile
- **THEN** monitor A switches to a new, empty slot 3 and its panel shows 3 slot tiles and "+ New"

### Requirement: Dragging a window onto a tile moves it to that slot
While `Ctrl+Alt` is held, pressing the left button on a managed window and dragging SHALL show a translucent preview of that window, at most 360×240 with its aspect ratio kept, following the cursor, while the real window stays in place. Releasing over a slot tile SHALL move the window (with its owned dialogs) into that slot, on that tile's monitor. Releasing over "+ New" SHALL create a slot at the end of that tile's monitor and move the window into it. Releasing anywhere else, or releasing `Ctrl` or `Alt` during the drag, SHALL cancel with no change. While `Ctrl+Alt` is held, left clicks MUST NOT reach applications.

#### Scenario: Drop into another slot on the same monitor
- **WHEN** the user drags Notepad on monitor A onto monitor A's tile 3
- **THEN** Notepad disappears, tile 3 lists it, and switching A to slot 3 shows it

#### Scenario: Drop onto the other monitor's current slot
- **WHEN** the user drags Notepad from monitor A onto monitor B's highlighted (current) tile
- **THEN** Notepad moves to monitor B and stays visible, keeping its maximized or normal state

#### Scenario: Drop onto "+ New"
- **WHEN** monitor B has 1 slot and the user drags Notepad from monitor A onto B's "+ New" tile
- **THEN** Notepad disappears, B's panel shows a new tile 2 listing it, and B stays on slot 1
