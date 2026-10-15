# Working on screen-shifter

Notes for coding agents (and humans) picking this project up on any machine. What the app does and how to use it is in `README.md`; the requirements are the specs in `openspec/specs/`.

## How changes are made

1. **Explore** the idea first (`/openspec-explore`): options, trade-offs, decisions with the owner.
2. **Propose** an OpenSpec change (`openspec new change <name>`): proposal, design, spec deltas, tasks. Name changes `<topic>-v<N>` after the version they'll ship as.
3. **Review loop:** run read-only subagent reviews of the change against the actual code, strict about what counts as "major" (lost/stranded windows, wrong Win32/DirectComposition facts, input state holes, spec/design contradictions, missing tasks). Fix the findings, review again, and stop when a review reports no major issues. Then implement without asking again.
4. **Implement**, with `cargo test` and `cargo clippy --all-targets -- -D warnings` clean.
5. **The owner tests manually.** Polish found while testing goes into the still-open change: update its design/spec/tasks (a short review round if the addition isn't trivial), then implement. Intermediate "WIP" commits as rollback points are welcome while iterating.
6. **On sign-off:** bump `version` in `Cargo.toml`, commit, `openspec archive <change> -y`, commit, tag `v0.N`, push.

Commit messages end with a `Co-Authored-By:` line for the agent.

## The one rule

**Windows must never be left hidden while the app isn't running.** Every exit path shows them first (Exit, logoff, Windows closing the app for an update/uninstall, a close request, a panic, a failed start); the state file in `%LOCALAPPDATA%\screen-shifter\hidden.txt` (`screen-shifter-pkg` for the installed MSIX) is written *before* anything is hidden, and the next start takes saved windows back into their slots.

Practical consequences:
- **Never kill the running app** (`taskkill /f`, Task Manager): ask the owner to choose **Exit** from the tray. Only one copy runs at a time, and a running debug build locks `target\debug\screen-shifter.exe`, so builds fail with "Access is denied" until it's exited.
- **Tests and spikes only touch windows of processes they launched** (match by PID). Never `FindWindow` an app class the owner may have open (Windows 11 Notepad is single-process, for example).

## Build, test, package

- `cargo run` (debug builds keep a console with timing logs), `cargo test`, `cargo clippy --all-targets -- -D warnings`. In Git Bash, cargo may need `export PATH="$USERPROFILE/.cargo/bin:$PATH"`.
- Run Windows SDK tools (`makeappx`, `makepri`, `signtool`) from **PowerShell**: Git Bash rewrites `/flag` arguments into paths.
- Package: `python tools/package.py` (stdlib only; version from `Cargo.toml`, `a.b.c` → `a.b.c.0`). Re-sideloading a newer build of the same version needs `--revision N`.
- Icons: `python tools/make_icons.py` regenerates every PNG (outputs are committed).
- `SCREEN_SHIFTER_GDI=1` forces the non-GPU slide fallback.

## Sideloading on a new machine

The dev signing certificate is **per machine and never committed** (it holds a private key, and every machine that trusts it would install anything signed with it):

1. `powershell -ExecutionPolicy Bypass -File tools\dev_cert.ps1` makes `dev-cert.pfx`/`.cer` (git-ignored).
2. The owner runs `tools\trust_dev_cert.ps1` in an **admin** PowerShell (`-Remove` to undo). Agents don't do this step: it changes machine security settings.
3. `python tools/package.py --sign dev-cert.pfx --password dev`, then `Add-AppxPackage dist\screen-shifter-<version>-x64.msix`.

Store builds are signed by Microsoft; any CI signing would use a certificate from a secret store, never from the repo.

## Code map

- `src/core.rs`: pure slot logic and the state file format (most tests live here).
- `src/win.rs`: Win32 shell (windows, monitors, show/hide, focus, state file I/O).
- `src/main.rs`: startup, message loop, input handling, switches and moves.
- `src/hooks.rs`: low-level keyboard/mouse hooks on their own thread (atomics + posted messages only).
- `src/panel.rs`: the Ctrl+Alt panel and the window-drag ghost.
- `src/anim.rs`: slide timing, GDI fallback sheets. `src/gpu.rs`: Desktop Duplication + DirectComposition (sheets, shared device).
- `src/expose.rs`: the exposé (layout, transitions, preview drag). `src/snap.rs`: window snapshot worker. `src/icons.rs`: app icon worker (shell icons per app, cached).
