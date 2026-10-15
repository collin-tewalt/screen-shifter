//! Overlay windows: a slot panel per monitor and the drag ghost.
//! Display only: they never take focus or clicks. All input arrives through the hooks.

use crate::anim::{self, ease_out};
use crate::core::{MAX_SLOTS, Target};
use crate::win::{self, MonitorInfo};
use std::cell::{Cell, RefCell};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

/// What a panel shows: per slot, its windows' icons (invalid = none) and titles.
pub struct PanelData {
    pub monitor: String,
    pub active: usize,
    pub titles: Vec<Vec<(HICON, String)>>,
}

struct Panel {
    data: PanelData,
    hwnd: HWND,
    rect: RECT, // screen coordinates
    /// Drawing scale: the monitor's DPI scale, shrunk if the tiles wouldn't fit.
    scale: f32,
    monitor: MonitorInfo,
}

/// The ghost's scaled snapshot, kept after the drag so the soul can fly with it.
struct Snap {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    w: i32,
    h: i32,
}

struct Flight {
    from: RECT,
    to: RECT,
    start: u64,
}

thread_local! {
    static PANELS: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
    static DROP: RefCell<Option<(String, Target)>> = const { RefCell::new(None) };
    static GHOST: Cell<HWND> = Cell::new(HWND::default());
    static SNAP: RefCell<Option<Snap>> = const { RefCell::new(None) };
    static FLIGHT: RefCell<Option<Flight>> = const { RefCell::new(None) };
    /// (monitor, slot, start) of a tile that just received a window
    static PULSE: RefCell<Option<(String, usize, u64)>> = const { RefCell::new(None) };
    /// (monitor, slot) of the x under the cursor
    static CLOSE_HOT: RefCell<Option<(String, usize)>> = const { RefCell::new(None) };
}

const FLY_MS: u64 = 180;
const PULSE_MS: u64 = 300;
const GHOST_ALPHA: u8 = 150;

const TILE: (i32, i32) = (220, 150);
const GAP: i32 = 12;
/// The close x: (diameter, margin from the tile's corner).
const CLOSE: (i32, i32) = (22, 6);
const MAX_TITLES: usize = 5;
const GHOST_MAX: (i32, i32) = (360, 240);
const BG: COLORREF = COLORREF(0x00202020);
const TILE_BG: COLORREF = COLORREF(0x00303030);
const DROP_BG: COLORREF = COLORREF(0x00585858);
const ACCENT: COLORREF = COLORREF(0x00D77800); // BGR: Windows blue
const TEXT: COLORREF = COLORREF(0x00F0F0F0);
const DIM: COLORREF = COLORREF(0x00909090);

fn px(v: i32, scale: f32) -> i32 {
    (v as f32 * scale) as i32
}

/// Tile `i` in panel client coordinates.
fn tile_rect(i: usize, scale: f32) -> RECT {
    let (tw, th, g) = (px(TILE.0, scale), px(TILE.1, scale), px(GAP, scale));
    let left = g + i as i32 * (tw + g);
    RECT { left, top: g, right: left + tw, bottom: g + th }
}

/// Slot tiles plus "+ New" below the limit.
fn tiles(data: &PanelData) -> usize {
    data.titles.len() + (data.titles.len() < MAX_SLOTS) as usize
}

/// The x that closes slot tile `i`, in panel client coordinates (its top-right corner).
fn close_rect(i: usize, scale: f32) -> RECT {
    let (t, size, m) = (tile_rect(i, scale), px(CLOSE.0, scale), px(CLOSE.1, scale));
    RECT { left: t.right - m - size, top: t.top + m, right: t.right - m, bottom: t.top + m + size }
}

/// Tiles carry an x only when there is another slot to fold into.
fn has_close(data: &PanelData) -> bool {
    data.titles.len() >= 2
}

/// The tile under a panel-local point, and whether the point is on its x.
fn locate(data: &PanelData, scale: f32, local: POINT) -> Option<(usize, bool)> {
    let i = (0..tiles(data)).find(|&i| unsafe { PtInRect(&tile_rect(i, scale), local).as_bool() })?;
    let on_x = i < data.titles.len() && has_close(data) && unsafe { PtInRect(&close_rect(i, scale), local).as_bool() };
    Some((i, on_x))
}

/// What tile `i` stands for.
fn target(data: &PanelData, i: usize) -> Target {
    if i < data.titles.len() { Target::Slot(i) } else { Target::New }
}

