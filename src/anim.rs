//! Illusory switch motion: a still picture of the monitor ("sheet") covers it while the
//! real switch happens underneath, then slides off. GPU sheets (`gpu.rs`) are moved by the
//! compositor; this file's GDI sheets are the fallback, stepped from the main loop.

use std::cell::RefCell;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

pub const SHEET_MS: u64 = 400;
pub const CHAIN_MS: u64 = 120;
/// The sheet holds still this long after the switch before sliding: hides and shows are
/// asynchronous, and sliding while they land shows windows popping in and out.
pub const SETTLE_MS: u64 = 100;
pub const HARD_LIMIT_MS: u64 = 1000;
pub const SHADOW: i32 = 24;
pub const SHADOW_ALPHA: f32 = 0.35;

// ---------- pure helpers ----------

pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// `ease_out` from `from` to `to` over `secs` as polynomial coefficients
/// (constant, linear, quadratic, cubic) in seconds: 1-(1-u)^3 = 3u - 3u^2 + u^3.
pub fn cubic_coeffs(from: f32, to: f32, secs: f32) -> (f32, f32, f32, f32) {
    let d = to - from;
    (from, 3.0 * d / secs, -3.0 * d / (secs * secs), d / (secs * secs * secs))
}

/// A value moving from `from` to `to` over `dur` ms starting at `start`, eased.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub from: f32,
    pub to: f32,
    pub start: u64,
    pub dur: u64,
}

impl Motion {
    pub fn progress(&self, now: u64) -> f32 {
        if self.dur == 0 {
            return 1.0;
        }
        (now.saturating_sub(self.start) as f32 / self.dur as f32).min(1.0)
    }

    pub fn at(&self, now: u64) -> f32 {
        self.from + (self.to - self.from) * ease_out(self.progress(now))
    }

    pub fn done(&self, now: u64) -> bool {
        self.progress(now) >= 1.0
    }

    /// Continue from the current position, finishing within `CHAIN_MS`.
    pub fn hurried(&self, now: u64) -> Motion {
        let remaining = (self.start + self.dur).saturating_sub(now);
        Motion { from: self.at(now), to: self.to, start: now, dur: remaining.min(CHAIN_MS) }
    }
}

/// Part of a sheet (left edge `x`, width `w`) inside the monitor span `[left, right)`:
/// (screen x, visible width, source x within the sheet bitmap).
pub fn clip(x: i32, w: i32, left: i32, right: i32) -> Option<(i32, i32, i32)> {
    let (a, b) = (x.max(left), (x + w).min(right));
    (b > a).then_some((a, b - a, a - x))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slide {
    /// To a higher slot: the sheet leaves to the left.
    Left,
    /// To a lower slot: the sheet leaves to the right.
    Right,
}

/// Sheet bitmap is the frame plus a shadow on its trailing edge. Returns
/// (frame offset in the bitmap, motion of the sheet's left edge).
pub fn sheet_layout(slide: Slide, mon_left: i32, mon_w: i32, shadow: i32, now: u64) -> (i32, Motion) {
    let bw = mon_w + shadow;
    let (offset, from, to) = match slide {
        Slide::Left => (0, mon_left, mon_left - bw),
        Slide::Right => (shadow, mon_left - shadow, mon_left + mon_w),
    };
    (offset, Motion { from: from as f32, to: to as f32, start: now, dur: SHEET_MS })
}

// ---------- sheets ----------

struct Sheet {
    monitor: String,
    hwnd: HWND,
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    rect: RECT,
    width: i32,
    motion: Motion,
    created: u64,
    /// The motion's clock starts at the first animation frame, not at creation: the
    /// capture and the switch work in between would otherwise eat most of the slide.
    started: bool,
    /// diagnostics: frames drawn, last frame time, worst gap between frames
    frames: u32,
    last: u64,
    worst_gap: u64,
    draw_us: u128,
    /// Created since the last composed frame: its capture would be stale for another switch.
    fresh: bool,
}

impl Sheet {
    fn destroy(self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }

    /// Draws the current frame; false when the sheet is fully off its monitor.
    fn draw(&self, now: u64) -> bool {
        let x = self.motion.at(now).round() as i32;
        let Some((sx, w, src)) = clip(x, self.width, self.rect.left, self.rect.right) else {
            return false;
        };
        let h = self.rect.bottom - self.rect.top;
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        unsafe {
            UpdateLayeredWindow(self.hwnd, None, Some(&POINT { x: sx, y: self.rect.top }), Some(&SIZE { cx: w, cy: h }),
                Some(self.dc), Some(&POINT { x: src, y: 0 }), COLORREF(0), Some(&blend), ULW_ALPHA)
                .is_ok()
        }
    }
}

thread_local! {
    static SHEETS: RefCell<Vec<Sheet>> = const { RefCell::new(Vec::new()) };
}

unsafe extern "system" fn sheet_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        return LRESULT(0); // only the app window decides when we close
    }
    unsafe { DefWindowProcW(h, msg, wp, lp) }
}

