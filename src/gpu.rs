//! GPU sheets: Desktop Duplication captures the monitor straight into a GPU surface,
//! DirectComposition shows it on a per-monitor overlay, and DWM runs the slide itself, so
//! nothing on our thread can make it stutter. Every failure returns `None`/`false` and the
//! caller falls back to the GDI sheet in `anim.rs`.

use crate::anim::{CHAIN_MS, HARD_LIMIT_MS, Motion, SETTLE_MS, Slide, SHEET_MS, cubic_coeffs};
use std::cell::RefCell;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::QueryPerformanceCounter;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, w};

/// Total time allowed to get a usable frame from the duplication.
const ACQUIRE_BUDGET_MS: u64 = 50;

struct Overlay {
    monitor: String,
    hwnd: HWND,
    _target: IDCompositionTarget,
    root: IDCompositionVisual,
    /// QPC time of this monitor's last hide/show: a capture must not predate its composition.
    last_switch: i64,
}

struct Sheet {
    monitor: String,
    visual: IDCompositionVisual,
    /// Frame's left edge in overlay pixels; `None` until `go` starts it.
    motion: Option<Motion>,
    target: f32,
    created: u64,
}

struct Gpu {
    adapter: IDXGIAdapter1,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    dcomp: IDCompositionDevice,
    overlays: Vec<Overlay>,
    sheets: Vec<Sheet>,
}

thread_local! {
    static GPU: RefCell<Option<Gpu>> = const { RefCell::new(None) };
}

fn log_err(what: &str, e: impl std::fmt::Display) {
    crate::log!("gpu: {what}: {e}");
}

fn qpc() -> i64 {
    let mut t = 0;
    unsafe {
        let _ = QueryPerformanceCounter(&mut t);
    }
    t
}

/// The adapter output whose GDI device name is `monitor`.
fn find_output(factory: &IDXGIFactory1, monitor: &str) -> Option<(IDXGIAdapter1, IDXGIOutput1)> {
    unsafe {
        for a in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(a) else { break };
            for o in 0.. {
                let Ok(output) = adapter.EnumOutputs(o) else { break };
                let Ok(desc) = output.GetDesc() else { continue };
                let n = desc.DeviceName.iter().position(|&c| c == 0).unwrap_or(desc.DeviceName.len());
                if String::from_utf16_lossy(&desc.DeviceName[..n]) == monitor {
                    return Some((adapter, output.cast().ok()?));
                }
            }
        }
    }
    None
}

fn init(adapter: IDXGIAdapter1) -> windows::core::Result<Gpu> {
    unsafe {
        let (mut device, mut context) = (None, None);
        D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context))?;
        let device = device.expect("D3D11CreateDevice succeeded");
        let dxgi: IDXGIDevice = device.cast()?;
        let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
        Ok(Gpu { adapter, device, context: context.expect("context"), dcomp, overlays: vec![], sheets: vec![] })
    }
}

unsafe extern "system" fn overlay_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        return LRESULT(0); // only the app window decides when we close
    }
    unsafe { DefWindowProcW(h, msg, wp, lp) }
}

fn make_overlay(dcomp: &IDCompositionDevice, monitor: &str, rect: RECT) -> windows::core::Result<Overlay> {
    unsafe {
        let inst = GetModuleHandleW(None)?.into();
        let cls = w!("ScreenShifterGpuOverlay");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(overlay_proc), hInstance: inst, lpszClassName: cls, ..Default::default() });
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            cls, cls, WS_POPUP, rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top,
            None, None, Some(inst), None,
        )?;
        // A layered window shows nothing until its attributes are set.
        SetLayeredWindowAttributes(hwnd, Default::default(), 255, LWA_ALPHA)?;
        crate::anim::exclude_from_capture(hwnd);
        let target = dcomp.CreateTargetForHwnd(hwnd, true)?;
        let root = dcomp.CreateVisual()?;
        target.SetRoot(&root)?;
        Ok(Overlay { monitor: monitor.to_string(), hwnd, _target: target, root, last_switch: 0 })
    }
}

