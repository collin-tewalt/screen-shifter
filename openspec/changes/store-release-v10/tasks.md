## 0. Owner, in parallel (outside the repo)

- [ ] 0.1 Register as an individual developer at storedeveloper.microsoft.com (free; ID and selfie verification)
- [ ] 0.2 Reserve the name "Screen Shifter" in Partner Center; copy the *Product identity* values (Package/Identity/Name, Publisher, PublisherDisplayName) for `packaging/store.json`

## 1. Package

- [ ] 1.1 `packaging/AppxManifest.xml`: `{{DISPLAY_NAME}}` in Properties, VisualElements and the startup task; startup task `Enabled="false"`; `MaxVersionTested="10.0.26200.0"`
- [ ] 1.2 `packaging/store.json` with placeholders; `tools/package.py --store` per design §1 (reads it, refuses placeholders and `--sign`, revision 0, `-store.msix` output); dev default display name "screen-shifter (dev)"
- [ ] 1.3 Build both packages; check the Store manifest's identity and names

## 2. Start with Windows asks first

- [ ] 2.1 `src/main.rs`: one modal helper used by the tray menu and both questions (`anim::clear_all`, `MODAL`, then the box or menu); a posted `WM_APP + n` (no clash with `WM_TRAY` or `hooks::WM_*`) after startup finishes asks the touchpad question (moved off the inline startup path) and then the startup question, never nested
- [ ] 2.2 Startup question per design §2: only for an installed package whose task is `Disabled` with no record; Yes → `RequestEnableAsync`; record `asked=yes|no` only on a real answer, `asked=skipped` for `Enabled`/`DisabledByUser`/policy states, nothing if the state can't be read; nothing after `shutdown`
- [ ] 2.3 Touchpad question: record only on a real answer (same fix)
- [ ] 2.4 The record's parse/format as a pure tested function
- [ ] 2.5 User-facing name "Screen Shifter" in the tray tooltip, message box titles and both questions

## 3. Policy and listing

- [ ] 3.1 `PRIVACY.md` per design §3; README: privacy link, and the tray text says the first launch asks once (no longer "on by default"); `tools/package.py` docstring and AGENTS.md build notes gain `--store`
- [ ] 3.2 `packaging/store/listing.md`, `certification.md`, `screenshots.md` per design §4
- [ ] 3.3 AGENTS.md "Store release" section: owner steps, `package.py --store`, the certification kit command, uninstall the dev package before trying a Store one, agents never submit

## 4. Verify and finish

- [ ] 4.1 `cargo test` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 Owner runs the Windows App Certification Kit on the Store package (admin terminal); failures fixed here
- [ ] 4.3 Owner manual test: fresh install of the sideload package asks once (after the touchpad question if that one is due), Yes and No paths; upgrade test: v0.9 installed with start-at-sign-in on, install this version over it, not asked, still starts after signing out and in; Ctrl+Alt while a question is up doesn't freeze anything
- [ ] 4.4 Owner: screenshots, age rating questionnaire, listing pasted, submission (agents never submit); certification feedback goes into this change
