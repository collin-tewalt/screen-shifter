## Context

The app is a single 335 KB unpackaged exe with a tray icon (`LoadIconW(IDI_APPLICATION)`), a state file in `%LOCALAPPDATA%\screen-shifter\hidden.txt`, and one rule above all: windows are never left hidden while the app isn't running. Exit, logoff (`WM_QUERYENDSESSION`), panic, and failed starts all show everything; the next start takes the saved layout back. The Windows SDK 10.0.19041 (makeappx, signtool, makepri) and Python are on the dev machine.

## Goals / Non-Goals

**Goals:** a recognizable icon; a Store-ready MSIX built by one command; start with Windows; survive being closed by Windows for an update/uninstall without stranding windows; one version number.

**Non-Goals:** Store listing/submission, distribution signing, in-app updates, winget-pkgs manifest, exe icon resource.

## Decisions

### 1. Icon: generated, not drawn in a tool

The design is only rectangles (bezel, screen, stand, sheet, shadow strip, two window bars, speed lines), so `tools/make_icons.py` draws it directly: each shape is a rect with optional rounded corners, rendered at 8× and box-filtered down for antialiasing, written as PNG with `zlib` + `struct`. No Pillow, no SVG renderer, no new dependency. The script is the source of truth; outputs are committed so building never needs Python.

Two drawings:
- **Full** (≥ 32 px): bezel, stand, blue screen (`#0078D7`, the panel accent), light sheet breaking out past the left edge with a dark trailing shadow, two window bars, three speed lines.
- **Simple** (16–24 px): no stand, no speed lines, thicker sheet; the break-out silhouette is what must read at tray size.

Outputs:
- MSIX: `Square44x44Logo` at scale-100/200 and `targetsize-16/24/32/48/256` (+ `_altform-unplated`), `Square150x150Logo` at scale-100/200, `StoreLogo` (50). No wide logo or intermediate scales unless Store validation asks. Qualified names resolve only through `resources.pri` (§4).
- Tray: `assets/tray-16.png`, `-20`, `-24`, `-32` (simple ≤ 24, full at 32).

### 2. Tray icon from embedded PNG bytes

`CreateIconFromResourceEx` accepts PNG data directly. Pick the size from `GetSystemMetricsForDpi(SM_CXSMICON, dpi of the primary monitor)` (16/20/24/32 → nearest file at or above), `include_bytes!` the four PNGs. No resource compiler, no `build.rs`. Icons are created once per size and cached for the process (no `DestroyIcon` churn); the size is re-picked on `TaskbarCreated`, `WM_DISPLAYCHANGE`, and `WM_SETTINGCHANGE` (scaling changes don't always send `WM_DISPLAYCHANGE`) with `NIM_MODIFY`. (The unpackaged exe keeps the generic icon in Explorer; the packaged app's Start menu, taskbar, and Settings use the MSIX logos.)

### 3. MSIX manifest

`packaging/AppxManifest.xml` with placeholders `{{VERSION}}`, `{{PUBLISHER}}`, `{{NAME}}`:
- `Identity Name`/`Publisher` come from Partner Center at submission time (`store-release`); for sideload testing `package.py` defaults to `ScreenShifter.Dev` / `CN=ScreenShifterDev`.
- `TargetDeviceFamily Windows.Desktop MinVersion="10.0.19041.0"` (`WDA_EXCLUDEFROMCAPTURE` needs 2004).
- `Application Executable="screen-shifter.exe" EntryPoint="Windows.FullTrustApplication"`, `rescap:Capability runFullTrust`.
- `<Resources><Resource Language="en-us"/></Resources>`, matching `priconfig.xml`'s `/dq en-US`.
- `uap:VisualElements AppListEntry="default"` so it's in Start (that's how users launch it after turning startup off); `BackgroundColor="transparent"`.
- `<desktop:Extension Category="windows.startupTask" Executable="screen-shifter.exe" EntryPoint="Windows.FullTrustApplication"><desktop:StartupTask TaskId="ScreenShifter" Enabled="true" DisplayName="screen-shifter"/></desktop:Extension>`.