/// Screen rect and drawing scale for a panel of `tiles` tiles, kept within 90% of the
/// monitor's width.
fn place(m: &MonitorInfo, tiles: usize) -> (RECT, f32) {
    let natural = tiles as i32 * (TILE.0 + GAP) + GAP;
    let fit = 0.9 * (m.rect.right - m.rect.left) as f32 / natural as f32;
    let s = m.scale.min(fit);
    let (w, h) = (px(natural, s), px(TILE.1 + 2 * GAP, s));
    let left = (m.work.left + m.work.right - w) / 2;
    let top = m.work.bottom - h - px(48, m.scale);
    (RECT { left, top, right: left + w, bottom: top + h }, s)
}

fn class(name: PCWSTR, proc_: WNDPROC) -> PCWSTR {
    unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default().into();
        RegisterClassW(&WNDCLASSW { lpfnWndProc: proc_, hInstance: inst, lpszClassName: name, ..Default::default() });
    }
    name
}

fn overlay(class_name: PCWSTR, r: RECT) -> HWND {
    unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class_name,
            class_name,
            WS_POPUP,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            None,
            None,
            None,
            None,
        )
        .unwrap_or_default()
    }
}

// ---------- panels ----------

pub fn show(monitors: &[MonitorInfo], mut data: Vec<PanelData>) {
    hide();
    let cls = class(w!("ScreenShifterPanel"), Some(panel_proc));
    let mut panels = Vec::new();
    for m in monitors {
        let Some(i) = data.iter().position(|d| d.monitor == m.name) else { continue };
        let d = data.swap_remove(i);
        let (rect, s) = place(m, tiles(&d));
        let hwnd = overlay(cls, rect);
        anim::exclude_from_capture(hwnd);
        // Windows 11 rounds the panel's corners; ignored elsewhere.
        let round = DWMWCP_ROUND;
        let _ = unsafe { DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as *const _, size_of_val(&round) as u32) };
        unsafe {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA);
        }
        panels.push(Panel { data: d, hwnd, rect, scale: s, monitor: m.clone() });
    }
    let hwnds: Vec<HWND> = panels.iter().map(|p| p.hwnd).collect();
    PANELS.with(|p| *p.borrow_mut() = panels);
    for h in hwnds {
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
    }
}

pub fn hide() {
    let old = PANELS.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for p in old {
        unsafe {
            let _ = DestroyWindow(p.hwnd);
        }
    }
    DROP.with(|d| *d.borrow_mut() = None);
    CLOSE_HOT.with(|c| *c.borrow_mut() = None);
}

pub fn is_shown() -> bool {
    PANELS.with(|p| !p.borrow().is_empty())
}

pub fn update(mut data: Vec<PanelData>) {
    CLOSE_HOT.with(|c| *c.borrow_mut() = None); // slot numbers may have changed under it
    PANELS.with(|p| {
        for panel in p.borrow_mut().iter_mut() {
            if let Some(i) = data.iter().position(|d| d.monitor == panel.data.monitor) {
                let resize = tiles(&data[i]) != tiles(&panel.data);
                panel.data = data.swap_remove(i);
                unsafe {
                    if resize {
                        (panel.rect, panel.scale) = place(&panel.monitor, tiles(&panel.data));
                        let r = panel.rect;
                        let _ = SetWindowPos(panel.hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top,
                            SWP_NOZORDER | SWP_NOACTIVATE);
                    }
                    let _ = InvalidateRect(Some(panel.hwnd), None, false);
                }
            }
        }
    });
}

/// None: not over a panel. Some(None): over a panel but between tiles. Some(Some(tile)).
/// A tile's x counts as part of the tile (hovering it keeps the tile's spread).
pub fn hit(pt: POINT) -> Option<Option<(String, Target)>> {
    hit_in(pt, true)
}

/// Like `hit`, but a tile's x counts as between tiles: for what acts on a tile (a click, a
/// release, a drop). An x is neither a click-through to its tile nor a drop target.
pub fn tile_hit(pt: POINT) -> Option<Option<(String, Target)>> {
    hit_in(pt, false)
}

fn hit_in(pt: POINT, with_x: bool) -> Option<Option<(String, Target)>> {
    PANELS.with(|p| {
        let p = p.borrow();
        let panel = p.iter().find(|p| unsafe { PtInRect(&p.rect, pt).as_bool() })?;
        let local = POINT { x: pt.x - panel.rect.left, y: pt.y - panel.rect.top };
        let tile = locate(&panel.data, panel.scale, local).filter(|&(_, on_x)| with_x || !on_x);
        Some(tile.map(|(i, _)| (panel.data.monitor.clone(), target(&panel.data, i))))
    })
}

