## MODIFIED Requirements

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
