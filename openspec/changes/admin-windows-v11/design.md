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

Tray **Run as administrator** (shown only when not elevated and elevation is offered, §5) calls `ShellExecuteExW` with the `runas` verb and `SEE_MASK_NOCLOSEPROCESS` on the route from spike item 2, passing `--takeover <pid>`. The call goes through the `modal()` helper with a re-entrancy guard (like `ASKING`), since the UAC prompt can pump messages: no nested tray menu, second relaunch or switch animation while it is up. Then:
- **Declined or failed** (`FALSE`, `ERROR_CANCELLED`, or no process handle): nothing happens; the app keeps running unelevated.
- **Approved** (`TRUE` with a real process handle): the unelevated copy runs its normal Exit path (`show_all`, then `shutdown`: every window shown, the state file kept).
- **The new copy**, started with `--takeover <pid>`, waits for that process to exit (`OpenProcess(SYNCHRONIZE)`, at most 10 s) as the very first thing in `main`, before the panic hook and before `CreateMutexW`. If the wait times out, it logs and exits without touching anything (the old copy is still running and in charge). Then it starts normally: takes the lock and adopts the saved layout, hiding the slots' windows again, now including admin ones. The flash of all windows is accepted for now.
- If the elevated copy fails to start after the old one exited, every window is already shown: nothing is stranded. If it panics after adopting, its panic hook shows from the state file, and as an elevated process it can show admin windows too.

The `runas` call reintroduces `ShellExecuteExW`, which the certification kit flags in its optional "blocked executables" test (overall WARNING, not FAIL); accepted.

### 3. Always run as administrator

A tray check item **Always run as administrator**, recorded in `elevate.txt` in the state directory (`always=yes|no`; parse/format a pure tested function). When on and the app starts unelevated (including from the startup task at sign-in), it posts the relaunch to itself once startup has finished, through the same path and the same modal treatment as the one-time questions (`WM_ASK` flow, after them): UAC appears; declining keeps it unelevated for this session (no repeated prompts). Turning it on while unelevated also offers the relaunch immediately. When it is on, the stranded-windows question (§4) is not asked separately: the relaunch prompt comes anyway and the elevated copy restores them. A package update restarts the app unelevated (`RegisterApplicationRestart`), which re-prompts when "always" is on; accepted.

### 4. Stranded admin windows

**Which windows count.** Today a window the app couldn't hide (an admin window while unelevated) still gets recorded in a hidden slot, though it stays visible. So "elevated process" alone would flag windows that aren't stranded at all. A saved entry is *stranded* only when its window still exists and is the same app (`check`), its process has a higher integrity level than this process (spike item 5; not just "elevated or access denied", so SYSTEM or protected windows that elevation can't help with don't count), **and** it is not visible (`!IsWindowVisible`). A visible admin window is simply on screen: it goes to the show list like any other (where showing it is a no-op).

**Kept, never dropped.** Stranded entries are kept out of `State`'s slots: in core, `adopt` routes them into a separate stranded list (never to `show`), and `serialize` always writes that list back unchanged, also through `clear_state` (Restore all) and the monitor-set reset, until an elevated run adopts them (an elevated run's `adopt` treats them as ordinary saved entries). Core tests cover the routing and that every write keeps them. This way no switch, Restore all, display change or missing monitor erases them.

**The question.** After startup (the `WM_ASK` flow, after the one-time questions), if any are stranded and elevation is offered (§5) and "always" is off: "Some windows of apps running as administrator are hidden, and Screen Shifter needs to run as administrator to show them. Run as administrator now?" Yes → the relaunch (§2); No → asked again at the next start. In a build that can't elevate they are logged (they can't be shown by it). This is the one case the rule can't cover without the user's approval; README documents it.

### 5. Packaging, the build flag, and who gets offered elevation

`packaging/AppxManifest.xml` gains `{{ELEVATION_CAPABILITY}}`, which `tools/package.py` always replaces: with `<rescap:Capability Name="allowElevation" />`, or with an empty string under `--no-elevation` (the template header comment lists it). The exe's embedded manifest stays `asInvoker`.

Elevation is offered (both tray items, the "always" relaunch, the stranded question) only when all hold: this process isn't elevated; the user is an administrator running in Admin Approval Mode (`TokenElevationType` is `TokenElevationTypeLimited`), so the elevated copy is the same user with the same state folder (a standard user would elevate as a different account with its own state, which could strand windows silently); and the build can elevate: a development build (no package) always can; a packaged build parses its installed `AppxManifest.xml` (next to the exe) for an actual `rescap:Capability` element named `allowElevation` (not a substring match). `packaging/store/certification.md` gains the `allowElevation` justification: elevation only on the user's explicit request through UAC, to manage windows of apps the user runs as administrator.

**While elevated**, the app calls `ChangeWindowMessageFilterEx(app, TaskbarCreated, MSGFLT_ALLOW)` (and for the tray callback if spike item 7 shows it's filtered), so an Explorer restart doesn't leave it without a tray icon and an Exit.

### 6. Visible state

While elevated, the tray tooltip is "Screen Shifter (administrator)" and the tray menu shows **Always run as administrator** (but not **Run as administrator**).

## Risks / Trade-offs

- **Store certification may refuse `allowElevation`.** Then the Store build uses `--no-elevation` and the feature is for sideload and (later) winget builds.
- **Elevated means a bigger blast radius:** global hooks and window control at admin level. Mitigation: nothing new is added beyond running the same code elevated; elevation is always the user's choice.
- **The flash on relaunch.** Accepted; a seamless hand-off can come later.
- **Stranded admin windows after a crash** need the user's Yes to come back (§4).
- **Spike outcomes can change §1, §2 and §4**; the design is updated before implementation.
