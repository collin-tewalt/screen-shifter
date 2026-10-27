# Certification (paste into Partner Center)

## Restricted capability justification (Submission options → runFullTrust)

Screen Shifter is a Win32 desktop tray application (packaged with the Desktop Bridge). It needs full trust to do its job: it shows and hides other applications' top-level windows (ShowWindow), moves them between monitors (SetWindowPlacement), watches its own keyboard and mouse shortcuts with low-level hooks, reads the screen with Desktop Duplication for its animations and previews, uses a startup task, and, only when the user asks to run as administrator, relaunches itself through UAC (ShellExecuteExW with "runas") to manage windows of elevated apps. None of this is possible from an app container.

## Notes for certification (Submission options → Notes for testers)

What it does: each monitor gets its own independent set of desktops ("slots"). Switching a slot hides the windows on that monitor and shows the slot's windows, leaving the other monitors as they are. Best tested with two monitors; everything works on one.

How to try it:
1. Launch Screen Shifter. It asks two one-time questions (a three-finger touchpad swipe, start when signing in); either answer is fine.
2. Hold Ctrl+Alt for half a second: a panel appears at the bottom of each monitor.
3. While holding Ctrl+Alt: scroll up to create and switch slots on the monitor under the mouse, click "+ New", click a slot tile to jump to it, rest the mouse on a tile to spread that slot's windows, drag a window or a preview onto a tile to move it. Release Ctrl+Alt to close the panel. Ctrl+Alt+← / → also switch.
4. Right-click the tray icon: Restore all windows, Start with Windows, Three-finger swipe, Run as administrator / Always run as administrator (shown to administrators), Exit.

On user input and system changes:
- The low-level keyboard hook (WH_KEYBOARD_LL) only tracks whether Ctrl and Alt are held (and Esc to close the panel). The low-level mouse hook (WH_MOUSE_LL) only acts while the panel is open (wheel, clicks and drags on the panel). Nothing is logged, stored or transmitted; all other input passes through untouched. The app makes no network connections.
- Other applications' windows are hidden and shown only when the user switches slots or moves a window. Every hidden window is shown again when the app exits, on sign-out, when Windows closes it for an update or uninstall, and after a crash (a state file written before anything is hidden lets the next start restore them).
- Windows' three-finger slide setting is changed only after the user answers Yes to an explicit question, and is restored when the app exits.
- Start at sign-in is off until the user answers Yes to an explicit question; it can be turned off from the tray menu or Settings → Apps → Startup.
- Run as administrator: the app relaunches itself elevated only when the user chooses "Run as administrator" (or turns on "Always run as administrator") in the tray menu and approves the UAC prompt, so it can manage windows of apps the user runs as administrator (Windows' UIPI blocks that otherwise). The exe runs as invoker; no restricted capability is used for this.
- The "×" on a window's preview sends that window a normal close request (as its own close button does); the app shows its usual "save changes?" prompt. Nothing is force-closed.