/// The slot whose x is under `pt`: (monitor, slot).
pub fn hit_close(pt: POINT) -> Option<(String, usize)> {
    PANELS.with(|p| {
        let p = p.borrow();
        let panel = p.iter().find(|p| unsafe { PtInRect(&p.rect, pt).as_bool() })?;
        let local = POINT { x: pt.x - panel.rect.left, y: pt.y - panel.rect.top };
        match locate(&panel.data, panel.scale, local)? {
            (i, true) => Some((panel.data.monitor.clone(), i)),
            _ => None,
        }
    })
}

/// Highlights the x under `pt` (if any).
pub fn set_close_hover(pt: POINT) {
    let now = hit_close(pt);
    if CLOSE_HOT.with(|c| c.replace(now.clone()) != now) {
        invalidate_all();
    }
}

/// Screen rect of a tile, for the soul to fly into.
pub fn tile_screen_rect(monitor: &str, slot: usize) -> Option<RECT> {
    PANELS.with(|p| {
        let p = p.borrow();
        let panel = p.iter().find(|p| p.data.monitor == monitor)?;
        let r = tile_rect(slot, panel.scale);
        let (dx, dy) = (panel.rect.left, panel.rect.top);
        Some(RECT { left: r.left + dx, top: r.top + dy, right: r.right + dx, bottom: r.bottom + dy })
    })
}

/// The panel window and its screen rect on `monitor`, if shown.
pub fn window_of(monitor: &str) -> Option<(HWND, RECT)> {
    PANELS.with(|p| p.borrow().iter().find(|p| p.data.monitor == monitor).map(|p| (p.hwnd, p.rect)))
}

/// Every visible overlay that must stay above switch sheets.
pub fn overlay_hwnds() -> Vec<HWND> {
    let mut v: Vec<HWND> = PANELS.with(|p| p.borrow().iter().map(|p| p.hwnd).collect());
    let ghost = GHOST.get();
    if unsafe { IsWindowVisible(ghost).as_bool() } {
        v.push(ghost);
    }
    v
}

pub fn pulse(monitor: &str, slot: usize) {
    PULSE.with(|p| *p.borrow_mut() = Some((monitor.to_string(), slot, win::now_ms())));
    invalidate_all();
}

fn invalidate_all() {
    PANELS.with(|p| {
        for panel in p.borrow().iter() {
            unsafe {
                let _ = InvalidateRect(Some(panel.hwnd), None, false);
            }
        }
    });
}

/// Advances the soul's flight and the tile pulse. True while either is running.
pub fn step(now: u64) -> bool {
    let flying = FLIGHT.with(|f| f.borrow().as_ref().map(|f| (f.from, f.to, f.start)));
    if let Some((from, to, start)) = flying {
        let t = (now.saturating_sub(start) as f32 / FLY_MS as f32).min(1.0);
        if t >= 1.0 {
            ghost_end();
        } else {
            let e = ease_out(t);
            let lerp = |a: i32, b: i32| a + ((b - a) as f32 * e) as i32;
            let r = RECT { left: lerp(from.left, to.left), top: lerp(from.top, to.top), right: lerp(from.right, to.right), bottom: lerp(from.bottom, to.bottom) };
            render_ghost(r, (GHOST_ALPHA as f32 * (1.0 - e)) as u8);
        }
    }
    // (still pulsing, pulse just ended): repaint during the pulse and once after it
    let (pulsing, ended) = PULSE.with(|p| {
        let mut p = p.borrow_mut();
        match &*p {
            Some((_, _, start)) if now.saturating_sub(*start) < PULSE_MS => (true, false),
            Some(_) => {
                *p = None;
                (false, true)
            }
            None => (false, false),
        }
    });
    if pulsing || ended {
        invalidate_all();
    }
    FLIGHT.with(|f| f.borrow().is_some()) || pulsing
}

/// Lets the ghost finish its trip: glide and scale to `to`, fading out.
pub fn ghost_fly(to: RECT) {
    let ghost = GHOST.get();
    let mut from = RECT::default();
    unsafe {
        let _ = GetWindowRect(ghost, &mut from);
    }
    if SNAP.with(|s| s.borrow().is_none()) {
        ghost_end();
        return;
    }
    FLIGHT.with(|f| *f.borrow_mut() = Some(Flight { from, to, start: win::now_ms() }));
}

