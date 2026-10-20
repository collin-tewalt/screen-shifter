# Store screenshots (the owner takes these)

1920×1080 or larger, PNG, up to 10; the first one is the main image. Nothing personal on screen (no email, chats, names, account pictures, file names): use sample documents and neutral apps. Screen Shifter hides its own overlays (panel, exposé, slides) from screen capture, so take them from a dev build with that turned off: Exit the installed app, then in PowerShell `$env:SCREEN_SHIFTER_SCREENSHOTS=1; cargo run --release`, and use the Snipping Tool with a delay (Win+Shift+S, or its timer) while holding Ctrl+Alt. In this mode the frosted backdrop may show a blurred copy of the panel; retake if it's distracting.

1. **The panel on both monitors:** Ctrl+Alt held, each monitor showing 2–3 slots with window titles and icons, the current slot highlighted. Caption: "Each monitor gets its own desktops."
2. **Exposé:** resting on a slot tile, its windows spread across the monitor over the frosted backdrop, app icons and × visible. Caption: "Rest on a slot to see its windows."
3. **Dragging a preview to another slot** (mid-drag, a tile highlighted). Caption: "Drag any window to any slot, on any monitor."
4. **The slide** (a frame mid-slide, if it can be caught). Caption: "Smooth switching, one monitor at a time."
5. **Tray menu.** Caption: "Restore everything at any time."
