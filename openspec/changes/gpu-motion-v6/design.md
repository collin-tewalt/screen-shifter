## Context

`anim.rs` today: `start_sheet` BitBlts the monitor into a 32-bpp DIB (+ shadow strip), creates a layered topmost click-through window, and `step` (called from the main loop's frame mode, paced by `DwmFlush`) redraws it with `UpdateLayeredWindow` at an eased x, clipped to the monitor. Chaining inserts the new sheet below and "hurries" running ones (`Motion::hurried`, ≤ 80 ms). Measured: capture 30–43 ms, ~85 fps, worst frame gaps 20–40 ms. Overlays use `WDA_EXCLUDEFROMCAPTURE`. `run_switch`: sheet up → `DwmFlush` → hide/show → the sheet starts moving on its first frame.

## Goals / Non-Goals

**Goals:** compositor-driven slide with no per-frame CPU work; faster capture; 400 ms; icons in tiles; a reusable GPU overlay for v7.

**Non-Goals:** exposé, GPU ghost/pulse, HDR capture, multi-GPU capture (falls back).

## Decisions

### 1. Pipeline

```
 once:   find the adapter whose outputs include the monitors (EnumAdapters → EnumOutputs,
         matched by DXGI_OUTPUT_DESC.DeviceName); D3D11 device on it (BGRA support)
         → IDXGIDevice → DCompositionCreateDevice. A monitor on another adapter → GDI.
 per monitor (lazy): overlay window  WS_EX_NOREDIRECTIONBITMAP | LAYERED | TRANSPARENT |
                     TOPMOST | TOOLWINDOW | NOACTIVATE, exactly the monitor rect,
                     SetLayeredWindowAttributes(255, LWA_ALPHA) (a layered window shows
                     nothing without it), WDA_EXCLUDEFROMCAPTURE; DComp target + root
                     visual. Hidden when idle.
 per sheet:
   capture  IDXGIOutput1::DuplicateOutput → check DXGI_OUTDUPL_DESC: Rotation IDENTITY and
            ModeDesc.Format B8G8R8A8 (else GDI: portrait monitors, HDR/FP16)
            → AcquireNextFrame, retried while LastPresentTime == 0 (pointer-only / empty
            first frame on some drivers) within a 50 ms total budget; out of budget → GDI → copy into the sheet surface (below) → ReleaseFrame
            → release the duplication
   visual   IDCompositionSurface (monitor size, BGRA, DXGI_ALPHA_MODE_IGNORE: duplicated
            alpha isn't guaranteed opaque) filled by BeginDraw(None) → (texture, offset)
            → CopySubresourceRegion(texture, 0, offset.x, offset.y, 0, frame, 0, None)
            → EndDraw. BeginDraw may hand back an atlas at a non-zero offset, so never
            CopyResource. Shadow: a child visual with a small premultiplied gradient
            surface (made once per height) at the trailing edge (child offset, not a
            wider bitmap as in GDI).
   show     root.AddVisual(sheet, below any running sheet on that monitor) → show the
            overlay window → Commit                     (start_sheet stops here)
            run_switch: raise_overlays() (panel/OSD/ghost above the overlay) →
            WaitForCommitCompletion → DwmFlush           (one frame of latency, as today)
            (Commit is asynchronous: without the wait, windows could change underneath
            for a frame before the sheet is composed; raising the overlays before the
            flush means no frame is ever composed with the sheet above the panel)
   switch   hide/show windows underneath (unchanged)
   go       OffsetX animation (§2) → Commit, BEFORE the panel/OSD update
   done     the existing 250 ms ANIM_TIMER tick removes sheets past their end (the
            animation's End parks them off-monitor, so late is harmless): RemoveVisual
            → Commit; hide the overlay when its root is empty. No new timers.
```

Visual offsets are in the overlay's client pixels: x = screen x − monitor left. The overlay is monitor-sized, so the sheet can never spill onto the other monitor (no manual clipping).

**One path per monitor at a time:** a GPU sheet created while a GDI sheet is still running on that monitor would sit above it (both topmost; the overlay is newer), so while any GDI sheet runs on a monitor, new sheets there use GDI too. The other way round, if a GPU capture fails while GPU sheets are running there, that switch gets no new sheet (the running ones still hurry off), rather than a GDI window covering them.

### 2. The curve as a composition animation

Ease-out cubic `1-(1-u)^3 = 3u - 3u² + u³` is a cubic polynomial, which is exactly what `IDCompositionAnimation::AddCubic(begin, constant, linear, quadratic, cubic)` evaluates. For x from `a` to `b` over `T` seconds (`d = b - a`): `AddCubic(0, a, 3d/T, -3d/T², d/T³)`, `End(T, b)`.

**Explicit start time:** an animation otherwise starts at the first composed frame after its commit, which we can't observe; early in the ease-out the sheet moves ~3d/T ≈ 19 000 px/s, so a one-frame guess error is a ~300 px jump when re-animating. So every animation gets `SetAbsoluteBeginTime(t)` with `t = GetFrameStatistics().nextEstimatedFrameTime` (QPC ticks, the unit `SetAbsoluteBeginTime` takes). If it's earlier than now + 2 ms (DWM idle makes it stale, possibly several periods in the past), use the first frame boundary after now + 2 ms: `lastFrameTime + k·period`, so the commit can't land after it and the slide never starts partway. Our `Motion` (on `win::now_ms()`) records the same instant converted as `now_ms() + (t − QueryPerformanceCounter()) · 1000 / timeFrequency`. Then "current x" for chaining is exact.

The GPU visual's offset is the **frame's** left edge (shadow is a separate child visual), unlike the GDI bitmap whose left edge includes the shadow for `Slide::Right`; a pure helper gives the frame's from/to per direction, tested against `sheet_layout`.

**Chaining:** a new sheet goes below the running ones; each running one gets a fresh animation from its x at the next frame time to its target over `min(remaining, CHAIN_MS)`, with the same absolute begin time. Same rule as today, computed by the existing pure helpers.

### 3. Capture: open per switch

Open the duplication per switch and release it right after the copy: no slot held between switches (slots are shared with OBS/Teams), no cached texture, no `ACCESS_LOST` bookkeeping. The spike measures `DuplicateOutput` + first `AcquireNextFrame`; only if that's well over ~15 ms do we revisit (a kept-open variant would need a cached texture and must not trust a timeout right after a switch).

**Chained captures must be current.** Duplication returns the last *presented* desktop; right after a switch on that monitor the previous hide/show may not be composed yet, so the capture could show the pre-switch image. Rule: record the QPC time of each monitor's last hide/show; if it's later than `GetFrameStatistics().lastFrameTime`, `DwmFlush` once before capturing (replaces today's `fresh` flag, which was cleared by the frame loop that GPU sheets no longer run). Cost: ≤ one frame, only on rapid repeats.

### 4. Fallback to GDI

`start_sheet` tries the GPU path; any failure (no device, output on another adapter, rotated output or non-BGRA format, `DuplicateOutput` unsupported e.g. RDP, access denied, acquire timeout or empty frame, device removed) is logged and that sheet uses the existing GDI path. A device-removed error drops the GPU state; the next switch rebuilds it. `clear_all` clears both kinds.

### 5. Loop and limits

GPU sheets need no frame stepping: `anim::active()` (which drives the main loop's frame mode) reports only GDI sheets, so the loop idles in `GetMessage` during a GPU slide. Removal rides the existing `ANIM_TIMER` (250 ms), which also enforces the 1 s hard limit; its kill check (`main.rs` `if !anim::active()`) becomes `if !anim::any()` so it keeps running while GPU sheets exist. Modal loops (tray menu) no longer freeze GPU sheets (DWM runs them), but `clear_all` stays around `TrackPopupMenu` for the GDI fallback.

### 6. Timing constants

`SHEET_MS = 400`, `CHAIN_MS = 120`, `HARD_LIMIT_MS = 1000` unchanged (400 + chain still well within).

### 7. Icons in tiles

Per window: class icon first (`GetClassLongPtrW(GCLP_HICONSM)`, `GCLP_HICON`: free, never blocks), then only if none, `SendMessageTimeoutW(WM_GETICON, ICON_SMALL2 / ICON_SMALL / ICON_BIG)` with `SMTO_ABORTIFHUNG | SMTO_BLOCK`, 50 ms. Many apps set their icon only via `WM_SETICON`, so try `WM_GETICON` first when budget allows: a **100 ms total budget per panel build**; once spent, class icons only. Results (including "none") are cached by HWND for the panel's lifetime, so a slow app costs at most once per panel open. Shared handles: never destroyed. Drawn with `DrawIconEx` at the panel's small-icon size beside each title. Hidden windows still answer `WM_GETICON`. (Store apps hosted by ApplicationFrameHost may show the host's generic icon; accepted.)

`PanelData.titles` becomes a list of `(icon, title)`; `titles()` in main fetches both.

### 8. Rounded panel

`DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)` on each panel window; ignored where unsupported. Spike checks it applies to our layered popup.

### 9. Spike first (throwaway, only our own windows)

0. With `WaitForCommitCompletion` + `DwmFlush` before a change underneath, is there ever a frame of the new content without the sheet over it?
1. Does Desktop Duplication honor `WDA_EXCLUDEFROMCAPTURE` (panel/OSD absent from the captured frame)?
2. Cost: `DuplicateOutput` + first `AcquireNextFrame` per switch vs. kept open (both monitors).
3. A DComp overlay with the window styles above is click-through and stays above normal topmost windows like today's sheets.
4. An `AddCubic` animation of 400 ms: frame pacing while the main thread deliberately sleeps 100 ms (proves independence).
5. Rounded corners on the layered panel popup.

Results recorded here; failures change §3/§4/§8, not the requirements.

## Risks / Trade-offs

- **Duplication unavailable** (HDR, RDP, some GPUs, protected content): fallback keeps today's behavior.
- **Protected video** appears black in the duplicated frame (same as GDI today).
- **Rotated monitors and HDR**: detected from the duplication description; GDI fallback (HDR desktops may duplicate as washed-out SDR rather than fail; the format check catches FP16).
- **Two code paths** for sheets: the GDI one already exists and is tested; the shared pure helpers (`Motion`, `sheet_layout`) drive both.
