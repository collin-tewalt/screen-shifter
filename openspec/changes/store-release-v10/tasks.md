## 0. Owner, in parallel (outside the repo)

- [ ] 0.1 Register as an individual developer at storedeveloper.microsoft.com (free; ID and selfie verification)
- [ ] 0.2 Reserve the name "Screen Shifter" in Partner Center; copy the *Product identity* values (Package/Identity/Name, Publisher, PublisherDisplayName) for `packaging/store.json`

## 1. Package

- [x] 1.1 `packaging/AppxManifest.xml`: `{{DISPLAY_NAME}}` in Properties, VisualElements and the startup task; startup task `Enabled="false"`; `MaxVersionTested="10.0.26200.0"`; the template header comment (lists every placeholder) and the startup task comment
- [x] 1.2 `packaging/store.json` with placeholders; `tools/package.py --store` per design §1 (reads it; refuses placeholders, a nonzero `--revision` and explicit identity flags; the uploaded `-store.msix` is never signed; with `--sign` also writes the kit-only `-store-kit.msix`); `tools/dev_cert.ps1 -Subject -Out` (default output stays `dev-cert.*`) and `tools/trust_dev_cert.ps1 -Cer`, `-Remove -Subject`; `--display-name` (refused with `--store`); the placeholder message names *Product identity*; dev default display name "screen-shifter (dev)"; docstring updated
- [x] 1.3 Build both packages; check the Store manifest's identity and names

## 2. Start with Windows asks first

- [x] 2.1 `src/main.rs`: one modal helper used by the tray menu and both questions (`anim::clear_all`, `MODAL.replace(true)` saved and restored, foreground first); `ASKING` makes `WM_TRAY` bring the question to the front instead of opening the menu; a question arriving during a modal is re-posted after it; message id `WM_APP + 2`; `SHUTTING_DOWN` set at the start of `shutdown`, checked after each box (answer ignored, nothing recorded or applied, no next question); a posted `WM_APP + n` (no clash with `WM_TRAY` or `hooks::WM_*`) after startup finishes asks the touchpad question (moved off the inline startup path) and then the startup question, never nested
- [x] 2.2 Startup question per design §2: only for an installed package whose task is `Disabled` with no record; Yes → `RequestEnableAsync`; record `asked=yes|no` only on a real answer, `asked=skipped` for `Enabled`/`DisabledByUser`/policy states, nothing if the state can't be read; nothing after `shutdown`
- [x] 2.3 Touchpad question moved to the posted path (design §2: registration, `put_back` and an already-given consent stay inline; Yes → `set_enabled(true)`); record only on a real answer and never after shutdown
- [x] 2.4 The record's parse/format as a pure tested function
- [x] 2.5 User-facing name "Screen Shifter" in the tray tooltip, message box titles and both questions

## 3. Policy and listing

- [x] 3.1 `PRIVACY.md` per design §3; README: privacy link, and the tray text says the first launch asks once (no longer "on by default"); `tools/package.py` docstring and AGENTS.md build notes gain `--store`
- [x] 3.2 `packaging/store/listing.md`, `certification.md`, `screenshots.md` per design §4
- [x] 3.3 AGENTS.md "Store release" section: owner steps, `package.py --store`, the certification kit command with the Store-publisher certificate made, trusted and removed again afterwards, uninstall the dev package before trying a Store one, agents never submit

## 4. Verify and finish

- [x] 4.1 `cargo test` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 Owner makes and trusts the Store-publisher certificate, runs the Windows App Certification Kit on `-store-kit.msix` (admin terminal); failures fixed here
- [ ] 4.3 Owner manual test: fresh install of the sideload package asks once (after the touchpad question if that one is due), Yes and No paths; upgrade test: v0.9 installed with start-at-sign-in on, install this version over it, not asked, still starts after signing out and in; Ctrl+Alt while a question is up doesn't freeze anything; right-clicking the tray icon while a question is up does nothing; signing out with a question up leaves the touchpad setting as it was
- [ ] 4.4 Owner: screenshots, age rating questionnaire, listing pasted, submission (agents never submit); certification feedback goes into this change
