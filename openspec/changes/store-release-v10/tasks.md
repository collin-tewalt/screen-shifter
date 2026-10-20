## 0. Owner, in parallel (outside the repo)

- [ ] 0.1 Register as an individual developer at storedeveloper.microsoft.com (free; ID and selfie verification)
- [ ] 0.2 Reserve the name "Screen Shifter" in Partner Center; copy the *Product identity* values (Package/Identity/Name, Publisher, PublisherDisplayName) for `packaging/store.json`

## 1. Package

- [ ] 1.1 `packaging/AppxManifest.xml`: `{{DISPLAY_NAME}}` in Properties, VisualElements and the startup task; startup task `Enabled="false"`; `MaxVersionTested="10.0.26200.0"`
- [ ] 1.2 `packaging/store.json` with placeholders; `tools/package.py --store` per design §1 (reads it, refuses placeholders and `--sign`, revision 0, `-store.msix` output); dev default display name "screen-shifter (dev)"
- [ ] 1.3 Build both packages; check the Store manifest's identity and names

## 2. Start with Windows asks first

- [ ] 2.1 `src/main.rs`: after startup, an installed package whose task is `Disabled` and that has no `startup.txt` record is asked once (posted to the app window, Yes/No message box); Yes → `RequestEnableAsync`; record `asked=yes|no`; `Enabled`, `DisabledByUser` and policy states are recorded without asking; no package: nothing
- [ ] 2.2 Test the record's parse/format as a pure function

## 3. Policy and listing

- [ ] 3.1 `PRIVACY.md` per design §3; README link
- [ ] 3.2 `packaging/store/listing.md`, `certification.md`, `screenshots.md` per design §4
- [ ] 3.3 AGENTS.md "Store release" section: owner steps, `package.py --store`, the certification kit command, uninstall the dev package before trying a Store one, agents never submit

## 4. Verify and finish

- [ ] 4.1 `cargo test` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 Owner runs the Windows App Certification Kit on the Store package (admin terminal); failures fixed here
- [ ] 4.3 Owner manual test: fresh install of the sideload package asks once, Yes and No paths, an existing install isn't asked
- [ ] 4.4 Owner: screenshots, age rating questionnaire, listing pasted, submission (agents never submit); certification feedback goes into this change
