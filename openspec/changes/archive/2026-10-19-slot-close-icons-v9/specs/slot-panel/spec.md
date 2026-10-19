## ADDED Requirements

### Requirement: Slot tiles carry an x that closes their slot
Each slot tile on a monitor with two or more slots SHALL show an x in its top-right corner; a monitor with a single slot SHALL show no x, and "+ New" tiles SHALL never show one. Clicking an x (press and release on the same x) SHALL close that slot as defined by "A slot can be closed" and SHALL NOT switch slots or act as a click on the tile. The x SHALL highlight while the cursor is over it. After a close the panel SHALL stay up and update, in swipe mode too. The x zone is not part of its tile for acting purposes: pressing a tile and releasing on its x, releasing `Ctrl+Alt` over an x, and dropping a window or a preview on an x SHALL all behave as if the point were between tiles (nothing is switched, moved, or closed; a dragged window or preview returns). This takes precedence over the "Clicking a tile", "Dragging a window onto a tile" and "Releasing Ctrl+Alt over a slot tile" requirements, over the exposé's preview drops, and, in swipe mode, over the rule that a tile click closes the panel: an x click keeps the panel up. The x SHALL NOT close or alter any window.

#### Scenario: Close from the panel
- **WHEN** monitor A has three slots, the panel is up, and the user clicks the x on tile 2
- **THEN** tile 2's windows fold into tile 1, A's panel shows two slot tiles and "+ New", and monitor B is unaffected

#### Scenario: No x on the last slot
- **WHEN** a monitor has one slot and the panel is shown
- **THEN** its slot tile has no x

#### Scenario: Press on an x, release elsewhere
- **WHEN** the user presses on an x and releases over a different tile
- **THEN** nothing is closed and nothing switches

#### Scenario: Release over an x
- **WHEN** the user releases `Ctrl+Alt` with the cursor over a tile's x
- **THEN** the panel closes and no slot is closed or switched

#### Scenario: Release after closing with the chord held
- **WHEN** the user holds `Ctrl+Alt`, clicks an x (so the panel shrinks and a different tile ends up under the cursor), and then releases `Ctrl+Alt`
- **THEN** the panel closes and no slot is switched

#### Scenario: Drop on an x
- **WHEN** the user drags a window and releases it over a tile's x
- **THEN** the window returns to where it was