fn render_ghost(r: RECT, alpha: u8) {
    let (w, h) = ((r.right - r.left).max(1), (r.bottom - r.top).max(1));
    SNAP.with(|s| {
        let s = s.borrow();
        let Some(snap) = s.as_ref() else { return };
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(dc, bmp.into());
            SetStretchBltMode(dc, HALFTONE);
            let _ = StretchBlt(dc, 0, 0, w, h, Some(snap.dc), 0, 0, snap.w, snap.h, SRCCOPY);
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: alpha, AlphaFormat: 0 };
            let _ = UpdateLayeredWindow(GHOST.get(), Some(screen), Some(&POINT { x: r.left, y: r.top }), Some(&SIZE { cx: w, cy: h }),
                Some(dc), Some(&POINT::default()), COLORREF(0), Some(&blend), ULW_ALPHA);
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            ReleaseDC(None, screen);
        }
    });
}

fn drop_snap() {
    if let Some(s) = SNAP.with(|s| s.borrow_mut().take()) {
        unsafe {
            SelectObject(s.dc, s.old);
            let _ = DeleteObject(s.bmp.into());
            let _ = DeleteDC(s.dc);
        }
    }
}

pub fn set_drop(target: Option<(String, Target)>) {
    let changed = DROP.with(|d| d.replace(target.clone()) != target);
    if changed {
        PANELS.with(|p| {
            for panel in p.borrow().iter() {
                unsafe {
                    let _ = InvalidateRect(Some(panel.hwnd), None, false);
                }
            }
        });
    }
}

unsafe fn fill(dc: HDC, r: &RECT, c: COLORREF) {
    unsafe {
        let b = CreateSolidBrush(c);
        FillRect(dc, r, b);
        let _ = DeleteObject(b.into());
    }
}