/// Our overlays must never appear in captures (or a stale copy would slide away).
pub fn exclude_from_capture(h: HWND) {
    unsafe {
        let _ = SetWindowDisplayAffinity(h, WDA_EXCLUDEFROMCAPTURE);
    }
}

/// Captures `rect` into a 32-bpp sheet bitmap with a premultiplied shadow strip.
fn capture(rect: RECT, slide: Slide, shadow: i32) -> Option<(HDC, HBITMAP, HGDIOBJ, i32)> {
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    let bw = w + shadow;
    let offset = if slide == Slide::Left { 0 } else { shadow };
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bw,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(Some(screen), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            ReleaseDC(None, screen);
            let _ = DeleteDC(dc);
            return None;
        };
        let old = SelectObject(dc, bmp.into());
        let ok = BitBlt(dc, offset, 0, w, h, Some(screen), rect.left, rect.top, SRCCOPY).is_ok();
        ReleaseDC(None, screen);
        if !ok {
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            return None;
        }
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (bw * h) as usize);
        for row in px.chunks_exact_mut(bw as usize) {
            for (i, p) in row.iter_mut().enumerate() {
                let i = i as i32;
                let in_frame = i >= offset && i < offset + w;
                *p = if in_frame {
                    *p | 0xFF00_0000
                } else {
                    // distance from the frame edge, 0 = touching it
                    let d = if i < offset { offset - 1 - i } else { i - offset - w };
                    let k = 1.0 - d as f32 / shadow as f32;
                    ((SHADOW_ALPHA * k * k * 255.0) as u32) << 24 // premultiplied black
                };
            }
        }
        Some((dc, bmp, old, bw))
    }
}

