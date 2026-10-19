## ADDED Requirements

### Requirement: A slot can be closed, folding its windows into a neighbor
Closing slot `i` of a monitor with two or more slots SHALL remove it and move its windows into the slot to its left, or into the slot to its right if `i` is the first. The slots after it SHALL renumber. Closing SHALL never close, minimize, or hide any window, and a monitor SHALL always keep at least one slot (closing the only slot does nothing). If the receiving slot is the monitor's current slot, the folded windows SHALL appear on screen. If the closed slot was the monitor's current slot, its windows SHALL stay on screen, the receiving neighbor SHALL become the current slot, and the neighbor's hidden windows SHALL be shown with them. The state file SHALL be written before any window is shown. No empty slot other than the current one SHALL result.

#### Scenario: Close a hidden slot next to the current slot
- **WHEN** a monitor has slots 1, 2, 3 (slot 1 current) with windows hidden in 2 and 3, and slot 2 is closed
- **THEN** slot 2's windows appear on screen (slot 1 is its left neighbor and is current), and old slot 3 is now slot 2

#### Scenario: Close a hidden slot next to another hidden slot
- **WHEN** a monitor has slots 1, 2, 3 (slot 1 current) and slot 3 is closed
- **THEN** slot 3's windows move into slot 2, still hidden, and the monitor has two slots

#### Scenario: Close the leftmost slot folds right
- **WHEN** a monitor has slots 1, 2 (slot 2 current, slot 1 holding hidden windows) and slot 1 is closed
- **THEN** slot 1's windows appear on screen with slot 2's, and the monitor has one slot

#### Scenario: Close the current slot
- **WHEN** a monitor has slots 1, 2 (slot 2 current, showing windows A and B) with window C hidden in slot 1, and slot 2 is closed
- **THEN** A and B stay where they are, C appears, and slot 1 is the only slot

#### Scenario: Last slot cannot be closed
- **WHEN** a monitor has one slot and a close of it is requested
- **THEN** nothing changes

#### Scenario: Restart after closing
- **WHEN** a slot is closed and the app is closed or killed immediately afterwards
- **THEN** on the next start every window the state file lists is shown or placed in a slot, none left hidden

### Requirement: A closed window is forgotten from its slot
Whenever the system notices that a window recorded in a hidden slot no longer exists (when a close requested from the exposé finishes, and when the panel opens), it SHALL remove that window, and any recorded owned windows that no longer exist, from its slot and from the state file. A hidden slot this empties SHALL be removed like any empty slot. A window that still exists MUST NOT be removed, wherever it now is. Removing dead windows MUST NOT affect any other entry.

#### Scenario: Last window closed
- **WHEN** slot 3 holds one hidden window and it is closed from its preview
- **THEN** slot 3 is removed and the slots after it renumber

#### Scenario: Other windows remain
- **WHEN** a hidden slot holds two windows and one is closed
- **THEN** the slot holds the other and keeps its number

#### Scenario: A pending close and a switch
- **WHEN** a close was requested for a window, and before it finishes the monitor switches slots so the window is hidden
- **THEN** the window stays recorded in its slot until it no longer exists
