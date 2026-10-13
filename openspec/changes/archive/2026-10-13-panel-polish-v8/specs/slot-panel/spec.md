## ADDED Requirements

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
