## ADDED Requirements

### Requirement: The app has its own icon
The system SHALL use the screen-shifter icon (a monitor whose previous screen slides out past its left edge) for its tray icon, at a size matching the display scaling, and for every logo of the installed package (Start menu, taskbar, Settings, Store). At 16 px the sliding sheet MUST remain recognizable.

#### Scenario: Tray icon
- **WHEN** the app is running on a monitor at 100% or 150% scaling
- **THEN** the notification area shows the screen-shifter icon, sharp at that size

#### Scenario: Start menu
- **WHEN** the package is installed
- **THEN** screen-shifter appears in the Start menu with its icon and can be launched from there

### Requirement: One command builds the package
The project SHALL provide a single command that builds the release exe and produces an MSIX package whose version is derived from `Cargo.toml` (`a.b.c` → `a.b.c.0`), runs with full trust on Windows 10 version 2004 or later, and can be signed for local sideloading.

#### Scenario: Build
- **WHEN** the developer runs the packaging command with `Cargo.toml` at version 0.5.0
- **THEN** `dist/` contains an MSIX at version 0.5.0.0 that installs and runs

### Requirement: Start with Windows
The installed app SHALL start when the user signs in, enabled by default after the first launch. The tray menu SHALL show a "Start with Windows" item reflecting the current setting and toggling it. If the user turned it off in Windows Settings, the item SHALL direct them there to turn it back on. In a build that isn't installed as a package, the item SHALL be shown disabled.

#### Scenario: Default on
- **WHEN** the user installs the package, launches it once, and signs out and in again
- **THEN** screen-shifter is running

#### Scenario: Turn off from the tray
- **WHEN** "Start with Windows" is checked and the user clicks it
- **THEN** it becomes unchecked and the app no longer starts at sign-in

#### Scenario: Turned off in Settings
- **WHEN** the user disabled screen-shifter in Settings → Apps → Startup and opens the tray menu
- **THEN** the item is unchecked and clicking it opens the Startup settings page

#### Scenario: Development build
- **WHEN** the app runs from `cargo run`
- **THEN** the item is shown disabled
