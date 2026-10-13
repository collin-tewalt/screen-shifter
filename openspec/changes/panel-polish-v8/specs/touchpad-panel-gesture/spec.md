## ADDED Requirements

### Requirement: Three-finger vertical swipes move the screen as the fingers travel
While three fingers move on the touchpad, the screen SHALL follow their vertical travel from touchdown, live and without waiting for release. At 100 touchpad units of upward travel the slot panel SHALL appear on every monitor, with no key held and no delay. At 250 units of upward travel the exposé SHALL open on every monitor, each spreading its current slot's windows. Moving back past a threshold SHALL undo the phase it crossed. Releasing the fingers SHALL leave the screen in the phase it reached.

From the panel, a further upward travel to 250 units SHALL open the exposé on every monitor, and downward travel past 100 units SHALL close the panel. From the exposé, downward travel past 100 units SHALL close the exposé and return to the panel, choosing the window under the mouse if a preview is hovered (as a Ctrl+Alt release over a preview does), and downward travel past 250 units SHALL close the panel.

The panel SHALL behave like the `Ctrl+Alt` panel: hovering a tile spreads its windows, clicking a tile switches to it, clicking a preview chooses its window. Key release SHALL NOT choose or switch in this mode.

#### Scenario: Panel while still moving
- **WHEN** the panel is closed and the user swipes three fingers up and passes 100 units without lifting
- **THEN** the panel is already visible while the fingers are still down

#### Scenario: Exposé while still moving
- **WHEN** the panel is up and the user swipes three fingers up and passes 250 units without lifting
- **THEN** both monitors show their current slot's windows spread across the screen while the fingers are still down

#### Scenario: Reversing undoes the phase
- **WHEN** the exposé is open from a swipe up and the user moves the fingers back down past 150 units without lifting
- **THEN** the exposé closes and the panel stays up

#### Scenario: Releasing keeps the phase reached
- **WHEN** the user swipes up past 100 units but not 250, then lifts
- **THEN** the panel stays up and no exposé opens

