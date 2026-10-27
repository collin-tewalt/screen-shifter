# packaging Specification

## Purpose
TBD - created by archiving change packaging-v5. Update Purpose after archive.
## Requirements
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
The installed app SHALL offer to start when the user signs in, and SHALL NOT enable that by itself. On the first launch of an installed package whose startup setting is off, the system SHALL ask once whether to start at sign-in, enable it only on Yes, and remember that it asked. An install where it is already on, or turned off in Windows Settings or by policy, SHALL NOT be asked. The tray menu SHALL show a "Start with Windows" item reflecting the current setting and toggling it. If the user turned it off in Windows Settings, the item SHALL direct them there to turn it back on. In a build that isn't installed as a package, the item SHALL be shown disabled and nothing is asked.

#### Scenario: Asked on first launch
- **WHEN** the user installs the package and launches it for the first time
- **THEN** they are asked whether to start it at sign-in; on Yes it starts at the next sign-in, on No it doesn't

#### Scenario: Asked only once
- **WHEN** the user answered the question and launches the app again
- **THEN** nothing is asked

#### Scenario: Existing install
- **WHEN** an install that already starts at sign-in is updated to this version
- **THEN** nothing is asked and it still starts at sign-in

#### Scenario: Turn off from the tray
- **WHEN** "Start with Windows" is checked and the user clicks it
- **THEN** it becomes unchecked and the app no longer starts at sign-in

#### Scenario: Turned off in Settings
- **WHEN** the user disabled the app in Settings → Apps → Startup and opens the tray menu
- **THEN** the item is unchecked and clicking it opens the Startup settings page

#### Scenario: Development build
- **WHEN** the app runs from `cargo run`
- **THEN** the item is shown disabled and nothing is asked

### Requirement: A Store build
The packaging command SHALL also build an unsigned package for Microsoft Store submission, with the package name, publisher, publisher display name and display name reserved in Partner Center (kept in `packaging/store.json`) and version `a.b.c.0`. It SHALL refuse to build while those values are placeholders, and the package for upload SHALL be unsigned; a signed copy, only for running the certification kit locally, MAY also be written. The display name SHALL come from the build's identity in every place the manifest shows a name, so the Store build is "Screen Shifter" and the sideload build is distinguishable from it.

#### Scenario: Store package
- **WHEN** `packaging/store.json` holds the reserved identity and the developer runs the Store build with `Cargo.toml` at 0.10.0
- **THEN** `dist/` contains an unsigned Store MSIX at 0.10.0.0 with that identity and the display name "Screen Shifter"

#### Scenario: Copy for the certification kit
- **WHEN** the developer runs the Store build with a signing certificate
- **THEN** `dist/` contains the unsigned package for upload and, separately named, a signed copy for the certification kit

#### Scenario: Not reserved yet
- **WHEN** `packaging/store.json` still holds placeholders
- **THEN** the Store build stops with a message naming the Partner Center page to copy the values from

### Requirement: A privacy policy
The project SHALL publish a privacy policy stating that the app collects, stores and transmits no personal information and makes no network connections, and describing what it keeps on the device and how to remove it. The README SHALL link to it.

#### Scenario: Linked
- **WHEN** a user opens the README
- **THEN** it links to the privacy policy

