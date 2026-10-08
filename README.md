# screen-shifter

Per-monitor "desktops" for Windows: each monitor gets its own row of slots of windows that you switch independently, like macOS's "Displays have separate Spaces".

## Use

Hold **`Ctrl+Alt`** to bring up a panel on every monitor showing what's in each slot. While holding it:

| Mouse | Action |
|---|---|
| Wheel up / tilt right | Next slot on the monitor under the mouse |
| Wheel down / tilt left | Previous slot on the monitor under the mouse |
| Click a tile | Jump straight to that slot |
| Click **+ New** | Add an empty slot at the end and switch to it |
| Press on a window and drag onto a tile | Move that window into that slot (either monitor) |
| Drag a window onto **+ New** | Put it in a new slot at the end |

Release `Ctrl+Alt` (or drop somewhere that isn't a tile) to cancel a drag. The keyboard still works too:

| Keys | Action |
|---|---|
| `Ctrl+Alt+→` | Next slot on the monitor under the mouse |
| `Ctrl+Alt+←` | Previous slot on the monitor under the mouse |

Each monitor starts with one slot; you get more with **+ New** (up to 8). A slot you leave empty disappears and the ones after it renumber. Scrolling stops at the first and last slot.

Without the panel up, a "2 / 3" indicator flashes on that monitor. Whatever is open on a monitor when you leave a slot belongs to that slot. Hidden windows disappear from the taskbar and Alt+Tab.

Switches animate: the old slot slides off to the left when you move toward the end and to the right when you move toward the start, and quick scrolling peels sheets off back to back. A dropped window's ghost flies into its tile (which pulses), glides to its new spot on the other monitor, or springs back if you cancel. Turn animations off with Windows' own **Settings → Accessibility → Visual effects → Animation effects**.

Tray icon (right-click): **Restore all windows**, **Exit**.

## Build

```bash
cargo build --release
```

The exe is `target\release\screen-shifter.exe`. Debug builds keep a console with logging; `screen-shifter --list` prints which windows are managed and why.

## Recovery

Hidden windows and the slot layout are written to `%LOCALAPPDATA%\screen-shifter\hidden.txt` *before* they are hidden. Windows are never left hidden while screen-shifter isn't running: they come back on Exit, logoff/shutdown, a panic, or a failed start.

The layout survives a restart: the next launch (after Exit, a crash, or a kill) takes each saved window back into its slot, as long as it's still open, still on the monitor it was saved on, and Windows hasn't restarted since. Anything else is shown. **Restore all** (and adding or removing a monitor) shows everything *and* forgets the layout.

Before running an older build (v0.3 or earlier), choose **Restore all**: older builds can't read this file and would discard it.

## Known limitations

- Windows of elevated (admin) apps can't be hidden; they stay visible on every slot.
- Some apps whose main window is owned by an invisible window (some Delphi/WinForms apps) aren't managed and stay visible.
- Slots ignore Windows' own virtual desktops: windows on other native desktops are left alone, but hidden slots are shared across native desktops.
- A monitor that drops out of the display list when it sleeps (common with DisplayPort) resets all slots.
- Hotkeys are fixed; if another app owns them, screen-shifter shows an error and exits.
- While `Ctrl+Alt` is held, left clicks and the wheel go to screen-shifter, not to apps.
- screen-shifter's overlays (panel, indicator, animations) don't appear in screenshots or screen sharing.
- Protected video may show as black in the sliding picture; with HDR on, the picture can look slightly dimmer than the live screen.
