## Context

Panel tiles are GDI, click-through, and driven entirely by the mouse hook: `panel::hit` maps a point to `(monitor, Target)`, `on_ldown`/`on_lup` store and confirm the pressed tile, and `jump` switches. Core state per monitor is `slots: Vec<Vec<Entry>>` plus `active`; the active slot's vec is always empty (its windows are on screen) and no inactive slot is ever empty (`prune`). The exposé previews are DirectComposition visuals made from snapshots taken on a worker thread (`snap.rs`), only for windows visible when the panel opened.

## Goals / Non-Goals

**Goals:** close a slot from its tile without ever stranding or closing a window; show a recognizable, sharp app icon on each preview.

**Non-Goals:** force-killing windows; a plate behind the icon (asked again before this change is finished); new icons on panel tiles (they keep their 16 px GDI icons).

## Decisions

### 1. Closing is a pure core operation returning a Plan

`State::close_slot(monitor, i, now_ms) -> Option<Closed>` where `Closed { plan: Plan, receiver: usize }`; None when the monitor is unknown (it must not create one), has one slot, or `i` is out of range. `receiver` is the neighbor's index after removal, for the pulse. The neighbor `n` is `i-1`, or `i+1` when `i == 0`.

| `i` | `n` | Effect | Plan |
|---|---|---|---|
| inactive | inactive | `slots[n].extend(slots[i])` (neighbor's windows first), remove `i` | empty |
| inactive | active | remove `i`; its windows are shown on screen | `show` = `slots[i]` |
| active | (becomes active) | `active = n`; `slots[n]` is taken and shown; remove `i`. The active slot's windows are not hidden | `show` = `slots[n]` |

Indices after `i` shift down and `active` is adjusted. The invariants hold afterwards: the active slot's vec is empty, no inactive slot is empty (merging only adds). Nothing is ever hidden, so there is no new way to strand a window; the folded windows only gain visibility.

**Race plan (`Monitor.last`).** `close_slot` takes no `visible` list, so it must not overwrite a still-fresh `last` (a switch under `RACE_WINDOW_MS` earlier keeps its `hide` list, which a following switch still needs). It follows `move_to_slot`: when `plan.show` is non-empty it extends the fresh `last`'s `show` (creating a `last` with an empty `hide` if there is none or it is stale); an empty plan leaves `last` untouched.

**Write-ahead.** As in `switch_with`: `old = hidden()` before, the windows `leaving` the hidden set afterwards, tagged with `tag_active`, written by `persist(&extra)` before `win::show`. A crash between persist and show is covered by adopt-on-startup, which shows any window tagged with the active slot. The hidden-into-hidden case shows nothing and needs no write-ahead.

Alternatives: always merging into the active slot (rejected: the user asked for left/right neighbor); a confirm prompt (rejected: nothing is destroyed).

### 2. Wiring in `main.rs` mirrors `switch_with`'s tail

`close_slot_ui(monitor, i)`: if a drag is in progress return; `stop_hover()`, clear `DROP_HOLD` (its slot index may now name another tile), reset the panel's x hover; collapse only this monitor's spread with `expose::collapse_monitor(&monitor, ..)` (its stored slot index and entries are stale; other monitors' swipe-mode spreads stay). Then under the STATE borrow run `close_slot`, copy out the plan, receiver and write-ahead tags; `persist(&extra)`, `win::show(&plan.show)`, update `VISIBLE` for the monitor by **appending `plan.show` to what is already there, de-duplicated by id** (a close hides nothing; `switch_with`'s replace would drop the windows already on screen from the active tile, `open_expose` and `drop_preview`), `persist(&[])`, `gpu::switched(&monitor)`. The `VISIBLE` merge uses what `win::show` returned (windows that are gone or reused are left out). Windows that just came on screen weren't in the panel-open snapshot batch, so `snap::start_batch` is called for them as a best effort (`win::show` is asynchronous, so some may not yet be visible and are skipped; they keep placeholder cards until the next panel opening). Focus is left alone (nothing was hidden, so the panel spec's "focus is where it was" holds). Then `panel::update` for the monitor and `panel::pulse(monitor, receiver)`. No slide sheet: nothing hides, so there is no screen to wipe. The panel stays up (in swipe mode too: closing is not a leave-the-mode click), so several slots can be closed in a row.

**Release after an x.** The panel shrinks and recenters after a close, so the cursor usually ends up over a different tile. In Ctrl+Alt mode the chord is still held, and the eventual release would run `release_on_tile` and switch to that unrelated tile. A panel-level flag `CLOSED_BY_X` is set by `close_slot_ui`; `release_on_tile` does nothing while it is set (and consumes it); the first mouse move that changes the cursor position clears it, so a deliberate release over another tile after moving still switches; it is cleared in `panel::show`, `hide` and `end_panel_mode`.

### 3. The x is a hit zone, drawn on the tile

