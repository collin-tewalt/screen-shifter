## ADDED Requirements

### Requirement: Hovering a tile spreads its windows across that monitor
While the panel is visible, when the mouse rests on a slot tile with at least one window for about 300 ms, the system SHALL dim that tile's monitor and lay out previews of that slot's windows across it, above the panel, without overlapping each other or the panel. Moving to another slot tile on either monitor SHALL replace the spread with that slot's, on that tile's monitor, without waiting. Moving to the other monitor anywhere but a tile, releasing `Ctrl+Alt` without a preview under the mouse, or any slot switch SHALL collapse it. The previews MUST NOT take focus. The "+ New" tile and empty slots SHALL show no spread.

#### Scenario: Hover a hidden slot
- **WHEN** monitor A's slot 2 holds hidden windows "Teams" and "Outlook" and the user rests the mouse on A's tile 2
- **THEN** A dims and previews of Teams and Outlook appear across A, and monitor B is unchanged

#### Scenario: Follow the mouse to the other monitor
- **WHEN** the spread is open on monitor A and the user moves to tile 1 of monitor B's panel
- **THEN** A's spread collapses and B's slot 1 spreads across B

#### Scenario: Passing over tiles
- **WHEN** the user sweeps the mouse across several tiles without stopping
- **THEN** no spread opens

### Requirement: Previews show what the windows look like
Each preview SHALL show the window's last captured image, taken while it was visible (at the latest when the panel last appeared), keeping its aspect ratio. A window with no image (minimized, never captured, or its app not responding) SHALL be shown as a card with its icon and title. Capturing MUST NOT delay the panel, a switch, or input, and MUST NOT wait on an application that is not responding. Hovering a preview SHALL highlight it. A window that was hidden or switched away while being captured SHALL keep its previous image rather than a blank one. Images MUST be kept in memory only.

#### Scenario: Current slot
- **WHEN** monitor A shows three overlapping windows and the user holds `Ctrl+Alt` and rests on A's current tile
- **THEN** all three appear as full, unobstructed previews

#### Scenario: Hung app
- **WHEN** a window in the slot belongs to an application that is not responding
- **THEN** the panel and spread appear without delay and that window is shown as a card

### Requirement: Choosing a preview brings that window forward
Clicking a preview, or releasing `Ctrl+Alt` while the mouse is on one, SHALL switch that monitor to the preview's slot (if it isn't already current) and give that window keyboard focus, restoring it first if it was minimized. Clicking a preview MUST NOT start a window drag or reach the window underneath it.

#### Scenario: Choose a window in a hidden slot
- **WHEN** the spread shows monitor A's hidden slot 3 and the user clicks the preview of "Notepad"
- **THEN** monitor A switches to slot 3 and Notepad has focus

#### Scenario: Release to choose
- **WHEN** the spread is open and the mouse is on the preview of "Teams" when the user releases `Alt`
- **THEN** the panel and spread disappear, Teams' slot is shown on its monitor, and Teams has focus

#### Scenario: Choose a minimized window
- **WHEN** the spread shows a minimized window as a card and the user clicks it
- **THEN** the window is restored on its monitor and has focus

#### Scenario: Not a release
- **WHEN** the mouse is on a preview and the user presses `Ctrl+Alt+Del` or types a character with AltGr
- **THEN** nothing is switched or focused, and the spread collapses

#### Scenario: Release elsewhere
- **WHEN** the spread is open and the mouse is not on a preview when the user releases `Ctrl+Alt`
- **THEN** the spread collapses and nothing is switched or focused

### Requirement: The spread animates
When Windows animation effects are enabled, opening SHALL grow each preview out of its window's position on screen (current slot) or out of the hovered tile (hidden slot) while the monitor dims, collapsing SHALL fold them back, and choosing SHALL expand the chosen preview onto its window while the others fade. The motion SHALL be driven by the system compositor. With animation effects off, the spread SHALL appear and disappear without motion. Releasing `Ctrl+Alt` SHALL let the collapse or choose motion finish rather than cut it off. No preview or dimming may remain on screen more than 1 second after the spread is closed, or at all after monitors change, "Restore all windows", or Exit.

#### Scenario: Open and collapse
- **WHEN** the user rests on a tile and then moves the mouse to the other monitor
- **THEN** the previews grow into place and then fold away, and nothing of the spread remains

#### Scenario: Animation effects off
- **WHEN** animation effects are off and the user rests on a tile
- **THEN** the previews appear in place at once