`Enabled="true"`: Windows enables the task the first time the app runs, so installing and launching once is enough; Settings → Apps → Startup shows it and the user can turn it off there or from the tray.

### 4. `tools/package.py`

```
version  = Cargo.toml [package] version  →  "a.b.c.0"
cargo build --release
stage/ = AppxManifest.xml (filled) + screen-shifter.exe + Assets/
makepri new /pr stage /cf packaging/priconfig.xml /mn stage/AppxManifest.xml /of stage/resources.pri   (MRT: scale/targetsize/unplated)
makeappx pack /d stage /p dist/screen-shifter-<version>-x64.msix /o
--sign <pfx>: signtool sign /fd SHA256 /f <pfx> /p <password>   (sideload testing only)
```

Stdlib only; finds `makeappx`/`makepri`/`signtool` under the newest `Windows Kits\10\bin\*\x64`. `packaging/priconfig.xml` is generated once (`makepri createconfig /cf … /dq en-US /pv 10.0.0`) and committed. `--publisher`/`--name` override the dev identity for the Store build; `--revision N` sets the fourth version part for the update test (0.5.0.1). `dist/` and `stage/` are git-ignored.

**Dev certificate** (`tools/dev_cert.ps1`, run once by the user): `New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=ScreenShifterDev" -CertStoreLocation Cert:\CurrentUser\My`, export to `dev-cert.pfx` (git-ignored); trusting it in Local Machine → Trusted People is an admin step the user does. The manifest Publisher must equal the certificate subject exactly.

### 5. Start with Windows

`windows::ApplicationModel::StartupTask::GetAsync("ScreenShifter")` → `.State()`:
- `Enabled` / `EnabledByPolicy` → checked.
- `Disabled` → unchecked; clicking calls `RequestEnableAsync()`.
- `DisabledByUser` (turned off in Settings) → unchecked; Windows won't let the app re-enable it, so the item says "Start with Windows (turn on in Settings)" and clicking opens `ms-settings:startupapps`.
- `DisabledByPolicy` → grayed.
- Checked + click → `Disable()`.

Calls are blocking `.join()` (windows 0.62's blocking wait) on the main thread; fine for desktop apps, which get no consent UI (would deadlock an STA if Windows ever showed one), only when the tray menu opens or the item is clicked (tens of ms). Unpackaged (`GetAsync` fails with "no package identity"): the item is grayed with "(installed app only)".

### 6. Being closed by Windows

By default, installing an update while the app runs is **deferred** until the app exits (or the next sign-in); nothing reaches the app. When the installer forces it (`-ForceApplicationShutdown`; whether the Store forces or waits for full-trust apps is unverified, so we test the forced case as the worst case) or for removal, running processes are stopped, expected via the Restart Manager: `WM_QUERYENDSESSION` with `ENDSESSION_CLOSEAPP`, then `WM_ENDSESSION`, then termination after a timeout. This is the expectation the spike (§7) verifies; the requirements are written so that a deferred or polite path keeps the rule, and the impolite path is documented.

```
WM_QUERYENDSESSION  → show_all() (existing: windows visible, layout kept)  → TRUE
WM_ENDSESSION(TRUE) → shutdown(): unhook, unregister hotkeys, remove tray, DestroyWindow
                      (the same teardown as Exit, minus show_all which already ran)
WM_ENDSESSION(FALSE)→ cancelled: keep running (state already reset to one slot)
```

`RegisterApplicationRestart(None, RESTART_NO_CRASH | RESTART_NO_HANG | RESTART_NO_REBOOT)` at startup: if the Restart Manager closed the app, it may relaunch it after the update (only after ≥ 60 s of running; whether deployment does this for packaged apps is unverified, the spike checks). Not restarted by registration after crashes (would loop on a startup crash) or reboots (the startup task covers sign-in). Nothing depends on the relaunch for safety: windows were shown before exit, and the next launch (manual or at sign-in) takes them back.

`WM_CLOSE` (sent by `taskkill` without `/f`, some installers, and Restart Manager fallbacks to top-level windows, hidden ones included) is handled exactly like tray Exit: `show_all()` then `shutdown()`. Today it falls to `DefWindowProc` → `DestroyWindow` → quit with windows still hidden.

Exit is refactored into `shutdown()` used by tray Exit, `WM_CLOSE`, and `WM_ENDSESSION(TRUE)`. The other top-level windows (OSD, panels, sheets, ghost) also receive `WM_CLOSE` from `taskkill`/Restart Manager; their procs ignore it so they aren't destroyed piecemeal while the app window shuts down. It calls `EndMenu()` first (a `WM_CLOSE` can arrive while the tray menu's modal loop runs). Nothing important may follow `DestroyWindow` in it: at real logoff the process can be killed right after the handler returns.