The x is a circle in a tile's top-right corner, about 20 px at 100% scale, drawn when its monitor has two or more slots (never on "+ New"). Three panel queries:
- `hit_close(pt) -> Option<(String, usize)>`: the x zone; takes priority in `on_ldown`.
- `hit(pt)`: unchanged, the whole tile including the x. Used for hover-spread only (so the spread continues while the cursor is on an x).
- `tile_hit(pt)`: `hit` with the x zones removed (a point on an x counts as between tiles). Used to store and confirm a tile press, by `release_on_tile`, by window drags, preview drags, `drop_preview`, and the drop highlight (`set_drop`).

So the x is not a click-through to its tile (a tile press released on its x does nothing), not a release target, and not a drop target. A press on an x is stored (`PRESSED_CLOSE`) and acts only when the release is on the same x; `PRESSED_CLOSE` is cleared wherever `PRESSED` is (panel hide, swipe-mode exit, the `WM_DISPLAYCHANGE` panel rebuild). The panel's hit functions read thread-local state, so the x rects and both queries are built on pure geometry helpers that the tests call. The x highlights while the cursor is over it: panel state `(monitor, index)` set from `on_hover` before any early return (the `DROP_HOLD` and swipe branches), and reset in `panel::show`, `hide` and `update`.

### 4. Icons come from the app, not the window

`WM_GETICON` returns whatever 16 to 32 px icon the app set; Electron apps' real 256 px icon is in the exe. `icons.rs` resolves, per window on a worker thread (COM `CoInitializeEx` with `COINIT_APARTMENTTHREADED`, a message-less STA as the shell prefers; `CoUninitialize` on exit): the window's explicit AppUserModelID (`SHGetPropertyStoreForWindow`, `PKEY_AppUserModel_ID`; set by packaged-app frames and PWAs, which live under `ApplicationFrameHost`; it does not expose a process-level AUMID, so Electron and plain Win32 apps fall through) if it resolves under `shell:AppsFolder`, else the process image path (`QueryFullProcessImageNameW`, `PROCESS_QUERY_LIMITED_INFORMATION`; may fail for elevated windows, which then use the fallback). The key is that AUMID or path.

Size: the request is always for 256 px (one cache entry per key serves every monitor and DPI; the shell scales from the best resource), then the picture is box-filtered to the drawn size when its surface is made (the largest drawn size in the set, at most 256) so DComp's bilinear never downscales more than 2x. The image comes from `IShellItemImageFactory::GetImage` with `SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK`; the returned `HBITMAP` is read with `GetObject`/`GetDIBits` (its real size may exceed the request) and deleted. Whether its alpha is already premultiplied is checked empirically on a known icon during implementation before converting for `surface_from_pixels(.., true)`. The drawn size is `min(target, bitmap pixels)` for every icon, so nothing is ever drawn larger than its pixels, and the surface is close to the drawn size, so DComp's bilinear scaling doesn't alias.

Fallback: `win::icon_of` uses thread-local caches and the panel-build budget, so it can't be called on the worker. The UI thread resolves the window's `HICON` with `win::icon_of` at request time (after `icon_budget(false)`, so the panel's remaining budget applies and windows beyond it fall back to the class icon) (the panel's `titles()` carries no window ids and drops untitled windows, so the request is built from the entries of each slot instead); `(id, HICON)` is sent with the request, and the worker draws it into a 64 px DIB (`GetIconInfo` + `GetDIBits` with the mask applied, so legacy icons without alpha come out opaque rather than invisible; HICONs are process-global; failure is tolerated, the app may have destroyed it). Such a low-resolution icon is shown at its native size.

Worker discipline: a bounded request queue (a new panel opening replaces stale requests), `IsHungAppWindow` windows are skipped, and a blocked shell handler can stall the worker (there is no real timeout on `GetImage`); the UI never waits on it, and previews simply have no icon until it arrives.

Cache: key to 256 px picture on the worker for the session (capped at 64 entries; cleared when over). Each result carries `(window id, key, picture)` even on a cache hit, so late windows get theirs. Requests are made when the panel opens for every window in every slot, hidden ones included (an icon needs no visible window). Results are drained in the `WM_SNAPSHOT` handler, which gains an `expose::icons_arrived` call beside `snapshots_arrived`. The exposé keeps one GPU surface per key, shared by all previews of that app; `expose::clear_all` removes sets only, so this map is capped at 64 keys (cleared with the GPU on reset or when over the cap; sets hold their own COM references, so clearing never pulls a surface from an open spread).

### 5. Placement and motion

All icons of one set live in a single container visual stacked above all previews and below nothing else of the set's (via `stack()`); this avoids per-preview stacking, which `drag_end(Land::Back)` (re-adds a preview in front of its siblings) and `restack` (re-inserts previews on fold and choose) would break. Each icon is placed at its preview cell's bottom-left, inset by 8 px, sized 22% of the cell's shorter side clamped to 32 to 72 px (times the monitor scale) and omitted when the shorter side is under 120 px. It is a sibling of the preview, not a child, so it keeps its pixel size while the preview scales. The pure size/inset/omit function is tested.

