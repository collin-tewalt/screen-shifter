## Context

The app is one process, single-instance by a `Local\screen-shifter` mutex created at startup (`main.rs`: a second copy that sees `ERROR_ALREADY_EXISTS` returns before touching anything). `win::hide`/`show` use `ShowWindowAsync`, which UIPI makes ineffective on windows of higher-integrity processes; the monitor-slots spec already says such a window "stays visible". The low-level hooks (`hooks.rs`) miss input while a higher-integrity window has focus (key-ups are already noted as missed there). Exit (`show_all` + `shutdown`) shows everything and keeps the state file, and the next start's `adopt_saved_layout` takes the saved windows back into their slots. Packaged state lives in the package's redirected local app data (`win::state_dir`), and package identity matters: `win::packaged()`, the startup task and the state directory all depend on it.

## Goals / Non-Goals

**Goals:** admin apps' windows behave like any other while the app runs elevated; elevation only ever by the user's request through UAC; the "never left hidden" rule kept, including after an elevated run dies.

**Non-Goals:** an elevated helper process (rejected: it would duplicate hooks, show/hide, placement and snapshots across two processes with a privileged channel to secure); `uiAccess` (not possible for MSIX: it requires Program Files or System32); elevating without a prompt (scheduled tasks); a seamless hand-off without the flash (later, if wanted); elevation as a different user (over-the-shoulder credentials: that copy has its own state folder; documented, not handled).

## Decisions

### 0. Spike first

Before anything else, a throwaway spike on the owner's machine, with test windows only of processes it launched itself (AGENTS.md):
1. With the app unelevated, do the hooks see Ctrl+Alt while an admin window has focus? (Expected no; confirms the motivation.)
2. How a packaged full-trust app relaunches itself elevated *with its package identity*: `ShellExecuteExW` `runas` on the installed exe path (identity likely lost), on an app execution alias (`uap5:AppExecutionAlias`), or on `shell:AppsFolder\<PFN>!ScreenShifter`; with and without `allowElevation`. The chosen route goes into §2.
3. What the startup task does with that exe at sign-in (it must still launch unelevated: the exe stays `asInvoker`).
4. From an unelevated process: can it tell that a window belongs to an elevated process (`OpenProcess` with limited query, `OpenProcessToken`/`TokenElevation`; access denied counted as elevated)?
5. `CreateMutexW` on a mutex created by an elevated copy, from an unelevated one: `ERROR_ALREADY_EXISTS` or `ERROR_ACCESS_DENIED`?

The spike's findings are written into this design before implementation.

### 1. Single instance across elevation

An object created by a high-integrity process gets a high mandatory label, so an unelevated `CreateMutexW` on it may fail with `ERROR_ACCESS_DENIED` instead of `ERROR_ALREADY_EXISTS`. Both mean "already running": the second copy exits without touching anything. (Confirmed by spike item 5.)

### 2. The relaunch (user-initiated)

Tray **Run as administrator** (shown only when not elevated and the package can elevate) calls `ShellExecuteExW` with the `runas` verb on the route from spike item 2, passing `--takeover <pid>`. UAC asks the user:
- **Declined or failed** (`ERROR_CANCELLED` or any error): nothing happens; the app keeps running unelevated.
- **Approved:** the unelevated copy runs its normal Exit path (`show_all`, then `shutdown`: every window shown, state file kept). The elevated copy, started with `--takeover <pid>`, first waits for that process to exit (`OpenProcess(SYNCHRONIZE)`, at most 10 s), then starts normally: takes the lock, adopts the saved layout (hiding the slots' windows again, now including admin ones). The flash of all windows is accepted for now (non-goal: seamless).
- If the elevated copy fails to start after the old one exited, every window is already shown: nothing is stranded. If it panics after adopting, its panic hook shows from the state file, and as an elevated process it can show admin windows too.

The `runas` call reintroduces `ShellExecuteExW`, which the certification kit flags in its optional "blocked executables" test (overall WARNING, not FAIL); accepted.

### 3. Always run as administrator

A tray check item **Always run as administrator**, recorded in `elevate.txt` in the state directory (`always=yes|no`; parse/format a pure tested function). When on and the app starts unelevated (including from the startup task at sign-in), it posts the relaunch to itself once startup has finished, through the same path and the same modal treatment as the one-time questions (`WM_ASK` flow, after them): UAC appears; declining keeps it unelevated for this session (no repeated prompts). Turning it on while unelevated also offers the relaunch immediately.

### 4. Stranded admin windows

An elevated run can hide admin windows that an unelevated run can't show. Every normal exit path shows them (the app is elevated at that point). After a crash, kill or power loss, though, the next start is usually unelevated. During `adopt_saved_layout`, saved entries whose process is elevated (spike item 4) while this process is not are collected; if any, after startup the app asks: "Some windows of apps running as administrator are hidden, and Screen Shifter needs to run as administrator to show them. Run as administrator now?" Yes → the relaunch (§2), whose elevated copy restores them through adoption; No → asked again at the next start, and they stay recorded in the state file (never dropped). Until then they can't be shown: this is the one case the rule can't cover without the user's approval, and it is documented.

### 5. Packaging and the build flag

`packaging/AppxManifest.xml` gains `{{ELEVATION_CAPABILITY}}`, which `tools/package.py` fills with `<rescap:Capability Name="allowElevation" />` unless `--no-elevation`. The exe's embedded manifest stays `asInvoker` (no `requestedExecutionLevel` change). At runtime the app decides whether elevation is possible: a development build (no package) always can (plain `runas` on its exe); a packaged build reads its installed `AppxManifest.xml` (next to the exe) for `allowElevation`; when it can't, neither tray item is shown and the stranded-windows question isn't asked. `packaging/store/certification.md` gains the justification for `allowElevation`: elevation only on the user's explicit request through UAC, to manage windows of apps the user runs as administrator.

### 6. Visible state

While elevated, the tray tooltip is "Screen Shifter (administrator)" and the tray menu shows **Always run as administrator** (but not **Run as administrator**).

## Risks / Trade-offs

- **Store certification may refuse `allowElevation`.** Then the Store build uses `--no-elevation` and the feature is for sideload and (later) winget builds.
- **Elevated means a bigger blast radius:** global hooks and window control at admin level. Mitigation: nothing new is added beyond running the same code elevated; elevation is always the user's choice.
- **The flash on relaunch.** Accepted; a seamless hand-off can come later.
- **Stranded admin windows after a crash** need the user's Yes to come back (§4).
- **Spike outcomes can change §1, §2 and §4**; the design is updated before implementation.
