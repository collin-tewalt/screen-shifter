## ADDED Requirements

### Requirement: A three-finger swipe up opens the slot panel
A three-finger swipe up on the touchpad SHALL show the slot panel on every monitor, exactly as holding `Ctrl+Alt` does, without the 250 ms delay and without requiring any key to be held. The panel SHALL behave identically to the `Ctrl+Alt` panel, including hovering a tile to spread its windows, clicking a tile to switch, and clicking a preview to choose a window. Key release SHALL NOT choose or switch in this mode.

#### Scenario: Swipe opens the panel
- **WHEN** the user swipes three fingers up on the touchpad with no keys held
- **THEN** the slot panel appears on every monitor with each monitor's current slot highlighted

#### Scenario: Click a tile
- **WHEN** the panel was opened by a swipe and the user clicks tile 2 of monitor B
- **THEN** monitor B switches to slot 2, and the panel closes

#### Scenario: Click a preview
- **WHEN** the panel was opened by a swipe, the spread is open, and the user clicks the preview of "Notepad" in a hidden slot
- **THEN** that monitor switches to the window's slot, Notepad has focus, and the panel closes

### Requirement: A swipe-opened panel closes without switching
A panel opened by a three-finger swipe SHALL close, switching nothing, when the user presses `Esc`, clicks empty space outside any tile or preview, or swipes three fingers down. After any close, mouse input SHALL reach applications normally.

#### Scenario: Esc closes
- **WHEN** the panel was opened by a swipe and the user presses `Esc`
- **THEN** the panel disappears, nothing switches, and the next click reaches the window under the cursor

#### Scenario: Click on empty space closes
- **WHEN** the panel was opened by a swipe and the user clicks where no tile or preview is shown
- **THEN** the panel disappears, nothing switches, and the click does not reach the window underneath

#### Scenario: Three-finger swipe down closes
- **WHEN** the panel was opened by a swipe and the user swipes three fingers down
- **THEN** the panel disappears and nothing switches

### Requirement: Mouse input is never stuck in swipe mode
While a swipe-opened panel is visible, left clicks and wheel input SHALL be handled by the panel, as they are while `Ctrl+Alt` is held. Every exit path in the requirements above, and quitting, logoff, or a crash of the app, SHALL end this mode and restore normal mouse input.

#### Scenario: Quit while the panel is open
- **WHEN** the panel was opened by a swipe and the user chooses Exit from the tray
- **THEN** the panel disappears and the next click reaches the window under the cursor

### Requirement: The Windows three-finger setting changes only with consent
The app SHALL detect a three-finger swipe itself. If Windows also acts on that swipe (by default, opening Task View) and the app cannot prevent it, the app SHALL ask the owner in-app whether to turn that Windows setting off. It SHALL change the setting only after the owner agrees, record the original value, and restore it when the app exits. If the owner declines, the app SHALL NOT change the setting and the swipe feature SHALL be unavailable.

#### Scenario: Owner declines
- **WHEN** the app asks to change the Windows three-finger setting and the owner declines
- **THEN** Windows' setting is unchanged and the three-finger swipe does not open the panel

#### Scenario: Owner agrees, then exits
- **WHEN** the owner agrees and then chooses Exit from the tray
- **THEN** the original Windows setting is restored before the app exits

### Requirement: Three-finger swipe left and right switch slots
While the touchpad gesture setting is on, a three-finger swipe left or right SHALL switch the slot of the monitor under the mouse cursor, in the direction of the slide animation: swipe left goes to the next slot and swipe right to the previous one. The arrow keys are unchanged. This works whether or not the panel is open, and a switch made while the panel is open keeps it open.

#### Scenario: Swipe left with no panel
- **WHEN** the cursor is on monitor B at slot 2 of 3, no panel is open, and the user swipes three fingers left
- **THEN** monitor B switches to slot 3 and monitor A is unaffected

#### Scenario: Swipe right with the panel open
- **WHEN** the panel is open from a swipe up and the user swipes three fingers right over monitor A at slot 2
- **THEN** monitor A switches to slot 1 and the panel stays open

### Requirement: Three-finger swipe down over a preview focuses that window
While the panel is open from a swipe, a three-finger swipe down SHALL bring focus to the window under the exposé's hovered preview, restoring it first if it was minimized, and then close the panel, as a click on that preview does. With no preview under the mouse, the swipe down SHALL close the panel without switching or focusing anything.

#### Scenario: Swipe down over a window in the exposé
- **WHEN** the spread is open and the mouse is on the preview of "Notepad" in a hidden slot, and the user swipes three fingers down
- **THEN** that monitor switches to Notepad's slot, Notepad has focus, and the panel closes

#### Scenario: Swipe down with no preview under the mouse
- **WHEN** the panel is open and the mouse is not on any preview, and the user swipes three fingers down
- **THEN** the panel closes and nothing switches or gains focus

### Requirement: A swipe up with the panel open spreads every monitor
With the panel open from a swipe, a three-finger swipe up SHALL open the exposé on every monitor at once, each spreading its current slot's windows. Hovering a tile on any monitor SHALL re-spread that monitor's slot, and moving the mouse between monitors SHALL NOT collapse the other monitors' spreads. Choosing a preview ends the exposé on every monitor.

#### Scenario: Swipe up spreads both screens
- **WHEN** the panel is open from a swipe up and the user swipes three fingers up
- **THEN** monitor A and monitor B each show their current slot's windows spread across the screen, and the panel stays up

#### Scenario: Moving between screens keeps both spreads
- **WHEN** both screens are spread and the mouse moves from monitor A to monitor B
- **THEN** monitor A's spread stays open

### Requirement: A swipe down with the exposé open and no preview returns to the panel
When the exposé is open from a swipe and the mouse is not on a preview, a three-finger swipe down SHALL close the exposé on every monitor and leave the panel up. A further swipe down, with no exposé open, SHALL close the panel.

#### Scenario: Back to the panel
- **WHEN** both screens are spread and the mouse is over a tile (not a preview) and the user swipes three fingers down
- **THEN** both spreads close and the panel stays up

#### Scenario: Second swipe down closes the panel
- **WHEN** the panel is up with no spread and the user swipes three fingers down
- **THEN** the panel closes and nothing switches

### Requirement: Vertical swipes move the screen as the fingers travel
While three fingers move, the screen SHALL follow their vertical travel from touchdown: at 100 touchpad units the panel comes in (from closed), and at 250 units the exposé comes in on every monitor. Moving back past a threshold SHALL undo the phase it crossed. Releasing the fingers SHALL leave the screen in the phase it reached. From the panel, swiping up past 250 units spreads every monitor, and swiping down past 100 units closes the panel. From the exposé, swiping down past 100 units returns to the panel and past 250 units closes the panel, choosing the window under the mouse if a preview is hovered.

#### Scenario: Panel while still moving
- **WHEN** the panel is closed and the user swipes three fingers up and passes 100 units without lifting
- **THEN** the panel is already visible while the fingers are still down

#### Scenario: Reversing undoes the phase
- **WHEN** the exposé is open from a swipe up and the user moves the fingers back down past 350 units without lifting
- **THEN** the exposé closes and the panel stays up

### Requirement: Left and right swipes switch once, live
When three fingers pass 100 units horizontally, the system SHALL switch the slot of the monitor under the cursor once, immediately, rather than on release. Left is the next slot and right the previous one, matching the slide animation. Further movement in the same touch does not switch again.

#### Scenario: Switch while still moving
- **WHEN** the user swipes three fingers left and passes 100 units without lifting
- **THEN** the cursor's monitor switches to its next slot before the fingers are lifted
