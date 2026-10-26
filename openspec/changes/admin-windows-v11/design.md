## Context

The app is one process, single-instance by a `Local\screen-shifter` mutex created at startup (`main.rs`: a second copy that sees `ERROR_ALREADY_EXISTS` returns before touching anything). `win::hide`/`show` use `ShowWindowAsync`, which UIPI makes ineffective on windows of higher-integrity processes; the monitor-slots spec already says such a window "stays visible". The low-level hooks (`hooks.rs`) miss input while a higher-integrity window has focus (key-ups are already noted as missed there). Exit (`show_all` + `shutdown`) shows everything and keeps the state file, and the next start's `adopt_saved_layout` takes the saved windows back into their slots. Packaged state lives in the package's redirected local app data (`win::state_dir`), and package identity matters: `win::packaged()`, the startup task and the state directory all depend on it.

## Goals / Non-Goals

**Goals:** admin apps' windows behave like any other while the app runs elevated; elevation only ever by the user's request through UAC; the "never left hidden" rule kept, including after an elevated run dies.

**Non-Goals:** an elevated helper process (rejected: it would duplicate hooks, show/hide, placement and snapshots across two processes with a privileged channel to secure); `uiAccess` (not possible for MSIX: it requires Program Files or System32); elevating without a prompt (scheduled tasks); a seamless hand-off without the flash (later, if wanted). Elevation as a different user is not offered at all (§5): only an administrator in Admin Approval Mode gets the items, so the elevated copy is always the same user with the same state folder.

## Decisions

### 0. Spike first

Before anything else, a throwaway spike on the owner's machine, with test windows only of processes it launched itself (AGENTS.md):
1. With the app unelevated, do the hooks see Ctrl+Alt while an admin window has focus? (Expected no; confirms the motivation.)
2. How a packaged full-trust app relaunches itself elevated *with its package identity*: `ShellExecuteExW` `runas` on the installed exe path (identity likely lost), on an app execution alias (`uap5:AppExecutionAlias`), or on `shell:AppsFolder\<PFN>!ScreenShifter`; with and without `allowElevation`. For the chosen route also: the command-line arguments (`--takeover <pid>`) arrive intact, and `ShellExecuteExW` with `SEE_MASK_NOCLOSEPROCESS` returns TRUE with a real process handle only once the elevated process exists.
3. **Same state.** The elevated copy resolves `win::state_dir()` to the same physical folder as the unelevated one (package identity kept, the same MSIX app-data redirection), and each copy can replace (write-to-temp-and-rename) files the other created (owner and ACL of files written elevated). The hand-off depends on this.
4. What the startup task does with the capability declared (it must still launch unelevated: the exe stays `asInvoker`), and that the `StartupTask` API works from an elevated process.
5. From an unelevated process: the integrity level of a window's process (`OpenProcess` limited query, `OpenProcessToken`, `TokenIntegrityLevel`; what an access denial means).
6. `CreateMutexW` from an unelevated copy on the mutex an elevated copy created: the error returned (windows-rs returns it as `Err`).
7. While elevated: is the registered `TaskbarCreated` message (Explorer restart) and the tray callback delivered, and does `ChangeWindowMessageFilterEx(..., MSGFLT_ALLOW)` fix it?

The spike's findings are written into this design before implementation (a short review round if they change it).

### 1. Single instance across elevation

