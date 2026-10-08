## 1. Package skeleton and spike

- [x] 1.1 `Cargo.toml` version 0.5.0; features `ApplicationModel`, `Win32_System_Recovery`, `Win32_Storage_Packaging_Appx` (`GetCurrentPackageFullName`); `.gitignore` `dist/`, `stage/`, `*.pfx`; state folder `screen-shifter-pkg` when packaged (design §8)
- [x] 1.2 `packaging/AppxManifest.xml` template (design §3), `packaging/priconfig.xml` (makepri createconfig), placeholder logos so packing validates; `tools/dev_cert.ps1` (design §4) for the user to run
- [x] 1.3 `tools/package.py` (design §4): version from Cargo, release build, stage, fill manifest, `makepri new`, `makeappx pack`, optional `--sign <pfx>`, `--publisher`/`--name` overrides, `--revision N` for the update test (`0.5.0.1`)
- [x] 1.4 `shutdown()` shared by tray Exit, `WM_CLOSE` (after `show_all`), and `WM_ENDSESSION(TRUE)`; `WM_ENDSESSION(FALSE)` keeps running; OSD/panel/sheet/ghost procs ignore `WM_CLOSE`; `RegisterApplicationRestart(None, RESTART_NO_CRASH | RESTART_NO_HANG | RESTART_NO_REBOOT)` at startup; a spike-only message log to a file in the state folder (packaged release has no console), removed after the spike
- [x] 1.5 Spike with the user (design §7): sideload 0.5.0.0, run > 60 s, hide a window in a slot, install 0.5.0.1 while running with default options and with `-ForceApplicationShutdown`, then uninstall while running (PowerShell and Settings); record what was received and whether windows reappeared and the app relaunched; write results into design §7 and update the spec deltas to match (the uninstall scenarios, the relaunch wording; README note if uninstall kills without asking)

## 2. Icon

- [x] 2.1 `tools/make_icons.py`: stdlib-only rect renderer (8× supersample, rounded corners, PNG via zlib/struct), full and simple drawings (design §1)
- [x] 2.2 Generate and commit `packaging/Assets/*` (every name the manifest references) and `assets/tray-{16,20,24,32}.png`; eyeball 16 px and 256 px with the user
- [x] 2.3 Tray icon from embedded PNG via `CreateIconFromResourceEx`, size from `GetSystemMetricsForDpi(SM_CXSMICON)`, cached per size; re-picked on `TaskbarCreated`, `WM_DISPLAYCHANGE`, `WM_SETTINGCHANGE`

## 3. Start with Windows

- [x] 3.1 Startup task in the manifest (design §3)
- [x] 3.2 Tray item states and actions (design §5): checked/unchecked/settings link/grayed, `RequestEnableAsync`, `Disable`, `ms-settings:startupapps`; "(installed app only)" when unpackaged

## 4. Verify

- [x] 4.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [x] 4.2 Manual on the sideloaded package: icon in tray (100% and 150%), Start menu, Settings; Start with Windows default on after first launch, sign out/in, toggle off/on from tray, off in Settings → tray links there; dev build shows the item disabled — signed off by user on the 0.5.0.2 sideloaded build (2026-10-08)
- [x] 4.3 Manual: all v0.4 scenarios still pass in the packaged app, starting from a machine with an existing dev `hidden.txt` (packaged state lives in `screen-shifter-pkg`); `WM_CLOSE` posted to the app window with windows hidden shows them and exits (`taskkill` without `/f` turned out to be a no-op for a windowless tray app) — signed off by user on the 0.5.0.2 sideloaded build (2026-10-08)
- [x] 4.4 README: install (sideload for now), Start with Windows, packaging command, uninstall note per spike