Motion: the container's opacity animates. Fade-in is a composition animation starting at the open animation's end (`begin + OPEN_MS`, using `begin_time(dcomp, delay_ms)` from `gpu.rs`, which `Tx::new` (delay always 0) is extended to take; the open is a fire-and-forget `Tx` and `Set` has no settle time, so the start time is recorded in a new `Set` field when the spread opens); an icon that arrives after that fades in individually at once (its own opacity group). The whole container fades out when the spread collapses (before the previews fold from `cell` to `home`, so icons never trail their previews), a preview is chosen, or a preview drag starts (only that preview's icon: each icon has its own opacity group inside the container); that icon fades back in on `Land::Back`, and is removed when its preview is `gone`. `drag_end(Land::Back)` re-adds the preview in front of everything, which would cover the container, so it re-raises the container afterwards (`RemoveVisual`, then `AddVisual(.., false, None)`); `restack` inserts relative to the highlight and leaves the container above the previews. Previews are not re-laid out when one leaves the set (the grid is not recomputed), so icons never need repositioning. Motion off: icons appear and disappear without fades (`Tx` duration 0).

### 6. An x on each preview asks the app to close the window

Added after the owner's first test: previews carry an x badge at their top-right, always visible (like the panel tile's), asking the app to close that window. It is a normal close request, never a kill: `PostMessageW(WM_SYSCOMMAND, SC_CLOSE)` to the preview's root window (the same request as its own close button and Alt+F4; non-blocking, so a hung app can't stall us; the app closes its owned dialogs itself, and an unsaved-changes prompt appears like any new window, on screen). The badge is two small premultiplied BGRA pictures (normal, and a hot red one while the cursor is on it), drawn procedurally with anti-aliasing (a pure, tested function) at 24 px × the monitor's scale, inset 8 px × scale from the cell's top-right, in the same container as the icons and with the same fade. A preview too small for an icon (under 120 px) still gets an x, since closing is useful there.

**Input.** `expose::close_at(pt) -> Option<u64>` returns the preview whose badge is under `pt`, ahead of `preview_at` in `on_ldown`. A press on a badge is stored (`PRESSED_CLOSE_WINDOW`) and acts only when the release is on the same badge; it never starts a preview drag, and it is cleared where `PRESSED_PREVIEW` is. While the cursor is on a badge, `on_hover` sets the spread's hover to None (the badge is hot, the preview is not highlighted), so releasing `Ctrl+Alt` over a badge only collapses the spread and never chooses the window.

**Pending, gone, kept.** `request_close(id)` sends the request, dims that preview (opacity 0.4) and records `(id, monitor, slot, current, started)` in a pending list polled by a 250 ms timer, which stops when the list is empty. Each poll: the window is gone if `!IsWindow(id)`, or, for a window of the current slot, if it is no longer visible and not minimized (an app that closes to its tray). Gone: the preview fades out and is marked `gone` (the grid isn't re-laid out, as for a dropped preview); `State::forget(monitor, slot, ids)` removes the root and any of its entries that no longer exist from a hidden slot (an inactive slot that empties goes, as always; a current slot keeps no entries); `VISIBLE` drops them; `persist(&[])` (removing a window needs no write-ahead: it can't be stranded); the panel updates, and a spread left with no live previews collapses. Kept: after 3 s with the window still there the preview returns to full opacity and the entry is dropped from the list; the window and its slot are untouched. A second click on a pending preview's x is ignored.

**Why no stale-index risk.** `forget` only ever empties the spread's own slot (the only one it removes entries from), so the one renumbering it can cause is that slot's own removal, which collapses that spread; other monitors' spreads (swipe mode) name slots of other monitors and are unaffected. A hidden slot's window can only be closed from its own spread.

**Safety.** Nothing is hidden, so "windows never left hidden" is untouched. A close request to a hidden window may make its app show a dialog or a "closing" window: new windows are not hidden by us, so they appear on screen and join the current slot by the normal capture rule. Closing is only ever requested by a click on an x the owner can see.

## Risks / Trade-offs

- **Dark icons on dark windows** (Cursor): accepted by the owner for now; a plate is the known remedy. Tasks end with asking the owner about it before this change is archived.
- **Wrong or generic icon** for hosts like `explorer.exe` or `ApplicationFrameHost.exe`: the AUMID path covers packaged apps and PWAs; the rest fall back to the per-window `WM_GETICON` icon.
- **COM on a worker thread**: STA, no apartment-bound objects cross threads (only pixel `Vec<u32>`).
- **Accidental x clicks**: nothing is destroyed and the windows stay reachable (drag them back out), so no confirmation. The panel resizes and recenters after a close, so the next x moves under the cursor; accepted.
- **Folding many windows into the current slot** can put a lot on screen at once; this is the requested behavior.
