## 1. Spike (owner's machine, test windows only of processes the spike launched)

- [x] 1.1 Hooks unelevated vs an admin window with focus (Ctrl+Alt seen?); elevated: snapshots (`PrintWindow`) and icons of admin windows work
- [x] 1.2 Relaunch elevated with package identity: `runas` on the exe path, an execution alias, `shell:AppsFolder\<PFN>!ScreenShifter`; with and without `allowElevation`; arguments arrive; `ShellExecuteExW` returns a real process handle; whether it blocks or pumps, and how the prompt appears when started from a background/startup-task launch
- [x] 1.3 The elevated copy resolves the same state folder and can replace files the unelevated copy wrote, and vice versa
- [x] 1.4 Startup task with the capability declared still launches unelevated; `StartupTask` API works when elevated
- [x] 1.5 Integrity level of a window's process from an unelevated process
- [x] 1.6 `CreateMutexW` from unelevated on an elevated copy's mutex: which error
- [x] 1.7 `TaskbarCreated` and the tray callback while elevated; `ChangeWindowMessageFilterEx`
- [x] 1.8 Write the findings into design.md; short review round if the design changes

## 2. Core: stranded entries

- [ ] 2.1 `core.rs`: a stranded list beside the slots; `adopt` takes a `stranded` predicate, called with the live entry from `check` and ANDed with `same_boot` in core, applied after `check` (Here and Elsewhere), before the active-slot/limit/monitor tests, only for the current boot; `serialize`, `restore_all` and `reset_if_monitors_changed` keep the list; `forget_dead` leaves it alone; an elevated run's adopt takes them back as ordinary entries; serialized tagged with the monitor's active slot; `hidden()` excludes them; a switch or move that captures one drops it from the list; tests: active slot, gone monitor, other boot, Restore all round trip, stranded → serialize → parse → adopt with the predicate false → shown
- [ ] 2.2 `main.rs`: Restore all and the monitor-set reset call `persist(&[])` instead of `win::clear_state()`; remove `clear_state`

## 2b. Elevation in the app

- [ ] 2b.1 `win.rs`: this process is elevated; elevation type (`Limited`/`Full` = an administrator elevating as themselves); stranded = same boot, exists, same app, process exactly High while we're Medium (token unreadable → not stranded), not visible
- [ ] 2b.2 Single instance: `ERROR_ACCESS_DENIED` (as the `Err` from `CreateMutexW`) also means already running
- [ ] 2b.3 Design §5: items available (an administrator elevating as themselves) vs relaunch requestable (also not elevated)
- [ ] 2b.4 Relaunch per design §2: `ShellExecuteExW` `runas` on the installed exe path (`GetModuleFileNameW`), on a worker thread posting the result; `RELAUNCHING` flag; result ignored after `SHUTTING_DOWN`; COM initialized on the worker; `--takeover <pid>` (old process already gone → proceed); exit only on TRUE with a process handle; the new copy waits for the old pid before the panic hook and the mutex (≤ 10 s, else log and exit untouched)
- [ ] 2b.5 Tray: "Run as administrator" (relaunch requestable), "Always run as administrator" check item (items available, elevated or not), tooltip "(administrator)" when elevated
- [ ] 2b.6 "Always": `elevate.txt` record (pure tested parse/format); relaunch asked once after startup through the `WM_ASK` path, after the one-time questions (even if one was dismissed); an in-memory `DECLINED` flag stops any re-prompt that session; turning it on offers the relaunch; with "always" on the stranded question isn't asked separately
- [ ] 2b.7 Stranded question per design §4 (only when elevation is offered and "always" is off; logged otherwise)

## 3. Store notes

- [ ] 3.1 `packaging/store/certification.md`: one sentence on the user-initiated relaunch (no new capability)

## 4. Verify and finish

- [ ] 4.1 `cargo test` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] 4.2 README (tray menu, admin windows, stranded case), PRIVACY.md (elevation only on request), AGENTS.md, ROADMAP.md (Now)
- [ ] 4.3 Owner manual test: run as admin (approve and decline), admin terminal in a slot, Exit while elevated shows it, "always" at sign-in, a second launch while elevated, stranded case (the owner ends the elevated app from Task Manager with only their own test admin window hidden), an admin window that stayed visible asks nothing, Explorer restart while elevated keeps the tray, declining the "always" prompt at sign-in asks nothing more that session, a standard user sees no items
- [ ] 4.4 Certification kit on the Store build; submit with the certification note
