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

### Requirement: Previews carry an x that asks the app to close the window
Each preview SHALL show an x at its top-right, highlighted while the cursor is on it. Clicking it (press and release on the same x) SHALL send the window a normal close request, as its own close button does, and SHALL NOT force-kill anything. The preview SHALL dim while the request is pending. When the window is gone (destroyed, or for a window of the current slot no longer shown) its preview SHALL fade away, the window SHALL be removed from its slot (a hidden slot that empties is removed and its spread collapses), and the panel SHALL update. If the window is still there after about three seconds, the preview SHALL return to normal and nothing else changes. Pressing an x SHALL NOT start a drag, releasing `Ctrl+Alt` over an x SHALL NOT choose the window, and the slot tile's x in the panel SHALL NOT close any window.

#### Scenario: Close a window in a hidden slot
- **WHEN** the spread of a hidden slot with two windows is up and the user clicks the x on one preview
- **THEN** that window receives a close request, its preview dims and then fades away when the window is gone, and the other preview and the slot remain

#### Scenario: Unsaved changes
- **WHEN** the user clicks the x on a preview of an editor with unsaved changes
- **THEN** the editor shows its own "save changes?" prompt, the preview stays dimmed, and after about three seconds returns to normal if the editor is still open

#### Scenario: Closing the last window of a hidden slot
- **WHEN** the only window of a hidden slot is closed from its preview
- **THEN** the slot is removed, the spread collapses, and the panel shows one slot fewer

#### Scenario: Closing to the tray
- **WHEN** the window of an app that hides to the tray instead of closing is a preview of the current slot and its x is clicked
- **THEN** the preview fades away and the window is no longer part of the slot

#### Scenario: Release over an x
- **WHEN** the user releases `Ctrl+Alt` with the cursor on a preview's x
- **THEN** the spread collapses and no window is chosen or closed