/// Copies the monitor's current desktop image into a new composition surface.
fn capture(g: &Gpu, output: &IDXGIOutput1, w: i32, h: i32) -> windows::core::Result<Option<IDCompositionSurface>> {
    unsafe {
        let dup = output.DuplicateOutput(&g.device)?;
        let desc = dup.GetDesc();
        if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY || desc.ModeDesc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
            || desc.ModeDesc.Width as i32 != w || desc.ModeDesc.Height as i32 != h
        {
            crate::log!("gpu: unsupported output (rotation {:?}, format {:?}, {}x{})", desc.Rotation, desc.ModeDesc.Format,
                desc.ModeDesc.Width, desc.ModeDesc.Height);
            return Ok(None);
        }
        let start = crate::win::now_ms();
        let frame = loop {
            let left = ACQUIRE_BUDGET_MS.saturating_sub(crate::win::now_ms() - start);
            let (mut info, mut res) = (DXGI_OUTDUPL_FRAME_INFO::default(), None);
            if dup.AcquireNextFrame(left as u32, &mut info, &mut res).is_err() {
                crate::log!("gpu: no desktop frame within {ACQUIRE_BUDGET_MS} ms");
                return Ok(None);
            }
            if info.LastPresentTime != 0 {
                break res.expect("acquired").cast::<ID3D11Texture2D>()?;
            }
            dup.ReleaseFrame()?; // pointer-only update: no image yet
        };
        let surface = g.dcomp.CreateSurface(w as u32, h as u32, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_IGNORE)?;
        let mut off = POINT::default();
        // BeginDraw may hand out an atlas at an offset: copy there, never CopyResource.
        let dst: ID3D11Texture2D = surface.BeginDraw(None, &mut off)?;
        g.context.CopySubresourceRegion(&dst, 0, off.x as u32, off.y as u32, 0, &frame, 0, None);
        surface.EndDraw()?;
        dup.ReleaseFrame()?;
        Ok(Some(surface))
    }
}

