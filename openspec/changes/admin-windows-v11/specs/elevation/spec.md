## ADDED Requirements

### Requirement: Run as administrator only on the user's request
When the app is not elevated, its build can elevate, and the user is an administrator (approving elevation as themselves), the tray menu SHALL offer "Run as administrator". Choosing it SHALL ask Windows (UAC) to start the app elevated. If the user approves, the app SHALL restart elevated, keeping every slot. If the user declines or elevation fails, nothing SHALL change. The app MUST NOT elevate in any other way than through a UAC prompt the user approves.

#### Scenario: Approve
- **WHEN** the user chooses "Run as administrator" and approves the UAC prompt
- **THEN** the app restarts elevated with the same slots, and windows of apps running as administrator can now be switched, moved and previewed like any other

#### Scenario: Decline
- **WHEN** the user chooses "Run as administrator" and declines the UAC prompt
- **THEN** the app keeps running unelevated and nothing changes

#### Scenario: Elevated
- **WHEN** the app runs elevated
- **THEN** the tray tooltip says it is running as administrator and "Run as administrator" isn't offered

### Requirement: Always run as administrator
The tray menu SHALL offer "Always run as administrator", remembered across runs, under the same conditions as "Run as administrator". When it is on and the app starts unelevated (including at sign-in), it SHALL ask through UAC once, after its own start, to restart elevated; declining SHALL keep it running unelevated for that session without asking again.

#### Scenario: Sign-in
- **WHEN** "Always run as administrator" is on and the app starts at sign-in
- **THEN** a UAC prompt appears; on approval it runs elevated, otherwise it runs unelevated until the next start

### Requirement: One copy at a time, elevated or not
A second copy SHALL exit without touching any window whether the running copy is elevated or not.

#### Scenario: Elevated copy running
- **WHEN** the app runs elevated and the user starts it again from Start
- **THEN** the second copy exits and the elevated one keeps running

### Requirement: Admin windows are never stranded silently
When an unelevated start finds windows of apps running as administrator that an earlier elevated run left hidden (still existing, not visible), the app SHALL ask to run as administrator to restore them, and SHALL ask again at every start until they are restored. They SHALL stay recorded until then, through every switch, Restore all, and display change. An admin window that stayed visible SHALL NOT count.

#### Scenario: Admin window that stayed visible
- **WHEN** an unelevated run switched away from a slot while an admin terminal stayed on screen, and the app starts again
- **THEN** nothing is asked

#### Scenario: After a crash while elevated
- **WHEN** the elevated app hid an admin terminal, the PC lost power, and the app starts unelevated at sign-in
- **THEN** it asks to run as administrator; on Yes the terminal comes back into its slot

### Requirement: Only for administrators
The administrator items, the "always" relaunch and the stranded question SHALL be offered only to a user who is an administrator approving elevation as themselves.

#### Scenario: Standard user
- **WHEN** a standard user runs the app
- **THEN** the tray menu has no administrator items

### Requirement: Builds that can't elevate
A build that can't elevate (its package lacks the capability) SHALL offer neither tray item and SHALL NOT ask about stranded admin windows; admin apps' windows stay on screen as before.

#### Scenario: Store build without the capability
- **WHEN** the app is installed from a package built without elevation
- **THEN** the tray menu has no administrator items

### Requirement: The tray works while elevated
While elevated, the tray icon SHALL come back after Windows Explorer restarts, so Exit stays reachable.

#### Scenario: Explorer restarts
- **WHEN** the app runs elevated and Explorer restarts
- **THEN** the tray icon reappears and its menu works
