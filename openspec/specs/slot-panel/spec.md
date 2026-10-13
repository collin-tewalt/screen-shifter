# slot-panel Specification

## Purpose
TBD - created by archiving change slot-panel-v2. Update Purpose after archive.
## Requirements
### Requirement: Holding Ctrl+Alt shows the slot panel on every monitor
When `Ctrl` and `Alt` are held with no other key for about 250 ms, the system SHALL show a panel on every connected monitor with one tile per slot for that monitor, followed by a "+ New" tile if that monitor has fewer than 8 slots. Each slot tile SHALL show its slot number and the titles of up to 5 windows in that slot, each with its application's icon where one can be obtained without waiting on an unresponsive application (or "—" if empty), with the monitor's current slot highlighted. The panel MUST fit within its monitor's width, shrinking its tiles if needed. When any monitor's slots change while the panel is visible (including slots removed on a monitor other than the one switched), its panel SHALL update to the new tiles. A "+ New" tile MUST always mean "a new slot at the end", even if slots changed since it was drawn. Releasing `Ctrl` or `Alt` SHALL hide all panels. Panels MUST NOT take keyboard focus.

#### Scenario: Panel appears on both monitors
- **WHEN** monitor A has 3 slots and monitor B has 1, and the user holds `Ctrl+Alt` for half a second
- **THEN** A's panel shows 3 slot tiles and "+ New", B's shows 1 slot tile and "+ New", each with its current slot highlighted

#### Scenario: Tiles list hidden windows
- **WHEN** monitor A's slot 2 contains hidden windows "Teams" and "Outlook" and the user holds `Ctrl+Alt`
- **THEN** monitor A's tile 2 lists "Teams" and "Outlook", each with its app icon

#### Scenario: Unresponsive app
- **WHEN** a window in a slot belongs to an application that is not responding and the user holds `Ctrl+Alt`
- **THEN** the panel appears without a noticeable delay and that title is listed, with or without an icon

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

### Requirement: Normal input is never stuck
The system MUST stop intercepting mouse input as soon as `Ctrl` or `Alt` is physically released, even if the release was not observed directly (for example while the secure desktop or a UAC prompt was shown). A mouse button press and its release MUST either both reach the application or both be intercepted.

#### Scenario: Ctrl+Alt+Del while holding the chord
- **WHEN** the user holds `Ctrl+Alt`, presses `Del`, cancels the security screen, and releases all keys
- **THEN** the next left click and wheel scroll reach the application under the cursor

#### Scenario: Chord pressed during a click
- **WHEN** the user presses the left button on a window title bar, then presses `Ctrl+Alt`, then releases the button
- **THEN** the application receives the button release and is not left mid-drag

#### Scenario: Chord released during a drag
- **WHEN** the user starts a drag with `Ctrl+Alt` held, releases `Ctrl+Alt`, then releases the button
- **THEN** the drag is cancelled and the application under the cursor receives no stray button release

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

### Requirement: Releasing Ctrl+Alt over a slot tile switches to that slot
When `Ctrl` or `Alt` is released while the panel is visible and the mouse is over a slot tile (not a preview), the system SHALL switch that tile's monitor to that slot, exactly as clicking the tile does, then hide the panels. Releasing over the monitor's current slot's tile SHALL change nothing. Releasing over "+ New" SHALL change nothing. In every case the panel SHALL hide and keyboard focus SHALL be where it was before, except when a switch happens. A release over a preview keeps the existing choose-the-window behavior.

#### Scenario: Release over another monitor's tile
- **WHEN** the cursor is on monitor A and the user holds `Ctrl+Alt`, moves to tile 3 of monitor B's panel, and releases `Alt`
- **THEN** monitor B switches to slot 3, monitor A is unaffected, and all panels disappear

#### Scenario: Release over the current tile
- **WHEN** the user releases `Ctrl+Alt` with the mouse on the highlighted (current) tile of a monitor
- **THEN** nothing switches and all panels disappear

#### Scenario: Release over "+ New"
- **WHEN** the user releases `Ctrl+Alt` with the mouse on a monitor's "+ New" tile
- **THEN** no slot is created, nothing switches, and all panels disappear

#### Scenario: Release over a preview still chooses
- **WHEN** the spread is open and the mouse is on a preview when the user releases `Ctrl+Alt`
- **THEN** the behavior of the existing preview choice applies unchanged

