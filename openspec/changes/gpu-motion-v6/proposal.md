## Why

The sheet slide works but isn't smooth: each frame redraws a full-monitor bitmap on the CPU with `UpdateLayeredWindow` from our own thread, so it runs ~85 fps with 20–40 ms hitches, and the BitBlt capture costs 30–43 ms before anything moves. The user also wants the motion slower so it's noticeable, and the panel to show app icons. The upcoming exposé (`slot-expose-v7`) needs a GPU overlay layer, which this change builds.

## What Changes

- **GPU sheets:** capture the monitor with DXGI Desktop Duplication straight into a GPU texture, show it through DirectComposition on a per-monitor click-through overlay, and let DWM run the slide as a composition animation (the same ease-out curve, as a cubic segment). Our thread does no per-frame work, so the motion can't hitch on our work.
- **Slower:** the slide takes about 400 ms (was 300); a chained sheet finishes within ~120 ms (was 80).
- **Fallback:** the existing GDI path stays for when the GPU path isn't available (no duplication on that output, e.g. HDR or Remote Desktop, a different GPU, device lost).
- **Icons in tiles:** each title in a panel tile gets its app's small icon.
- **Panel corners:** rounded window corners on Windows 11 (one DWM attribute; no-op elsewhere).
- Non-goals: the exposé (`slot-expose-v7`), moving the drag ghost / tile pulse to the GPU (small, smooth enough), HDR-correct capture.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `slot-motion`: slide duration ~400 ms; motion runs on the compositor (smooth regardless of the app's own work) with a fallback.
- `slot-panel`: tiles show each window's app icon beside its title.

## Impact

- New `src/gpu.rs`: D3D11 device, Desktop Duplication per output, DirectComposition device, per-monitor overlay windows, sheet visuals and animations.
- `anim.rs`: `start_sheet` tries the GPU path first; constants; GDI path kept as fallback. `main.rs` frame loop no longer needed for GPU sheets (kept for GDI sheets, ghost, pulse).
- `panel.rs`: icons (`WM_GETICON` with timeout, class icon fallback), rounded corners.
- `Cargo.toml`: `windows` features `Win32_Graphics_Direct3D`, `Win32_Graphics_Direct3D11`, `Win32_Graphics_Dxgi`, `Win32_Graphics_Dxgi_Common`, `Win32_Graphics_DirectComposition`. No new crates.