/// The soft shadow on the sheet's trailing edge: `sw` px of premultiplied black, densest
/// at the sheet (`at_left` = the sheet is on the left of the strip).
fn shadow_surface(g: &Gpu, sw: i32, h: i32, at_left: bool) -> windows::core::Result<IDCompositionSurface> {
    let row: Vec<u32> = (0..sw)
        .map(|i| {
            let d = if at_left { i } else { sw - 1 - i };
            let k = 1.0 - d as f32 / sw as f32;
            ((crate::anim::SHADOW_ALPHA * k * k * 255.0) as u32) << 24
        })
        .collect();
    let pixels: Vec<u32> = (0..h).flat_map(|_| row.iter().copied()).collect();
    unsafe {
        let surface = g.dcomp.CreateSurface(sw as u32, h as u32, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
        let mut off = POINT::default();
        let dst: ID3D11Texture2D = surface.BeginDraw(None, &mut off)?;
        let bx = D3D11_BOX { left: off.x as u32, top: off.y as u32, front: 0, right: (off.x + sw) as u32, bottom: (off.y + h) as u32, back: 1 };
        g.context.UpdateSubresource(&dst, 0, Some(&bx), pixels.as_ptr() as *const _, (sw * 4) as u32, 0);
        surface.EndDraw()?;
        Ok(surface)
    }
}

/// Covers `monitor` with a still picture of itself, static and committed (the caller raises
/// its overlays, then `wait_committed` + DwmFlush, then switches, then `go`). False: use GDI.
pub fn start_sheet(monitor: &str, rect: RECT, slide: Slide, scale: f32, now: u64) -> bool {
    if std::env::var_os("SCREEN_SHIFTER_GDI").is_some() {
        return false;
    }
    let factory: IDXGIFactory1 = match unsafe { CreateDXGIFactory1() } {
        Ok(f) => f,
        Err(e) => return { log_err("factory", e); false },
    };
    let Some((adapter, output)) = find_output(&factory, monitor) else {
        crate::log!("gpu: no DXGI output for {monitor}");
        return false;
    };
    GPU.with(|cell| {
        let mut cell = cell.borrow_mut();
        // One device, on the adapter driving the monitors; a monitor elsewhere uses GDI.
        if cell.as_ref().is_some_and(|g| unsafe { g.adapter.GetDesc1() }.ok().map(|d| d.AdapterLuid)
            != unsafe { adapter.GetDesc1() }.ok().map(|d| d.AdapterLuid))
        {
            crate::log!("gpu: {monitor} is on another adapter");
            return false;
        }
        if cell.is_none() {
            match init(adapter) {
                Ok(g) => *cell = Some(g),
                Err(e) => return { log_err("init", e); false },
            }
        }
        let g = cell.as_mut().expect("initialized");
        match start(g, monitor, rect, &output, slide, scale, now) {
            Ok(started) => started,
            Err(e) => {
                log_err("sheet", &e);
                if e.code() == DXGI_ERROR_DEVICE_REMOVED || e.code() == DXGI_ERROR_DEVICE_RESET {
                    drop_state(&mut cell);
                }
                false
            }
        }
    })
}

fn start(g: &mut Gpu, monitor: &str, rect: RECT, output: &IDXGIOutput1, slide: Slide, scale: f32, now: u64) -> windows::core::Result<bool> {
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if !g.overlays.iter().any(|o| o.monitor == monitor) {
        let o = make_overlay(&g.dcomp, monitor, rect)?;
        g.overlays.push(o);
    }
    let unsafe_flush = {
        let o = g.overlays.iter().find(|o| o.monitor == monitor).expect("made above");
        // Duplication returns the last *presented* image: don't capture a switch's old picture.
        o.last_switch > unsafe { g.dcomp.GetFrameStatistics() }?.lastFrameTime
    };
    if unsafe_flush {
        unsafe {
            let _ = DwmFlush();
        }
    }
    let t = std::time::Instant::now();
    let Some(surface) = capture(g, output, w, h)? else { return Ok(false) };
    crate::log!("  gpu capture {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);

    let shadow = (crate::anim::SHADOW as f32 * scale) as i32;
    let (motion, shadow_x, at_left) = frame_motion(slide, w, shadow, now);
    unsafe {
        let visual = g.dcomp.CreateVisual()?;
        visual.SetContent(&surface)?;
        let shade = g.dcomp.CreateVisual()?;
        shade.SetContent(&shadow_surface(g, shadow, h, at_left)?)?;
        shade.SetOffsetX2(shadow_x as f32)?;
        visual.AddVisual(&shade, true, None::<&IDCompositionVisual>)?;
        visual.SetOffsetX2(0.0)?;
        let overlay = g.overlays.iter().find(|o| o.monitor == monitor).expect("made above");
        // Newest at the bottom: running sheets hurry off on top of it.
        overlay.root.AddVisual(&visual, false, None::<&IDCompositionVisual>)?;
        let (begin, begin_ms) = begin_time(&g.dcomp, 0)?;
        for s in g.sheets.iter_mut().filter(|s| s.monitor == monitor) {
            if let Some(m) = s.motion {
                let hurried = Motion { from: m.at(begin_ms), to: m.to, start: begin_ms, dur: (m.start + m.dur).saturating_sub(begin_ms).min(CHAIN_MS) };
                s.visual.SetOffsetX(&animation(&g.dcomp, &hurried, begin)?)?;
                s.motion = Some(hurried);
            }
        }
        let _ = SetWindowPos(overlay.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        g.sheets.push(Sheet { monitor: monitor.to_string(), visual, motion: None, target: motion.to, created: now });
        g.dcomp.Commit()?;
    }
    Ok(true)
}

/// The frame's motion in overlay pixels (sheet covering the monitor at 0), the shadow
/// strip's x relative to the frame, and whether the sheet is on the strip's left.
pub fn frame_motion(slide: Slide, w: i32, shadow: i32, now: u64) -> (Motion, i32, bool) {
    let gone = (w + shadow) as f32;
    match slide {
        // leaving left: trailing edge (and shadow) on the right
        Slide::Left => (Motion { from: 0.0, to: -gone, start: now, dur: SHEET_MS }, w, true),
        Slide::Right => (Motion { from: 0.0, to: gone, start: now, dur: SHEET_MS }, -shadow, false),
    }
}

/// The next composition frame at least 2 ms away, plus `delay_ms` (QPC), and the same
/// instant on `now_ms`. An idle DWM can report a stale estimate, possibly several frames
/// in the past.
fn begin_time(dcomp: &IDCompositionDevice, delay_ms: u64) -> windows::core::Result<(i64, u64)> {
    let st = unsafe { dcomp.GetFrameStatistics() }?;
    let (now, freq) = (qpc(), st.timeFrequency.max(1));
    let min = now + freq * 2 / 1000;
    let mut t = st.nextEstimatedFrameTime;
    if t < min {
        let rate = st.currentCompositionRate;
        let period = if rate.Numerator > 0 { freq * rate.Denominator as i64 / rate.Numerator as i64 } else { freq / 60 };
        let k = (min - st.lastFrameTime + period - 1) / period.max(1);
        t = st.lastFrameTime + k * period;
    }
    let ms = crate::win::now_ms() + ((t - now).max(0) * 1000 / freq) as u64;
    Ok((t + delay_ms as i64 * freq / 1000, ms + delay_ms))
}

/// The eased slide as a composition animation starting exactly at QPC `begin`.
fn animation(dcomp: &IDCompositionDevice, m: &Motion, begin: i64) -> windows::core::Result<IDCompositionAnimation> {
    let secs = m.dur.max(1) as f64 / 1000.0;
    let (c, l, q, k) = cubic_coeffs(m.from, m.to, secs as f32);
    unsafe {
        let a = dcomp.CreateAnimation()?;
        a.SetAbsoluteBeginTime(begin)?;
        a.AddCubic(0.0, c, l, q, k)?;
        a.End(secs, m.to)?;
        Ok(a)
    }
}

/// Blocks until the last commit is in the compositor (then one DwmFlush puts it on screen).
pub fn wait_committed() {
    GPU.with(|g| {
        if let Some(g) = g.borrow().as_ref() {
            unsafe {
                let _ = g.dcomp.WaitForCommitCompletion();
            }
        }
    });
}

/// The windows underneath changed on `monitor` (for the next capture's freshness check).
pub fn switched(monitor: &str) {
    GPU.with(|g| {
        if let Some(g) = g.borrow_mut().as_mut()
            && let Some(o) = g.overlays.iter_mut().find(|o| o.monitor == monitor)
        {
            o.last_switch = qpc();
        }
    });
}

/// Starts the slide of every sheet still waiting (the switch underneath is done).
pub fn go() {
    GPU.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(g) = cell.as_mut() else { return };
        let result = (|| -> windows::core::Result<()> {
            if !g.sheets.iter().any(|s| s.motion.is_none()) {
                return Ok(());
            }
            // hold still while the hides/shows underneath land, then slide
            let (begin, begin_ms) = begin_time(&g.dcomp, SETTLE_MS)?;
            for s in g.sheets.iter_mut().filter(|s| s.motion.is_none()) {
                let m = Motion { from: 0.0, to: s.target, start: begin_ms, dur: SHEET_MS };
                unsafe { s.visual.SetOffsetX(&animation(&g.dcomp, &m, begin)?)? };
                s.motion = Some(m);
            }
            unsafe { g.dcomp.Commit() }
        })();
        if let Err(e) = result {
            log_err("go", e);
            clear(g);
        }
    });
}

/// Removes sheets that finished (or outlived the hard limit). True while any remain.
pub fn reap(now: u64) -> bool {
    GPU.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(g) = cell.as_mut() else { return false };
        let done = |s: &Sheet| now.saturating_sub(s.created) > HARD_LIMIT_MS || s.motion.is_some_and(|m| m.done(now));
        let (gone, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut g.sheets).into_iter().partition(done);
        g.sheets = keep;
        if !gone.is_empty() {
            remove(g, &gone);
        }
        !g.sheets.is_empty()
    })
}

