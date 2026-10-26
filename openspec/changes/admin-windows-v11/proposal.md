## Why

Windows' UIPI keeps a normal process from controlling an elevated one, so Screen Shifter can't hide, show, move or snapshot the windows of apps running as administrator (an admin terminal, Task Manager, an IDE run as admin), and its keyboard and mouse hooks go blind while such a window has focus. Today those windows just stay on screen through every switch. Users who work with admin apps want them in slots too.

## What Changes

- **Run as administrator, only on the user's request.** The tray menu gains **Run as administrator**: Screen Shifter relaunches itself elevated after the user approves the UAC prompt. If they decline, nothing changes. It never elevates by itself.
- **Always run as administrator** (a tray option, remembered): every later start, including the start at sign-in, comes up normally and immediately asks UAC to relaunch elevated; declining keeps it running unelevated, managing normal windows as today.
- **One process.** The elevated copy is the whole app (one state, one set of hooks); an elevated process controls admin and normal windows alike. No elevated helper.
- **Hand-off:** the relaunch uses the existing Exit and adopt-on-start path: the unelevated copy shows everything and exits, the elevated copy takes the saved layout back (a brief flash, but no new way to strand windows).
- **Stranded admin windows.** If a start that isn't elevated finds windows of admin apps hidden by an earlier elevated run (after a crash or power loss), it asks to run as administrator to restore them, on every start until they are restored.
- **One copy at a time across elevation.** The single-instance check also holds when the running copy is elevated.
- **Packaging.** The package declares the `allowElevation` restricted capability (the exe still runs as invoker, so it never elevates on its own); `tools/package.py --no-elevation` builds without it, and the app hides both tray items when its package can't elevate. The Store build tries with the capability first; the flag is the fallback if certification refuses it.
- When elevated, the tray tooltip says "Screen Shifter (administrator)".
- **BREAKING:** none.

## Capabilities

### New Capabilities
- `elevation`: user-initiated relaunch as administrator, the "always" option, the hand-off, single instance across elevation, and stranded admin windows.

### Modified Capabilities
- `packaging`: the `allowElevation` capability and the build flag that leaves it out.

## Impact

- `src/main.rs`: tray items, the relaunch (`ShellExecuteExW` with `runas`, which brings back the certification kit's optional "blocked executables" warning), the takeover wait, the "always" record, the startup question for stranded admin windows, the single-instance check.
- `src/win.rs`: whether this process is elevated; whether a window belongs to an elevated process.
- `packaging/AppxManifest.xml`, `tools/package.py`: the capability (template), `--no-elevation`, possibly an app execution alias (see the spike).
- `packaging/store/certification.md`: the `allowElevation` justification.
- Docs: README (tray menu), PRIVACY.md (unchanged in substance; mentions elevation only on request), AGENTS.md.
