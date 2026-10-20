## Context

`tools/package.py` already takes `--name`, `--publisher`, `--publisher-display` and `--revision` and signs only with `--sign`; the manifest template fills `{{NAME}}`, `{{PUBLISHER}}`, `{{PUBLISHER_DISPLAY}}`, `{{VERSION}}`, but `DisplayName` is hard-coded `screen-shifter` in `<Properties>` and `<uap:VisualElements>` (and the startup task's `DisplayName`). The startup task is declared `Enabled="true"`, so Windows enables it at the first launch; `main.rs` reads its state for the tray item (`startup_task`, `startup_item`, `toggle_startup`, with `RequestEnableAsync`, which shows no consent UI for desktop apps). The touchpad consent (`touchpad.rs`) is the pattern for a one-time question recorded in a file in the state directory. Store Policy 10.2 (version 7.20) requires supported methods and user consent to change the user's Windows experience; 10.5 requires a privacy policy only if personal information is accessed, collected or transmitted.

## Goals / Non-Goals

**Goals:** a package Partner Center accepts and certification passes; consent for start-at-sign-in; listing and reviewer materials ready to paste.

**Non-Goals:** the community winget repo, SignPath, CI (later change); submitting (the owner does it in Partner Center; agents never do); paid listing, ads, in-app purchases.

## Decisions

### 1. One template, two identities

`packaging/store.json` holds `{"name", "publisher", "publisher_display", "display_name"}` exactly as Partner Center's *Product identity* page shows them (they are public: they appear in every installed package). `--store` reads it, forces revision 0 and refuses `--sign` (the Store signs), and refuses to run while any value is still a placeholder. The dev defaults stay, with `display_name` "screen-shifter (dev)", so the dev and Store packages are distinguishable in Start and Settings (they have different package family names and can both be installed; only one runs at a time, by the existing mutex). `{{DISPLAY_NAME}}` replaces the three hard-coded display names. Output: `dist/screen-shifter-<version>-x64-store.msix`.

### 2. Start with Windows asks once

Manifest `Enabled="false"`. On start, an installed package whose task is `Disabled` and that has no record of having asked shows a Yes/No message box (as the touchpad question does) once the app is running (after the tray icon exists, never before windows are adopted): "Start Screen Shifter when you sign in? You can change this any time from the tray icon." Yes calls `RequestEnableAsync`. The answer is recorded in `startup.txt` in the state directory (`asked=yes|no`), and the question is never repeated. A task already `Enabled` (an existing install), `DisabledByUser` or by policy is recorded as asked without asking. Development builds (no package) never ask. The hooks are live by then, so the question is not shown inline during startup: a message is posted to the app window and the box is shown from the message loop, like the tray menu's modal loop, which the hooks already cope with.

### 3. Privacy policy

`PRIVACY.md` at the repo root, linked from the README; its GitHub URL goes into Partner Center (the repo is public). Content: no personal information collected, stored or transmitted; no network access; what stays on the device (the state file of window handles, process ids and window classes in the package's local app data, deleted on Restore all; window pictures and icons only in memory; the touchpad and startup answers); how to remove it (uninstall).

### 4. Listing and reviewer notes

`packaging/store/listing.md` (description, short description, up to 20 features, 7 keywords, release notes, category Utilities & tools, free, all markets) and `packaging/store/certification.md` (notes for testers: how to try it with two monitors and with one; what `runFullTrust` is for: a Win32 tray app that shows and hides other applications' top-level windows; that `WH_KEYBOARD_LL` only watches for the Ctrl+Alt chord and Esc and `WH_MOUSE_LL` only acts while the panel is up, nothing is recorded or transmitted; that hidden windows are restored on Exit, sign-out, update, uninstall and crash; the touchpad setting is changed only after a Yes and restored on Exit; startup asks first). `packaging/store/screenshots.md`: the shots to take (panel on two monitors, exposé with icons, slot x, drag to a tile), size 1920×1080 or larger, nothing personal on screen.

### 5. Certification kit

`appcert.exe` from the Windows SDK, run by the owner in an admin terminal on the Store package (the kit installs and launches the package). Documented in AGENTS.md with the exact command. Failures become tasks in this change.

## Risks / Trade-offs

- **Reviewers may still object to hiding other apps' windows or to the hooks.** Mitigation: explicit certification notes; the behavior is user-initiated and reversible. If rejected, the report goes into this change.
- **New installs no longer start at sign-in unless the user says Yes.** Accepted for consent; the question is shown once, at first launch.
- **Two installed packages (dev and Store)** can confuse the owner; AGENTS.md says to uninstall the dev package before trying the Store one.
- **Identity values change the package family name**, so the Store install is a different app from the sideloaded one: its state directory is separate (MSIX redirects app data per package), so no state crosses over.
