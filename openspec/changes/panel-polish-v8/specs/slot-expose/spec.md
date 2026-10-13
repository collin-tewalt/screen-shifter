## ADDED Requirements

### Requirement: Previews fold back in their windows' stacking order
When the exposé collapses, or a preview is chosen, the previews SHALL move back over one another in the order of their windows: the topmost window's preview is on top of the others as they animate. The previews SHALL end where the windows are, and no preview SHALL flicker or be replaced by a different order when the windows reappear.

#### Scenario: Collapse by releasing Ctrl+Alt
- **WHEN** three windows overlap on a monitor, the spread is open, and the user releases `Ctrl+Alt`
- **THEN** the previews fly back with the topmost window's preview on top, and when they finish the windows show the same order

#### Scenario: Collapse by a swipe down
- **WHEN** the exposé is open from a swipe and the user swipes three fingers down to close it
- **THEN** the previews fold back in the same window order as above
