## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: A Store build
The packaging command SHALL also build an unsigned package for Microsoft Store submission, with the package name, publisher, publisher display name and display name reserved in Partner Center (kept in `packaging/store.json`) and version `a.b.c.0`. It SHALL refuse to build while those values are placeholders and SHALL NOT sign it. The display name SHALL come from the build's identity in every place the manifest shows a name, so the Store build is "Screen Shifter" and the sideload build is distinguishable from it.

#### Scenario: Store package
- **WHEN** `packaging/store.json` holds the reserved identity and the developer runs the Store build with `Cargo.toml` at 0.10.0
- **THEN** `dist/` contains an unsigned Store MSIX at 0.10.0.0 with that identity and the display name "Screen Shifter"

#### Scenario: Not reserved yet
- **WHEN** `packaging/store.json` still holds placeholders
- **THEN** the Store build stops with a message naming the Partner Center page to copy the values from

### Requirement: A privacy policy
The project SHALL publish a privacy policy stating that the app collects, stores and transmits no personal information and makes no network connections, and describing what it keeps on the device and how to remove it. The README SHALL link to it.

#### Scenario: Linked
- **WHEN** a user opens the README
- **THEN** it links to the privacy policy