An unelevated `CreateMutexW` on the mutex an elevated copy created may fail with `ERROR_ACCESS_DENIED` instead of `ERROR_ALREADY_EXISTS` (the elevated token's default DACL grants only Administrators and SYSTEM). Both mean "already running": the second copy exits without touching anything. windows-rs returns the failure as `Err`, so the code is checked there, not with a later `GetLastError`. (Spike item 6.) A third copy started in the instant between the old copy's exit and the new copy's lock wins, and the elevated copy exits; nothing is hidden at that moment, so nothing is stranded.

### 2. The relaunch (user-initiated)

Tray **Run as administrator** (shown only when not elevated and elevation is offered, §5) calls `ShellExecuteExW` with the `runas` verb and `SEE_MASK_NOCLOSEPROCESS` on the route from spike item 2, passing `--takeover <pid>`. `ShellExecuteExW("runas")` waits for the user's answer and doesn't pump the app's messages, and at sign-in the prompt may sit behind a flashing taskbar button for a long time, so it never runs on the UI thread (hotkeys and hook messages would pile up, and an unanswered `WM_QUERYENDSESSION` at logoff would skip showing the windows). It runs on a worker thread (COM initialized with `COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE`, as `ShellExecuteEx` requires), which posts the result (success with a process handle, or failure) to the app window; a `RELAUNCHING` flag blocks a second relaunch while it is pending, and the result is ignored if `SHUTTING_DOWN` is set. Then:
- **Declined or failed** (`FALSE`, `ERROR_CANCELLED`, or no process handle): nothing happens; the app keeps running unelevated.
- **Approved** (`TRUE` with a real process handle, as posted by the worker): the unelevated copy runs its normal Exit path (`show_all`, then `shutdown`: every window shown, the state file kept).
- **The new copy**, started with `--takeover <pid>`, waits for that process to exit (`OpenProcess(SYNCHRONIZE)`, at most 10 s) as the very first thing in `main`, before the panic hook and before `CreateMutexW`. If the old process is already gone (`OpenProcess` fails), it proceeds at once. If the wait times out, it logs and exits without touching anything (the old copy is still running and in charge). If the old pid was already gone and reused by an unrelated process, the wait may also time out: the hand-off then fails safely (both copies gone or the old one still in charge, never windows hidden without an app); accepted rather than passing creation times. Then it starts normally: takes the lock and adopts the saved layout, hiding the slots' windows again, now including admin ones. The flash of all windows is accepted for now.
- If the elevated copy fails to start after the old one exited, every window is already shown: nothing is stranded. If it panics after adopting, its panic hook shows from the state file, and as an elevated process it can show admin windows too.

If the user chooses Exit while the UAC prompt is still up and then approves it, the elevated copy may still start (its wait succeeds at once) and take the layout back, or Windows may not start it once the requester is gone; either way everything is already shown. The Approved handler is the same path as `WM_CLOSE` (`show_all`, `shutdown`), which already copes with an open menu or question box. The `runas` call reintroduces `ShellExecuteExW`, which the certification kit flags in its optional "blocked executables" test (overall WARNING, not FAIL); accepted.

### 3. Always run as administrator

A tray check item **Always run as administrator**, recorded in `elevate.txt` in the state directory (`always=yes|no`; parse/format a pure tested function). When on and the app starts unelevated (including from the startup task at sign-in), it posts the relaunch to itself once startup has finished, through the same path and the same modal treatment as the one-time questions (`WM_ASK` flow, after them): UAC appears; declining keeps it unelevated for this session (an in-memory `DECLINED` flag, so neither a re-posted `WM_ASK` nor anything else prompts again). Turning it on while unelevated also offers the relaunch immediately. The relaunch prompt and the stranded question run after the one-time questions even if one of those was dismissed without an answer (only `SHUTTING_DOWN` stops the sequence). When it is on, the stranded-windows question (§4) is not asked separately: the relaunch prompt comes anyway and the elevated copy restores them. A package update restarts the app unelevated (`RegisterApplicationRestart`), which re-prompts when "always" is on; accepted.

### 4. Stranded admin windows

**When this happens.** An elevated run is ended without its exit path (a crash the panic hook doesn't catch, or killed from Task Manager) while it had admin windows hidden, and the app is started again unelevated in the same Windows session. (After a reboot the windows are gone anyway, and adoption already ignores a file from an earlier boot.)

**Which windows count.** Today a window the app couldn't hide (an admin window while unelevated) still gets recorded in a hidden slot, though it stays visible. So "elevated process" alone would flag windows that aren't stranded at all. A saved entry is *stranded* only when: the file is from the current boot; the window still exists and is the same app (`check` says Here or Elsewhere); its process runs at exactly High integrity (elevated) while this process runs at Medium (SYSTEM or protected windows, which elevation can't help with, don't count; spike item 5); **and** it is not visible (`!IsWindowVisible`). If spike 5 shows the token of an elevated process can't be read from Medium, the fallback is: the process can be opened for limited query but its token can't, plus the other conditions. A visible admin window is simply on screen: it goes to the show list like any other (showing it is a no-op).

**Routing in `adopt`.** Core gets a `stranded` predicate from the shell (like `check`) and applies it right after `check`, for both Here and Elsewhere, before the active-slot, slot-limit and missing-monitor tests (otherwise an entry in the active slot or on a gone monitor would go to `show`, silently fail, and be dropped from the file). Stranded entries go into a separate stranded list in `State`, never to `show` and never into a slot.

**Kept, never dropped.** `serialize` writes the stranded list back as ordinary `w` lines tagged with their monitor's current active slot, as `tag_active` does for windows about to be shown (no format change). The old slot numbers belong to the dead elevated run and mean nothing now, so the windows come back *shown*: an elevated start's `adopt` sends an entry in the active slot (or on a gone monitor, via Elsewhere) to the show list. An unelevated start re-classifies them as stranded first, so they stay recorded. `State::hidden()` excludes the stranded list (what is hidden or shown by this run never includes them). A window an admin app re-showed by itself and that a switch or move then captures is dropped from the stranded list, so it is never recorded twice. `restore_all` and `reset_if_monitors_changed` keep the list, and the two call sites that cleared the file (Restore all and the monitor-set reset, both `win::clear_state()`, which writes an empty `State`) call `persist(&[])` instead; `clear_state` goes away. `forget_dead`/`reap_dead` leave the list alone (the next start's `check` drops dead entries). Core tests: routing for an entry in the active slot, on a gone monitor, from another boot (not stranded), and Restore all → serialize → parse → adopt still yields them.

**The question.** After startup (the `WM_ASK` flow, after the one-time questions), if any are stranded and elevation can be requested (§5) and "always" is off: "Some windows of apps running as administrator are hidden, and Screen Shifter needs to run as administrator to show them. Run as administrator now?" Yes → the relaunch (§2); No → asked again at the next start. In a build that can't elevate they are logged (it can't show them). This is the one case the rule can't cover without the user's approval; README documents it. Known false positive: an admin window an unelevated run recorded (its hide failed) that its own app later hid (to the tray) looks stranded; Yes then shows it. Accepted and documented.

### 5. Packaging, the build flag, and who gets offered elevation

`packaging/AppxManifest.xml` gains `{{ELEVATION_CAPABILITY}}`, which `tools/package.py` always replaces: with `<rescap:Capability Name="allowElevation" />`, or with an empty string under `--no-elevation` (the template header comment lists it). The exe's embedded manifest stays `asInvoker`.

Two conditions. **The administrator items are available** when the user is an administrator who elevates as themselves (`TokenElevationType` is `TokenElevationTypeLimited` when unelevated or `TokenElevationTypeFull` when elevated; `Default` means a standard user or UAC off: nothing is shown) and the build can elevate. **A relaunch can be requested** (Run as administrator, the "always" relaunch, the stranded question) only when, in addition, this process isn't elevated. "Always run as administrator" is shown whenever the items are available, elevated or not, so it can be turned off while elevated. Requiring the user's own elevation means the elevated copy is the same user with the same state folder (a standard user would elevate as a different account with its own state, which could strand windows silently). The build can elevate: a development build (no package) always can; a packaged build parses its installed `AppxManifest.xml` (next to the exe) for an actual `rescap:Capability` element named `allowElevation` (not a substring match). `packaging/store/certification.md` gains the `allowElevation` justification: elevation only on the user's explicit request through UAC, to manage windows of apps the user runs as administrator.

**While elevated**, the app calls `ChangeWindowMessageFilterEx(app, TaskbarCreated, MSGFLT_ALLOW)` (and for the tray callback if spike item 7 shows it's filtered), so an Explorer restart doesn't leave it without a tray icon and an Exit.

### 6. Visible state

While elevated, the tray tooltip is "Screen Shifter (administrator)" and the tray menu shows **Always run as administrator** (but not **Run as administrator**).

## Risks / Trade-offs

- **Store certification may refuse `allowElevation`.** Then the Store build uses `--no-elevation` and the feature is for sideload and (later) winget builds.
- **Elevated means a bigger blast radius:** global hooks and window control at admin level. Mitigation: nothing new is added beyond running the same code elevated; elevation is always the user's choice.
- **The flash on relaunch.** Accepted; a seamless hand-off can come later.
- **Stranded admin windows after a crash** need the user's Yes to come back (§4).
- **Spike outcomes can change §1, §2 and §4**; the design is updated before implementation.
