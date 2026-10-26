## ADDED Requirements

### Requirement: Elevation capability is a build option
Packages SHALL declare the `allowElevation` capability by default, and the packaging command SHALL offer a flag that builds without it. The exe SHALL run as invoker in every build, so it never elevates by itself.

#### Scenario: Default build
- **WHEN** the developer builds a package without the flag
- **THEN** its manifest declares `allowElevation` and the installed app offers "Run as administrator" to an administrator

#### Scenario: Without elevation
- **WHEN** the developer builds with the no-elevation flag
- **THEN** the manifest doesn't declare it and the installed app offers no administrator items
