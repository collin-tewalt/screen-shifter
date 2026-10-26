## 1. Spike (owner's machine, test windows only of processes the spike launched)

- [ ] 1.1 Hooks unelevated vs an admin window with focus (Ctrl+Alt seen?)
- [ ] 1.2 Relaunch elevated with package identity: `runas` on the exe path, an execution alias, `shell:AppsFolder\<PFN>!ScreenShifter`; with and without `allowElevation`; arguments arrive; `ShellExecuteExW` returns a real process handle
- [ ] 1.3 The elevated copy resolves the same state folder and can replace files the unelevated copy wrote, and vice versa
- [ ] 1.4 Startup task with the capability declared still launches unelevated; `StartupTask` API works when elevated
- [ ] 1.5 Integrity level of a window's process from an unelevated process
- [ ] 1.6 `CreateMutexW` from unelevated on an elevated copy's mutex: which error
- [ ] 1.7 `TaskbarCreated` and the tray callback while elevated; `ChangeWindowMessageFilterEx`
- [ ] 1.8 Write the findings into design.md; short review round if the design changes

## 2. Core: stranded entries

- [ ] 2.1 `core.rs`: a stranded list beside the slots; `adopt` routes entries marked stranded there (never to `show`); `serialize`, `restore_all` and the monitor-set reset keep it; an elevated run's adopt takes them back as ordinary entries; tests for each

## 2b. Elevation in the app

- [ ] 2b.1 `win.rs`: this process is elevated; elevation type (`TokenElevationTypeLimited`); a window's process integrity level vs ours (spike 1.5); stranded = exists, same app, higher integrity, not visible
- [ ] 2b.2 Single instance: `ERROR_ACCESS_DENIED` (as the `Err` from `CreateMutexW`) also means already running
- [ ] 2b.3 Elevation offered (design §5): not elevated, Admin Approval Mode, build can elevate (dev: yes; packaged: a real `allowElevation` capability element in the installed manifest)
- [ ] 2b.4 Relaunch per design §2: through `modal()` with a re-entrancy guard; `runas` with `--takeover <pid>`; exit only on TRUE with a process handle; the new copy waits for the old pid before the panic hook and the mutex (≤ 10 s, else log and exit untouched)
- [ ] 2b.5 Tray: "Run as administrator", "Always run as administrator" check item, tooltip "(administrator)" when elevated
- [ ] 2b.6 "Always": `elevate.txt` record (pure tested parse/format); relaunch asked once after startup through the `WM_ASK` path, after the one-time questions; turning it on offers the relaunch; with "always" on the stranded question isn't asked separately
- [ ] 2b.7 Stranded question per design §4 (only when elevation is offered and "always" is off; logged in builds that can't elevate)
- [ ] 2b.8 While elevated: `ChangeWindowMessageFilterEx` for `TaskbarCreated` (and the tray callback per spike 1.7)

## 3. Packaging

- [ ] 3.1 `{{ELEVATION_CAPABILITY}}` in the manifest template (always replaced, empty under `--no-elevation`; header comment updated); `package.py --no-elevation`; any alias the spike needs
- [ ] 3.2 `packaging/store/certification.md`: the `allowElevation` justification

## 4. Verify and finish

- [ ] 4.1 `cargo test` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 README (tray menu, admin windows, stranded case), AGENTS.md, ROADMAP.md (Now)
- [ ] 4.3 Owner manual test: run as admin (approve and decline), admin terminal in a slot, Exit while elevated shows it, "always" at sign-in, a second launch while elevated, a `--no-elevation` package has no items, stranded case (the owner ends the elevated app from Task Manager with only their own test admin window hidden), an admin window that stayed visible asks nothing, Explorer restart while elevated keeps the tray, declining the "always" prompt at sign-in asks nothing more that session, a standard user sees no items
- [ ] 4.4 Certification kit on the Store build with the capability; submit with the justification; fall back to `--no-elevation` if refused
