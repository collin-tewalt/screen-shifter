## ADDED Requirements

### Requirement: Previews show the application's icon
Each preview large enough to carry one SHALL show its application's icon at the bottom-left of the preview, resolved at high resolution from the window's package or executable where possible, falling back to the window's own icon, and never drawn larger than its pixels. The icon SHALL appear once the spread has settled, SHALL fade away as the spread collapses or the preview is chosen, SHALL be hidden while its preview is dragged, and SHALL return if the preview returns to the spread. Looking up an icon SHALL never delay opening the spread or the panel; previews appear without an icon until it arrives.

#### Scenario: Electron app
- **WHEN** a hidden slot holds a Claude window and a Cursor window and the spread opens
- **THEN** each preview shows that app's own icon at its bottom-left, sharp rather than a 16 pixel icon scaled up

#### Scenario: Icon arrives late
- **WHEN** an icon is not ready when the spread has settled
- **THEN** the preview is shown without it and the icon fades in when it arrives

#### Scenario: Drag
- **WHEN** the user drags a preview
- **THEN** its icon disappears for the drag and fades back in if the preview returns

#### Scenario: Unresolvable icon
- **WHEN** neither the package nor the executable gives an icon
- **THEN** the window's own icon is shown at its native size, or none if there isn't one