unsafe fn text(dc: HDC, s: &str, r: &mut RECT, color: COLORREF, flags: DRAW_TEXT_FORMAT) {
    unsafe {
        SetTextColor(dc, color);
        let mut w: Vec<u16> = s.encode_utf16().collect();
        DrawTextW(dc, &mut w, r, flags | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
    }
}

/// Blend two COLORREFs, `k` = 0 → a, 1 → b.
fn mix(a: COLORREF, b: COLORREF, k: f32) -> COLORREF {
    let ch = |shift: u32| {
        let (x, y) = (((a.0 >> shift) & 0xFF) as f32, ((b.0 >> shift) & 0xFF) as f32);
        ((x + (y - x) * k) as u32) << shift
    };
    COLORREF(ch(0) | ch(8) | ch(16))
}

fn font(height: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(height, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"))
    }
}

unsafe fn paint(h: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(h, &mut ps);
        PANELS.with(|p| {
            let p = p.borrow();
            let Some(panel) = p.iter().find(|p| p.hwnd == h) else { return };
            let drop = DROP.with(|d| d.borrow().clone());
            let s = panel.scale;
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            fill(dc, &rc, BG);
            SetBkMode(dc, TRANSPARENT);
            let (big, small) = (font(px(22, s), FW_SEMIBOLD.0 as i32), font(px(15, s), FW_NORMAL.0 as i32));
            for i in 0..tiles(&panel.data) {
                let r = tile_rect(i, s);
                let is_drop = drop.as_ref().is_some_and(|(m, t)| *m == panel.data.monitor && *t == target(&panel.data, i));
                if target(&panel.data, i) == Target::New {
                    if is_drop {
                        fill(dc, &r, DROP_BG);
                    }
                    let pen = CreatePen(PS_DASH, 1, DIM);
                    let (old_pen, old_brush) = (SelectObject(dc, pen.into()), SelectObject(dc, GetStockObject(NULL_BRUSH)));
                    let _ = Rectangle(dc, r.left, r.top, r.right, r.bottom);
                    SelectObject(dc, old_pen);
                    SelectObject(dc, old_brush);
                    let _ = DeleteObject(pen.into());
                    let old = SelectObject(dc, big.into());
                    let mut tr = r;
                    text(dc, "+ New", &mut tr, DIM, DT_CENTER | DT_VCENTER);
                    SelectObject(dc, old);
                    continue;
                }
                if i == panel.data.active {
                    fill(dc, &r, ACCENT);
                }
                let inner = if i == panel.data.active {
                    RECT { left: r.left + px(3, s), top: r.top + px(3, s), right: r.right - px(3, s), bottom: r.bottom - px(3, s) }
                } else {
                    r
                };
                let pulse = PULSE.with(|p| match &*p.borrow() {
                    Some((m, t, start)) if *m == panel.data.monitor && *t == i => {
                        1.0 - (win::now_ms().saturating_sub(*start) as f32 / PULSE_MS as f32).min(1.0)
                    }
                    _ => 0.0,
                });
                let base = if is_drop { DROP_BG } else { TILE_BG };
                fill(dc, &inner, if pulse > 0.0 { mix(base, ACCENT, pulse) } else { base });
                if has_close(&panel.data) {
                    let c = close_rect(i, s);
                    let hot = CLOSE_HOT.with(|h| h.borrow().as_ref().is_some_and(|(m, t)| *m == panel.data.monitor && *t == i));
                    let (pen, brush) = (GetStockObject(NULL_PEN), CreateSolidBrush(if hot { mix(TILE_BG, TEXT, 0.3) } else { mix(TILE_BG, BG, 0.5) }));
                    let (old_pen, old_brush) = (SelectObject(dc, pen), SelectObject(dc, brush.into()));
                    let _ = Ellipse(dc, c.left, c.top, c.right, c.bottom);
                    SelectObject(dc, old_pen);
                    SelectObject(dc, old_brush);
                    let _ = DeleteObject(brush.into());
                    let old = SelectObject(dc, big.into());
                    let mut cr = c;
                    text(dc, "×", &mut cr, if hot { TEXT } else { DIM }, DT_CENTER | DT_VCENTER);
                    SelectObject(dc, old);
                }
                let pad = px(10, s);
                let line = px(20, s);
                let old = SelectObject(dc, big.into());
                let mut tr = RECT { left: r.left + pad, top: r.top + pad / 2, right: r.right - pad, bottom: r.top + pad / 2 + px(28, s) };
                text(dc, &(i + 1).to_string(), &mut tr, TEXT, DT_LEFT);
                SelectObject(dc, small.into());
                let titles = &panel.data.titles[i];
                let mut y = tr.bottom;
                if titles.is_empty() {
                    let mut lr = RECT { left: r.left + pad, top: y, right: r.right - pad, bottom: y + line };
                    text(dc, "\u{2014}", &mut lr, DIM, DT_LEFT);
                }
                let icon = px(16, s);
                for (n, (ic, t)) in titles.iter().enumerate() {
                    let mut lr = RECT { left: r.left + pad, top: y, right: r.right - pad, bottom: y + line };
                    if n == MAX_TITLES - 1 && titles.len() > MAX_TITLES {
                        text(dc, &format!("+{} more", titles.len() - n), &mut lr, DIM, DT_LEFT);
                        break;
                    }
                    if !ic.is_invalid() {
                        let _ = DrawIconEx(dc, lr.left, y + (line - icon) / 2, *ic, icon, icon, 0, None, DI_NORMAL);
                    }
                    lr.left += icon + px(6, s);
                    text(dc, t, &mut lr, TEXT, DT_LEFT);
                    y += line;
                }
                SelectObject(dc, old);
            }
            let _ = DeleteObject(big.into());
            let _ = DeleteObject(small.into());
        });
        let _ = EndPaint(h, &ps);
    }
}

unsafe extern "system" fn panel_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_PAINT {
            paint(h);
            return LRESULT(0);
        }
        if msg == WM_CLOSE {
            return LRESULT(0); // only the app window decides when we close
        }
        DefWindowProcW(h, msg, wp, lp)
    }
}

// ---------- drag ghost ----------

unsafe extern "system" fn ghost_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(h, msg, wp, lp) }
}

