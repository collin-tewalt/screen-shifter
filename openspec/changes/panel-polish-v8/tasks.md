## 1. Release over a tile (item 1)

- [x] 1.1 In `WM_CHORD_UP` (`src/main.rs`), when `RELEASED` and no preview was chosen, hit-test the tile under the cursor with `panel::hit` and call `jump()` for a real, different slot (`release_on_tile`)
- [ ] 1.2 Unit-test the "+ New" and current-slot cases in the pure logic (not done: the decision lives in `main.rs`, which has no tests; owner's manual test covers it)

## 2. Gesture spike (item 2, done)

- [x] 2.1 Throwaway binary (`scratchpad/touchspike`, outside `src/`) that registers Raw Input for the touchpad and logs contact reports
- [x] 2.2 Owner ran it: Raw Input delivers touchpad contacts (report ID 3, up to 5 contacts, contact count near the end); Windows still opens Task View on the swipe
- [x] 2.3 Toggled the setting in Settings > Bluetooth & devices > Touch and diffed `HKCU`: `PrecisionTouchPad\ThreeFingerSlideEnabled` went from absent to `0`
- [x] 2.4 Decision: consented setting change (`design.md` "Windows setting")
- [x] 2.5 Confirmed with `ThreeFingerSlideEnabled = 0`: the spike still receives three-contact reports on swipes, and Task View no longer opens

## 3. Swipe detection and mode

- [x] 3.1 Raw Input registration and three-finger up/down classification in `src/touchpad.rs` (`register`, `on_raw_input`, `Tracker`); the gesture is returned from `WM_INPUT` directly, not posted
- [x] 3.2 `PANEL_LATCH` in `src/hooks.rs`; mouse hook uses `CHORD || PANEL_LATCH` for swallowing and move posting, and the chord reset stays CHORD-only
- [x] 3.3 `on_gesture` in `src/main.rs`: swipe up sets the latch and calls `show_panels()` with no delay
- [x] 3.4 `end_panel_mode()`: clears the latch, hover, presses; collapses the exposé; hides the panels. Used by Esc, swipe down, the Ctrl+Alt release, a click on empty space, and after a tile or preview click (`after_panel_click`)
- [x] 3.5 Esc while the latch is set: keyboard hook posts `WM_PANEL_ESC` (the key still reaches the application)
- [x] 3.6 `shutdown()` clears the latch; the process-scoped hooks are removed by the OS on crash
- [x] 3.7 `WM_CHORD_UP` while the latch is set goes through `end_panel_mode`
- [x] 3.8 `on_ldown` with no panel hit in swipe mode calls `end_panel_mode` and starts no window drag
- [x] 3.9 The mouse-hook reset is CHORD-only (only the swallow checks use `CHORD || PANEL_LATCH`)

## 4. Windows setting (consent)

- [x] 4.1 One-time in-app Yes/No prompt at startup (`touchpad::startup`); the answer is recorded in `%LOCALAPPDATA%\screen-shifter\touchpad.txt`, which also holds the value to restore, written before any change
- [x] 4.2 On consent, `ThreeFingerSlideEnabled` is set to `0`; restored on Exit (`shutdown`), on panic (panic hook), and on the next start if a run died mid-change
- [x] 4.3 Decline leaves the setting untouched and the swipe disabled
- [x] 4.4 Tray item "Three-finger swipe opens panel" (checked when on) turns the feature and the setting on or off

## 5. Specs and verification

- [ ] 5.1 Confirm the delta specs in `specs/slot-panel` and `specs/touchpad-panel-gesture` match the implementation (the sample empty-space click between tiles on the panel is treated as empty space only if the owner wants it; see notes)
- [x] 5.2 `cargo test` (53 passed, including six touchpad tests) and `cargo clippy --all-targets -- -D warnings` clean
- [ ] 5.3 Owner tests manually: release over tile; swipe up opens; hover spread; click tile; click preview; each exit (Esc, empty click, swipe down, Ctrl+Alt tap); input normal after each exit; tray toggle on and off; Windows setting restored on Exit

## 6. Left/right switch and swipe-down focus (added after the first test)

- [x] 6.1 Tracker reads horizontal travel; swipe left/right classify as `GESTURE_LEFT`/`GESTURE_RIGHT` (larger-axis rule)
- [x] 6.2 Left/right call `on_switch` on the cursor's monitor; swipe left is next and right is previous, matching the slide animation (reversed from the first pass; the arrow keys are unchanged); the panel stays open
- [x] 6.3 Swipe down with the panel open chooses the hovered preview (`choose`), then closes; with no preview it only closes
- [x] 6.4 Tests for left, right, and diagonal-follows-larger-axis (55 passed; clippy clean)
- [ ] 6.5 Owner tests: swipe left/right with and without the panel; swipe down over a window in the spread

## 7. Exposé on every screen, and back to the panel

- [x] 7.1 `expose::State.open` holds one spread per monitor; `hovered` and previews are keyed by window id (unique across screens)
- [x] 7.2 Collapse is monitor-scoped for hover (`open` replaces this monitor's spread only); a Ctrl+Alt open still collapses the others (`open_expose` outside the swipe mode)
- [x] 7.3 Swipe up with the panel up calls `open_all_spreads`, spreading each monitor's current slot
- [x] 7.4 Hover in the swipe mode keeps every monitor's spread; moving between monitors does not collapse
- [x] 7.5 Swipe down with a spread open and no preview collapses every spread and keeps the panel; a second swipe down closes the panel
- [x] 7.6 Compiles under `cargo clippy --all-targets -- -D warnings`; 55 tests pass
- [ ] 7.7 Owner tests: swipe up spreads both screens; hover between screens; choose a preview; swipe down over a tile returns to the panel; second swipe down closes

## 8. Live phases for the three-finger swipe

- [x] 8.1 `touchpad::Tracker` reports `Begin`, live `Vertical(dy)`, and a one-time `Horizontal(dx)` as the fingers move; the first axis past 150 units is the swipe's axis
- [x] 8.2 `touchpad::target(start, dy)`: panel at 100 units, exposé at 250, with reversal undoing the phase crossed; `goto` steps the screen there in at most two actions
- [x] 8.3 Left and right switch once when passing 150 units (`on_swipe`), not on release
- [x] 8.4 Swipe down from the exposé past 350 units chooses the hovered preview, as it did on lift (`goto`)
- [x] 8.5 Tests for each threshold, reversal, the starting phases, and the one-time horizontal event (57 passed; clippy clean)
- [ ] 8.6 Owner tests the new thresholds (100 and 250) and tunes `PANEL_UNITS`/`EXPOSE_UNITS` if needed
