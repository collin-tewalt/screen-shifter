# screen-shifter

Per-monitor "desktops" for Windows: each monitor gets 4 slots of windows that you switch independently, like macOS's "Displays have separate Spaces".

## Use

Hold **`Ctrl+Alt`** to bring up a panel on every monitor showing what's in each slot. While holding it:

| Mouse | Action |
|---|---|
| Wheel up / tilt right | Next slot on the monitor under the mouse |
| Wheel down / tilt left | Previous slot on the monitor under the mouse |
| Click a tile | Jump straight to that slot |
| Press on a window and drag onto a tile | Move that window into that slot (either monitor) |

Release `Ctrl+Alt` (or drop somewhere that isn't a tile) to cancel a drag. The keyboard still works too:

| Keys | Action |
|---|---|
| `Ctrl+Alt+→` | Next slot on the monitor under the mouse |
| `Ctrl+Alt+←` | Previous slot on the monitor under the mouse |

Without the panel up, a "2 / 4" indicator flashes on that monitor. Whatever is open on a monitor when you leave a slot belongs to that slot. Hidden windows disappear from the taskbar and Alt+Tab.

Tray icon (right-click): **Restore all windows**, **Exit**.

## Build

```bash
cargo build --release
```

The exe is `target\release\screen-shifter.exe`. Debug builds keep a console with logging; `screen-shifter --list` prints which windows are managed and why.

## Recovery

Hidden windows are written to `%LOCALAPPDATA%\screen-shifter\hidden.txt` *before* they are hidden. They come back on Exit, Restore all, logoff/shutdown, a panic, when a monitor is added or removed, and on the next launch after a crash or kill.

## Known limitations

- Windows of elevated (admin) apps can't be hidden; they stay visible on every slot.
- Some apps whose main window is owned by an invisible window (some Delphi/WinForms apps) aren't managed and stay visible.
- Slots ignore Windows' own virtual desktops: windows on other native desktops are left alone, but hidden slots are shared across native desktops.
- A monitor that drops out of the display list when it sleeps (common with DisplayPort) resets all slots.
- Hotkeys are fixed; if another app owns them, screen-shifter shows an error and exits.
- While `Ctrl+Alt` is held, left clicks and the wheel go to screen-shifter, not to apps.
