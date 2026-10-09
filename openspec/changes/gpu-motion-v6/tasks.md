## 1. Spike (throwaway; only our own windows)

- [x] 1.1 Desktop Duplication of each monitor with our panel/OSD up: are `WDA_EXCLUDEFROMCAPTURE` windows absent? (design §9.1)
- [x] 1.2 Time `DuplicateOutput` + first `AcquireNextFrame` per switch vs. a kept-open duplication, both monitors; decide design §3
- [x] 1.0 Commit → `WaitForCommitCompletion` → `DwmFlush`, then change content underneath: any frame without the sheet? (§9.0)
- [x] 1.3 DComp overlay window (styles in §1, incl. `SetLayeredWindowAttributes`): click-through, above normal topmost windows, excluded from capture
- [x] 1.4 400 ms `AddCubic` slide while the main thread sleeps 100 ms mid-slide: smooth? (§9.4)
- [x] 1.5 `DWMWCP_ROUND` on the layered panel popup; record all results in design §9

## 2. GPU sheets (`src/gpu.rs`, `anim.rs`)

- [x] 2.1 `Cargo.toml` features; lazy GPU state: adapter matched to the monitors' outputs, D3D11 device (BGRA), DComp device, per-monitor overlay with `SetLayeredWindowAttributes(255)` (created on first use, hidden when idle, destroyed on monitor change)
- [x] 2.2 Capture per §3: per switch; output matched by device name; reject non-identity rotation and non-BGRA formats; `AcquireNextFrame` retried while `LastPresentTime == 0` within a 50 ms budget, then fail; `DwmFlush` first if this monitor's last hide/show (QPC) is later than `lastFrameTime`; any failure → `None` with a logged cause
- [x] 2.3 Sheet surface (`ALPHA_MODE_IGNORE`) filled via `BeginDraw` offset + `CopySubresourceRegion`; shadow child visual (premultiplied); client-pixel offsets; insert below running sheets; show overlay; Commit (`start_sheet` stops there; `run_switch` does `raise_overlays` → `WaitForCommitCompletion` → `DwmFlush`); one path per monitor while sheets run (GPU failure mid-chain → no new sheet)
- [x] 2.4 Pure helper `cubic_coeffs(from, to, secs) -> (c, l, q, k)` with a test that it matches `ease_out`; every animation uses `SetAbsoluteBeginTime(nextEstimatedFrameTime)` (if earlier than now + 2 ms, the next frame boundary after now + 2 ms) and `Motion` records that time converted via QPC to `now_ms`; pure helper for the frame's from/to (no shadow offset) tested against `sheet_layout`; chaining re-animates running sheets from their x at that time over `min(remaining, CHAIN_MS)`
- [x] 2.5 Removal of ended GPU sheets on the existing `ANIM_TIMER` tick (its kill check becomes `!anim::any()`); `clear_all` covers GPU sheets; `active()` counts only GDI sheets (frame loop), `any()` keeps the 1 s backstop timer alive for all sheets; device-removed drops GPU state
- [x] 2.6 `start_sheet` tries GPU then GDI; `run_switch` starts the GPU animation after the hide/show and before the panel/OSD update (sheet static until then); `SHEET_MS = 400`, `CHAIN_MS = 120`; update tests that assume old constants

## 3. Panel

- [x] 3.1 Icons per §7 (class icon, `WM_GETICON` with 50 ms timeout within a 100 ms budget per panel build, cache incl. misses per panel open); `PanelData` titles carry icons; draw beside titles
- [x] 3.2 Rounded corners per §8

## 4. Verify

- [x] 4.1 `cargo test`, `cargo clippy --all-targets -- -D warnings` clean
- [x] 4.2 Manual: slides on both monitors, both directions, fast flicks chain, wheel while panel up (panel not in the sheet), ~400 ms feels right; timing log shows sheet-up time and no per-frame work
- [x] 4.3 Manual: icons appear for common apps (signed off by user); forced GDI fallback and hung-app checks skipped by the user's call (2026-10-09)
- [x] 4.4 README animation section updated; packaged build to be re-sideloaded with the next `--revision`
