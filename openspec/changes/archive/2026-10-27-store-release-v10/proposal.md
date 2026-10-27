## Why

screen-shifter only installs where the owner's dev certificate is trusted. The Microsoft Store is now free for individual developers and signs the package itself, which makes the app installable anywhere and also brings it to winget through the `msstore` source (`winget install <StoreId>`), without a code-signing certificate. Community-repo winget (SignPath-signed GitHub Releases) comes later, in its own change.

## What Changes

- **Store build.** `tools/package.py --store` builds an unsigned MSIX (version `a.b.c.0`) with the identity Partner Center assigns when the name is reserved (`packaging/store.json`: package name, publisher, publisher display name, display name). The manifest's display name becomes a template value, since the Store requires it to match the reserved name exactly: "Screen Shifter" for the Store; the dev build keeps "screen-shifter (dev)" style names so the two are told apart.
- **Start with Windows asks first.** Store policy requires user consent to change the Windows experience. The package's startup task is no longer enabled by itself on first launch: the first launch of an installed package asks once ("Start Screen Shifter when you sign in?"), and the answer is recorded; the tray item still toggles it afterwards. **BREAKING** for new installs only: start-at-sign-in is now opt-in. Installs where it is already on keep it on and are not asked.
- **Privacy policy.** `PRIVACY.md`: the app collects, stores and transmits no personal information and makes no network connections; it keeps a local state file of window handles to restore windows, and window pictures only in GPU memory. Linked from the README and entered in Partner Center.
- **Store listing materials** in `packaging/store/`: description, short description, feature list, keywords, release notes, the screenshot plan, and certification notes for the reviewers (why `runFullTrust`, what the low-level keyboard/mouse hooks are used for and that nothing is logged, that other apps' windows are hidden and shown only on the user's command and always restored, the consented touchpad setting, the startup prompt).
- **Certification check.** The Windows App Certification Kit is run on the Store package before submission; failures are fixed in this change.
- `MaxVersionTested` goes to the Windows version the owner tests on (10.0.26200.0).
- AGENTS.md gains a "Store release" section: the owner's Partner Center steps, the build command, the kit run, and that agents never submit.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities
- `packaging`: a Store build alongside the sideload build; start with Windows asks on first launch instead of turning itself on; a privacy policy.

## Impact

- `tools/package.py`: `--store` (reads `packaging/store.json`, no signing, revision 0), display name template.
- `packaging/AppxManifest.xml`: `{{DISPLAY_NAME}}`, startup task `Enabled="false"`, `MaxVersionTested`.
- `packaging/store.json` (new, filled in from Partner Center; not secret), `packaging/store/*.md` (new).
- `src/main.rs`: the one-time startup question for an installed package and its record in the state directory.
- `PRIVACY.md`, `README.md`, `AGENTS.md`.
- Owner-only steps outside the repo: developer account registration and identity verification, name reservation, age rating questionnaire, screenshots, submission.