/// Covers `monitor` with a picture of itself that will slide off. Returns false (and the
/// switch just happens without motion) if capture fails or this monitor already got a
/// sheet this frame. Caller must `DwmFlush` before changing what's on screen.
pub fn start_sheet(monitor: &str, rect: RECT, slide: Slide, scale: f32, now: u64) -> bool {
    // One path per monitor while sheets run: a newer overlay would cover a GDI sheet and
    // vice versa. GPU first; if it fails while GPU sheets run here, this switch gets none.
    let gdi_running = SHEETS.with(|s| s.borrow().iter().any(|s| s.monitor == monitor));
    if !gdi_running && crate::gpu::start_sheet(monitor, rect, slide, scale, now) {
        return true;
    }
    if crate::gpu::busy(monitor) {
        return false;
    }
    let busy = SHEETS.with(|s| s.borrow().iter().any(|s| s.monitor == monitor && s.fresh));
    if busy {
        return false;
    }
    let shadow = (SHADOW as f32 * scale) as i32;
    let t = std::time::Instant::now();
    let Some((dc, bmp, old, width)) = capture(rect, slide, shadow) else { return false };
    crate::log!("  capture {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    let (_, motion) = sheet_layout(slide, rect.left, rect.right - rect.left, shadow, now);
    let hwnd = unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default().into();
        let cls = w!("ScreenShifterSheet");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(sheet_proc), hInstance: inst, lpszClassName: cls, ..Default::default() });
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            cls, cls, WS_POPUP, 0, 0, 0, 0, None, None, Some(inst), None,
        )
        .unwrap_or_default()
    };
    exclude_from_capture(hwnd);
    let sheet = Sheet { monitor: monitor.to_string(), hwnd, dc, bmp, old, rect, width, motion, created: now, started: false, frames: 0, last: 0, worst_gap: 0, draw_us: 0, fresh: true };
    sheet.draw(now);
    // Below any sheets still moving on this monitor; they hurry off on top of it.
    let below = SHEETS.with(|s| {
        let mut s = s.borrow_mut();
        for other in s.iter_mut().filter(|o| o.monitor == monitor) {
            other.motion = other.motion.hurried(now);
        }
        s.iter().rev().find(|o| o.monitor == monitor).map(|o| o.hwnd)
    });
    unsafe {
        let _ = SetWindowPos(hwnd, Some(below.unwrap_or(HWND_TOPMOST)), 0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
    SHEETS.with(|s| s.borrow_mut().push(sheet));
    true
}

/// Advances every sheet; destroys finished ones and any past the hard limit.
pub fn step(now: u64) {
    let sheets = SHEETS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let mut keep = Vec::new();
    for mut sheet in sheets {
        if !sheet.started {
            crate::log!("sheet on {}: first frame {} ms after capture", sheet.monitor, now.saturating_sub(sheet.created));
            sheet.motion.start = now + SETTLE_MS;
            sheet.started = true;
            sheet.last = now;
        }
        sheet.frames += 1;
        sheet.worst_gap = sheet.worst_gap.max(now.saturating_sub(sheet.last));
        sheet.last = now;
        let expired = now.saturating_sub(sheet.created) > HARD_LIMIT_MS;
        let t = std::time::Instant::now();
        let drawn = !expired && sheet.draw(now);
        sheet.draw_us += t.elapsed().as_micros();
        if drawn && !sheet.motion.done(now) {
            keep.push(sheet);
        } else {
            crate::log!("  slide: {} frames in {} ms, worst gap {} ms, avg draw {:.1} ms{}", sheet.frames,
                now.saturating_sub(sheet.motion.start), sheet.worst_gap,
                sheet.draw_us as f64 / 1000.0 / sheet.frames.max(1) as f64, if expired { " (hard limit)" } else { "" });
            sheet.destroy();
        }
    }
    SHEETS.with(|s| s.borrow_mut().extend(keep));
}

/// Timer tick: removes finished GPU sheets, and enforces the hard limit on GDI sheets
/// (whose frames don't step inside modal loops).
pub fn enforce_limit(now: u64) {
    crate::gpu::reap(now);
    let sheets = SHEETS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let (expired, keep): (Vec<_>, Vec<_>) =
        sheets.into_iter().partition(|s| now.saturating_sub(s.created) > HARD_LIMIT_MS);
    expired.into_iter().for_each(Sheet::destroy);
    SHEETS.with(|s| s.borrow_mut().extend(keep));
}

/// A frame was composed: pictures captured from now on are current.
pub fn frame_composed() {
    SHEETS.with(|s| s.borrow_mut().iter_mut().for_each(|s| s.fresh = false));
}

/// GDI sheets need the main loop's frame mode; GPU sheets don't.
pub fn active() -> bool {
    SHEETS.with(|s| !s.borrow().is_empty())
}

/// Any sheet of either kind (keeps the backstop timer alive).
pub fn any() -> bool {
    active() || crate::gpu::any()
}

pub fn clear_all() {
    crate::gpu::clear_all();
    let sheets = SHEETS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    sheets.into_iter().for_each(Sheet::destroy);
}

/// Windows' "Animation effects" setting.
pub fn enabled() -> bool {
    let mut on = windows::core::BOOL(1);
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut on as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    on.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_is_monotonic_and_bounded() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert_eq!(ease_out(2.0), 1.0);
        let mut last = 0.0;
        for i in 1..=10 {
            let v = ease_out(i as f32 / 10.0);
            assert!(v > last);
            last = v;
        }
        assert!(ease_out(0.5) > 0.5, "ease-out front-loads the motion");
    }

    #[test]
    fn motion_runs_from_start_to_end() {
        let m = Motion { from: 0.0, to: -100.0, start: 1000, dur: 200 };
        assert_eq!(m.at(1000), 0.0);
        assert_eq!(m.at(500), 0.0, "before start");
        assert_eq!(m.at(1200), -100.0);
        assert!(m.done(1200) && !m.done(1199));
    }

    #[test]
    fn cubic_matches_ease_out() {
        let (from, to, secs) = (0.0f32, -2584.0f32, 0.4f32);
        let (c, l, q, k) = cubic_coeffs(from, to, secs);
        for i in 0..=10 {
            let t = secs * i as f32 / 10.0;
            let poly = c + l * t + q * t * t + k * t * t * t;
            let eased = from + (to - from) * ease_out(t / secs);
            assert!((poly - eased).abs() < 0.5, "t={t}: {poly} vs {eased}");
        }
    }

    #[test]
    fn gpu_frame_motion_matches_gdi_layout() {
        // The GDI bitmap's left edge includes the shadow for Right; the GPU visual is the frame.
        for slide in [Slide::Left, Slide::Right] {
            let (off, m) = sheet_layout(slide, 2560, 2560, 24, 0);
            let (g, shadow_x, at_left) = crate::gpu::frame_motion(slide, 2560, 24, 0);
            assert_eq!(g.from, m.from + off as f32 - 2560.0, "{slide:?} from");
            assert_eq!(g.to, m.to + off as f32 - 2560.0, "{slide:?} to");
            assert_eq!(at_left, slide == Slide::Left);
            assert_eq!(shadow_x, if slide == Slide::Left { 2560 } else { -24 });
        }
    }

    #[test]
    fn hurried_motion_continues_and_finishes_fast() {
        let m = Motion { from: 0.0, to: -1000.0, start: 0, dur: SHEET_MS };
        let h = m.hurried(50);
        assert_eq!(h.from, m.at(50), "no jump");
        assert_eq!(h.dur, CHAIN_MS);
        let late = m.hurried(SHEET_MS - 20);
        assert_eq!(late.dur, 20, "never slower than what was left");
    }

    #[test]
    fn clip_keeps_sheet_on_its_monitor() {
        // monitor A at [0, 2560), monitor B to its right
        assert_eq!(clip(0, 2584, 0, 2560), Some((0, 2560, 0)));
        assert_eq!(clip(-1000, 2584, 0, 2560), Some((0, 1584, 1000)));
        assert_eq!(clip(-2584, 2584, 0, 2560), None);
        // sliding right: never spills past the right edge onto B
        assert_eq!(clip(1000, 2584, 0, 2560), Some((1000, 1560, 0)));
        // monitor at negative coordinates
        assert_eq!(clip(-2560, 2584, -2560, 0), Some((-2560, 2560, 0)));
    }

    #[test]
    fn sheet_layouts_cover_then_leave() {
        let (off, m) = sheet_layout(Slide::Left, 0, 2560, 24, 0);
        assert_eq!(off, 0);
        assert_eq!(clip(m.from as i32, 2584, 0, 2560), Some((0, 2560, 0)), "covers exactly at start");
        assert_eq!(clip(m.to as i32, 2584, 0, 2560), None, "fully gone at end");

        let (off, m) = sheet_layout(Slide::Right, 2560, 2560, 24, 0);
        assert_eq!(off, 24);
        assert_eq!(clip(m.from as i32, 2584, 2560, 5120), Some((2560, 2560, 24)), "frame aligned with monitor");
        assert_eq!(clip(m.to as i32, 2584, 2560, 5120), None);
    }
}