If Windows kills without asking (the spike checks): windows stay hidden until the app next runs (it adopts them from the file); after an uninstall it never runs again. Code can't fix that case, so the spike result decides the README guidance ("Exit before uninstalling").

### 7. Spike first (on a sideloaded package)

Before the rest: build the package with the manifest and `WM_CLOSE`/`WM_ENDSESSION`/restart handling, sideload 0.5.0.0, run it for over 60 s, hide a window in a slot, then:
(a) `Add-AppxPackage` 0.5.0.1 while running, default options (expected: deferred or "in use", nothing received);
(b) the same with `-ForceApplicationShutdown`;
(c) uninstall while running (`Remove-AppxPackage`, and once from Settings → Apps).
Log every message the app window receives with its lParam, and in any relaunched instance whether it has package identity (`GetCurrentPackageFullName`; a relaunch without identity would use the dev state folder) (to a file in the state folder, since a packaged release build has no console), whether windows reappear, and whether the app relaunches. Results go into this section.

Sideloading needs one of: Developer Mode (Settings → System → For developers) with `Add-AppxPackage -Register` of the staged folder, or a self-signed certificate trusted in Local Machine → Trusted People with the signed `.msix`. The update test needs the real `.msix` path (a registered folder doesn't go through the same update). Both are one-time user actions (they change machine security settings).

### 8. Packaged-app differences that need nothing

- `%LOCALAPPDATA%`: since 1903 only *new* files are redirected into the package's private folder; existing ones are modified in place, and reads check private then real. On a machine that has run a dev build, `screen-shifter\hidden.txt` exists, so writes would split across both. **Fix:** when the process has package identity (`GetCurrentPackageFullName` succeeds), the state folder is `screen-shifter-pkg`, which never pre-exists, so every file is new and private: separate from dev builds and removed on uninstall (after the windows were shown).
- The single-instance mutex is shared between packaged and dev builds (full-trust apps aren't in an AppContainer): only one runs at a time, as wanted.
- Hooks, `SetWindowDisplayAffinity`, hiding other apps' windows: unchanged for full-trust apps.

### 9. Version

`Cargo.toml` `version = "0.5.0"`; `package.py` derives `0.5.0.0`. Tags stay `vX.Y`. (The Store requires the fourth part to be 0 and each submission's version to be higher.)

## Risks / Trade-offs

- **Uninstall may kill without notice** → spike decides; worst case README guidance. Windows of other apps stay hidden until those apps restart; nothing is deleted.
- **Store certification of `runFullTrust` + input hooks** → justification text in `store-release`; many window managers ship this way.
- **Startup task `Enabled="true"` feels presumptuous** → it's what a tray utility is for; one click (or Settings) turns it off.
- **Old SDK (19041)** → sufficient for these manifest features; `package.py` picks the newest installed.