/// Shows a translucent, size-capped snapshot of `src` at the cursor. False if it couldn't.
pub fn ghost_start(src: HWND, pt: POINT) -> bool {
    unsafe {
        let mut wr = RECT::default();
        if GetWindowRect(src, &mut wr).is_err() {
            return false;
        }
        let (w, h) = (wr.right - wr.left, wr.bottom - wr.top);
        if w <= 0 || h <= 0 {
            return false;
        }
        ghost_end(); // a soul still flying from the last drop
        if GHOST.get().is_invalid() {
            let g = overlay(class(w!("ScreenShifterGhost"), Some(ghost_proc)), RECT::default());
            anim::exclude_from_capture(g);
            GHOST.set(g);
        }
        let ghost = GHOST.get();

        let screen = GetDC(None);
        let full_dc = CreateCompatibleDC(Some(screen));
        let full = CreateCompatibleBitmap(screen, w, h);
        let old_full = SelectObject(full_dc, full.into());
        // PrintWindow sends messages to the target, so never call it on a hung app.
        let printed = !IsHungAppWindow(src).as_bool()
            && PrintWindow(src, full_dc, PRINT_WINDOW_FLAGS(2 /* PW_RENDERFULLCONTENT */)).as_bool();
        if !printed {
            let _ = BitBlt(full_dc, 0, 0, w, h, Some(screen), wr.left, wr.top, SRCCOPY);
        }

        let k = (GHOST_MAX.0 as f32 / w as f32).min(GHOST_MAX.1 as f32 / h as f32).min(1.0);
        let (sw, sh) = (((w as f32 * k) as i32).max(1), ((h as f32 * k) as i32).max(1));
        let small_dc = CreateCompatibleDC(Some(screen));
        let small = CreateCompatibleBitmap(screen, sw, sh);
        let old_small = SelectObject(small_dc, small.into());
        SetStretchBltMode(small_dc, HALFTONE);
        let _ = StretchBlt(small_dc, 0, 0, sw, sh, Some(full_dc), 0, 0, w, h, SRCCOPY);

        let pos = ghost_pos(pt, sw, sh);
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 150, AlphaFormat: 0 };
        let ok = UpdateLayeredWindow(ghost, Some(screen), Some(&pos), Some(&SIZE { cx: sw, cy: sh }),
            Some(small_dc), Some(&POINT::default()), COLORREF(0), Some(&blend), ULW_ALPHA).is_ok();

        // keep the small snapshot for the soul's flight after the drop
        SNAP.with(|s| *s.borrow_mut() = Some(Snap { dc: small_dc, bmp: small, old: old_small, w: sw, h: sh }));
        SelectObject(full_dc, old_full);
        let _ = DeleteObject(full.into());
        let _ = DeleteDC(full_dc);
        ReleaseDC(None, screen);

        if ok {
            let _ = SetWindowPos(ghost, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
        ok
    }
}

/// Cursor sits at the ghost's top-left quarter.
fn ghost_pos(pt: POINT, w: i32, h: i32) -> POINT {
    POINT { x: pt.x - w / 4, y: pt.y - h / 4 }
}

pub fn ghost_move(pt: POINT) {
    let ghost = GHOST.get();
    unsafe {
        let mut r = RECT::default();
        let _ = GetWindowRect(ghost, &mut r);
        let p = ghost_pos(pt, r.right - r.left, r.bottom - r.top);
        let _ = SetWindowPos(ghost, None, p.x, p.y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

pub fn ghost_end() {
    FLIGHT.with(|f| f.borrow_mut().take());
    drop_snap();
    unsafe {
        let _ = ShowWindow(GHOST.get(), SW_HIDE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(slots: usize) -> PanelData {
        PanelData { monitor: "M".into(), active: 0, titles: vec![vec![]; slots] }
    }
    fn center(r: RECT) -> POINT {
        POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
    }

    #[test]
    fn x_is_in_the_tiles_corner_when_there_are_two_slots() {
        let d = data(3);
        for i in 0..3 {
            assert_eq!(locate(&d, 1.0, center(close_rect(i, 1.0))), Some((i, true)));
            let t = tile_rect(i, 1.0);
            assert_eq!(locate(&d, 1.0, POINT { x: t.left + 5, y: t.bottom - 5 }), Some((i, false)));
        }
    }

    #[test]
    fn no_x_on_a_single_slot_or_on_new() {
        let one = data(1);
        assert_eq!(locate(&one, 1.0, center(close_rect(0, 1.0))), Some((0, false)));
        let two = data(2);
        assert_eq!(tiles(&two), 3);
        assert_eq!(locate(&two, 1.0, center(close_rect(2, 1.0))), Some((2, false)), "+ New");
    }

    #[test]
    fn x_stays_inside_its_tile_and_scales() {
        for s in [1.0_f32, 1.5, 2.5] {
            let (t, c) = (tile_rect(1, s), close_rect(1, s));
            assert!(c.left >= t.left && c.right <= t.right && c.top >= t.top && c.bottom <= t.bottom);
            assert!(c.right - c.left >= 20);
        }
    }
}