### Requirement: Swipe-opened panel exits without switching unless a choice is made
A panel opened by a three-finger swipe SHALL close, switching nothing, when the user presses `Esc`, clicks empty space (any point that is not a tile or preview, including the gaps between tiles and the dimmed screen around the exposé's previews), or swipes three fingers down past 100 units from the panel. Clicking a tile switches that tile's monitor to its slot and closes the panel. Clicking a preview chooses its window and closes the panel. Releasing `Ctrl+Alt` closes the panel without choosing. After any close, mouse input SHALL reach applications normally.

#### Scenario: Esc closes
- **WHEN** the panel was opened by a swipe and the user presses `Esc`
- **THEN** the panel disappears, nothing switches, and the next click reaches the window under the cursor

#### Scenario: Click on empty space closes
- **WHEN** the panel was opened by a swipe and the user clicks where no tile or preview is shown
- **THEN** the panel disappears, nothing switches, and the click does not reach the window underneath

#### Scenario: Click a tile
- **WHEN** the panel was opened by a swipe and the user clicks tile 2 of monitor B
- **THEN** monitor B switches to slot 2, and the panel closes

#### Scenario: Click a preview
- **WHEN** the exposé is open from a swipe and the user clicks the preview of "Notepad" in a hidden slot
- **THEN** that monitor switches to the window's slot, Notepad has focus, and the panel closes

### Requirement: Swipe down over a preview focuses that window
When the exposé is open from a swipe and a preview is under the mouse, downward travel past 100 units SHALL bring focus to the window under that preview, restoring it first if it was minimized, and close the exposé and the panel, as a Ctrl+Alt release over that preview does. The rest of that touch SHALL NOT reopen the panel. With no preview under the mouse, downward travel past 100 units SHALL only return to the panel, and past 250 units close it, without switching or focusing anything.

#### Scenario: Swipe down over a window in the exposé
- **WHEN** the exposé is open and the mouse is on the preview of "Notepad" in a hidden slot, and the user swipes three fingers down past 100 units
- **THEN** that monitor switches to Notepad's slot, Notepad has focus, and the panel closes

#### Scenario: Swipe down with no preview under the mouse
- **WHEN** the exposé is open and the mouse is not on any preview, and the user swipes three fingers down past 250 units
- **THEN** the exposé and the panel close and nothing switches or gains focus

### Requirement: A swipe up with the panel open spreads every monitor
Hovering a tile on any monitor SHALL re-spread that monitor's slot. Moving the mouse between monitors SHALL NOT collapse the other monitors' spreads while the exposé was opened by a swipe. Choosing a preview ends the exposé on every monitor.

#### Scenario: Moving between screens keeps both spreads
- **WHEN** both screens are spread from a swipe and the mouse moves from monitor A to monitor B
- **THEN** monitor A's spread stays open

#### Scenario: A fast swipe up waits for the pictures
- **WHEN** the user swipes three fingers up past 250 units before the panel's window pictures have been taken
- **THEN** the exposé opens once the pictures arrive (or after 300 ms at most), with its previews in place rather than replaced mid-animation

#### Scenario: Hovering a tile re-spreads its monitor only
- **WHEN** both screens are spread and the mouse rests on tile 3 of monitor B
- **THEN** monitor B switches its spread to slot 3 and monitor A's spread is unchanged

### Requirement: Mouse input is never stuck in swipe mode
While a swipe-opened panel is visible, left clicks and wheel input SHALL be handled by the panel, as they are while `Ctrl+Alt` is held. Every exit path in the requirements above, and quitting, logoff, or a crash of the app, SHALL end this mode and restore normal mouse input.

#### Scenario: Quit while the panel is open
- **WHEN** the panel was opened by a swipe and the user chooses Exit from the tray
- **THEN** the panel disappears and the next click reaches the window under the cursor

### Requirement: Left and right swipes switch once, live
When three fingers pass 100 units horizontally, the system SHALL switch the slot of the monitor under the cursor once, immediately, rather than on release. Left is the next slot and right the previous one, matching the slide animation. Further movement in the same touch does not switch again. The arrow keys are unchanged. This works whether or not the panel is open. A switch made while the exposé is open keeps the panel and re-spreads every monitor, so the exposé stays open.

#### Scenario: Switch while still moving
- **WHEN** the user swipes three fingers left and passes 100 units without lifting
- **THEN** the cursor's monitor switches to its next slot before the fingers are lifted

#### Scenario: Swipe right with the panel open
- **WHEN** the panel is open from a swipe up and the user swipes three fingers right over monitor A at slot 2
- **THEN** monitor A switches to slot 1 and the panel stays open

#### Scenario: Swipe right with the exposé open
- **WHEN** both screens are spread from a swipe and the user swipes three fingers right over monitor A
- **THEN** monitor A switches to its previous slot and spreads that new slot; monitor B's spread stays open

### Requirement: The Windows three-finger setting changes only with the owner's consent
The app SHALL read touchpad contacts itself, because Windows also acts on a three-finger swipe (by default, opening Task View) and the app cannot prevent that from its own input. On first start the app SHALL ask the owner, in a Yes/No prompt, whether to let a three-finger swipe up open the slot panel. On Yes the app SHALL save the current value of `HKCU\Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad\ThreeFingerSlideEnabled` to `touchpad.txt` in its state folder, and then set it to `0`. The original value SHALL be restored when the app exits, on a panic, and on the next start if a previous run ended before restoring. An absent value is restored by deleting it, not by writing `1`.

The tray item "Three-finger swipe opens panel" SHALL turn the feature and the Windows setting on or off, and its checked state SHALL reflect the current choice. On No, the app SHALL NOT change the Windows setting and the three-finger gestures SHALL do nothing.

**Known limitation (to be fixed in a future change):** on the owner's machine, after the Windows setting was turned back on and the app restarted, Windows' Settings page showed the three-finger gesture as off, yet Task View still opened on a three-finger swipe up. The app also records the value it finds when it starts, so after such a start Exit restores that value rather than the Windows default. Until the future change, the Windows three-finger setting may need to be set by hand in Settings > Bluetooth & devices > Touch.

#### Scenario: Owner declines
- **WHEN** the app asks and the owner chooses No
- **THEN** Windows' setting is unchanged and three-finger swipes do nothing in the app

#### Scenario: Owner agrees, then exits
- **WHEN** the owner chooses Yes and then chooses Exit from the tray
- **THEN** the original Windows setting is restored before the app exits

#### Scenario: Previous run ended without restoring
- **WHEN** the app starts and `touchpad.txt` records a changed setting from a previous run
- **THEN** the original value is restored before the setting is applied again