fn remove(g: &Gpu, gone: &[Sheet]) {
    unsafe {
        for s in gone {
            if let Some(o) = g.overlays.iter().find(|o| o.monitor == s.monitor) {
                let _ = o.root.RemoveVisual(&s.visual);
            }
        }
        let _ = g.dcomp.Commit();
        for o in &g.overlays {
            if !g.sheets.iter().any(|s| s.monitor == o.monitor) {
                let _ = ShowWindow(o.hwnd, SW_HIDE);
            }
        }
    }
}

fn clear(g: &mut Gpu) {
    let gone = std::mem::take(&mut g.sheets);
    remove(g, &gone);
}

/// Running GPU sheets on `monitor` (a GDI sheet there would cover them).
pub fn busy(monitor: &str) -> bool {
    GPU.with(|g| g.borrow().as_ref().is_some_and(|g| g.sheets.iter().any(|s| s.monitor == monitor)))
}

pub fn any() -> bool {
    GPU.with(|g| g.borrow().as_ref().is_some_and(|g| !g.sheets.is_empty()))
}

pub fn clear_all() {
    GPU.with(|g| {
        if let Some(g) = g.borrow_mut().as_mut() {
            clear(g);
        }
    });
}

fn drop_state(cell: &mut Option<Gpu>) {
    if let Some(mut g) = cell.take() {
        clear(&mut g);
        for o in &g.overlays {
            unsafe {
                let _ = DestroyWindow(o.hwnd);
            }
        }
    }
}

/// Monitors changed (or the device was lost): rebuild lazily on the next sheet.
pub fn reset() {
    GPU.with(|g| drop_state(&mut g.borrow_mut()));
}
