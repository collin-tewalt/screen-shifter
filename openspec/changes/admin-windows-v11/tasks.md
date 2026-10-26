## 1. Spike (owner's machine, test windows only of processes the spike launched)

- [ ] 1.1 Hooks unelevated vs an admin window with focus (Ctrl+Alt seen?)
- [ ] 1.2 Relaunch elevated with package identity: `runas` on the exe path, an execution alias, `shell:AppsFolder\<PFN>!ScreenShifter`; with and without `allowElevation`
- [ ] 1.3 Startup task with the capability declared: still launches unelevated
- [ ] 1.4 Detect a window of an elevated process from an unelevated one
- [ ] 1.5 `CreateMutexW` from unelevated on an elevated copy's mutex: which error
- [ ] 1.6 Write the findings into design.md (§1, §2, §4); short review round if the design changes

## 2. Elevation in the app

- [ ] 2.1 `win.rs`: this process is elevated; a window's process is elevated (spike 1.4)
- [ ] 2.2 Single instance: `ERROR_ACCESS_DENIED` also means already running
- [ ] 2.3 Can this build elevate (dev: yes; packaged: `allowElevation` in the installed manifest)
- [ ] 2.4 Relaunch per design §2: `ShellExecuteExW` `runas` with `--takeover <pid>`; declined/failed changes nothing; approved → normal Exit; the new copy waits for the old pid (≤ 10 s) before starting
- [ ] 2.5 Tray: "Run as administrator" (unelevated, can elevate), "Always run as administrator" check item, tooltip "(administrator)" when elevated
- [ ] 2.6 "Always": `elevate.txt` record (pure tested parse/format); relaunch asked once after startup through the `WM_ASK` path, after the one-time questions; turning it on while unelevated offers the relaunch
- [ ] 2.7 Stranded admin windows (design §4): collected during adoption, the question after startup, asked every start until restored, never dropped from the state file

## 3. Packaging

- [ ] 3.1 `{{ELEVATION_CAPABILITY}}` in the manifest template; `package.py --no-elevation`; any alias the spike needs
- [ ] 3.2 `packaging/store/certification.md`: the `allowElevation` justification

## 4. Verify and finish

- [ ] 4.1 `cargo test` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 README (tray menu, admin windows, stranded case), AGENTS.md, ROADMAP.md (Now)
- [ ] 4.3 Owner manual test: run as admin (approve and decline), admin terminal in a slot, Exit while elevated shows it, "always" at sign-in, a second launch while elevated, a `--no-elevation` package has no items, stranded case (end the elevated app from Task Manager only with the owner's own test window open)
- [ ] 4.4 Certification kit on the Store build with the capability; submit with the justification; fall back to `--no-elevation` if refused
