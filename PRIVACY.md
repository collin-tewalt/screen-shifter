# Screen Shifter privacy policy

Screen Shifter does not collect, store or transmit any personal information. It makes no network connections of any kind: no telemetry, no analytics, no update checks, no accounts.

## What it reads, and why

- **Your windows and screen.** To switch a monitor's windows, Screen Shifter lists the windows on your screen (their handles, positions, owning process and window class) and hides or shows them. To draw its animations and the exposé previews, it reads the picture on your monitors (Windows Desktop Duplication) and asks windows to draw themselves. These pictures are processed in memory, on your graphics card, and are never written to disk or sent anywhere. They are discarded when the panel closes or the app exits.
- **Your keyboard, mouse and touchpad.** Screen Shifter watches for its own shortcuts (holding Ctrl+Alt, the wheel, clicks and drags while its panel is open, Esc) and, if you allow it, three-finger touchpad swipes. Nothing you type or do is recorded, and other keys pass through untouched.
- **App icons.** It asks Windows for the icons of the apps whose windows it shows, to display them on the previews. They are kept in memory only.

## What it keeps on your device

In the app's own local data folder:

- a small state file listing the windows it has hidden (window handle, process id, window class, and which slot), so that if the app or your PC stops unexpectedly, the next start can put them back. It is deleted by **Restore all windows** and is rewritten as you switch.
- your answers to its two one-time questions (whether a three-finger swipe may open the panel, and whether to start when you sign in), and, while the swipe is on, the previous value of the Windows three-finger setting so it can be put back.

None of this leaves your device. Uninstalling Screen Shifter removes it all.

## Changes it makes to Windows

Only with your consent, and undone by the app: turning off Windows' own three-finger slide while the swipe feature is on (restored when the app exits), and starting when you sign in (you can turn it off from the tray icon or Settings → Apps → Startup). It runs as administrator only when you choose it from the tray icon and approve Windows' UAC prompt, to manage windows of apps you run as administrator; nothing else changes when it does.

## Contact

Questions: open an issue at https://github.com/collin-tewalt/screen-shifter/issues.
