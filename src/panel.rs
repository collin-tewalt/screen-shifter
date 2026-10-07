//! Overlay windows: a slot panel per monitor and the drag ghost.
//! Display only: they never take focus or clicks. All input arrives through the hooks.

use crate::core::SLOTS;
use crate::win::MonitorInfo;
use std::cell::{Cell, RefCell};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

/// What a panel shows: per slot, the titles of its windows.
pub struct PanelData {
    pub monitor: String,
    pub active: usize,
    pub titles: Vec<Vec<String>>,
}

struct Panel {
    data: PanelData,
    hwnd: HWND,
    rect: RECT, // screen coordinates
    scale: f32,
}

thread_local! {
    static PANELS: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
    static DROP: RefCell<Option<(String, usize)>> = const { RefCell::new(None) };
    static GHOST: Cell<HWND> = Cell::new(HWND::default());
}

const TILE: (i32, i32) = (220, 150);
const GAP: i32 = 12;
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
        let s = m.scale;
        let (w, h) = (px(SLOTS as i32 * (TILE.0 + GAP) + GAP, s), px(TILE.1 + 2 * GAP, s));
        let left = (m.work.left + m.work.right - w) / 2;
        let top = m.work.bottom - h - px(48, s);
        let rect = RECT { left, top, right: left + w, bottom: top + h };
        let hwnd = overlay(cls, rect);
        unsafe {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA);
        }
        panels.push(Panel { data: d, hwnd, rect, scale: s });
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
}

pub fn is_shown() -> bool {
    PANELS.with(|p| !p.borrow().is_empty())
}

pub fn update(mut data: Vec<PanelData>) {
    PANELS.with(|p| {
        for panel in p.borrow_mut().iter_mut() {
            if let Some(i) = data.iter().position(|d| d.monitor == panel.data.monitor) {
                panel.data = data.swap_remove(i);
                unsafe {
                    let _ = InvalidateRect(Some(panel.hwnd), None, false);
                }
            }
        }
    });
}

/// None: not over a panel. Some(None): over a panel but between tiles. Some(Some(tile)).
pub fn hit(pt: POINT) -> Option<Option<(String, usize)>> {
    PANELS.with(|p| {
        let p = p.borrow();
        let panel = p.iter().find(|p| unsafe { PtInRect(&p.rect, pt).as_bool() })?;
        let local = POINT { x: pt.x - panel.rect.left, y: pt.y - panel.rect.top };
        let tile = (0..SLOTS).find(|&i| unsafe { PtInRect(&tile_rect(i, panel.scale), local).as_bool() });
        Some(tile.map(|i| (panel.data.monitor.clone(), i)))
    })
}

pub fn set_drop(target: Option<(String, usize)>) {
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
            for i in 0..SLOTS {
                let r = tile_rect(i, s);
                let is_drop = drop.as_ref().is_some_and(|(m, t)| *m == panel.data.monitor && *t == i);
                if i == panel.data.active {
                    fill(dc, &r, ACCENT);
                }
                let inner = if i == panel.data.active {
                    RECT { left: r.left + px(3, s), top: r.top + px(3, s), right: r.right - px(3, s), bottom: r.bottom - px(3, s) }
                } else {
                    r
                };
                fill(dc, &inner, if is_drop { DROP_BG } else { TILE_BG });
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
                for (n, t) in titles.iter().enumerate() {
                    let mut lr = RECT { left: r.left + pad, top: y, right: r.right - pad, bottom: y + line };
                    if n == MAX_TITLES - 1 && titles.len() > MAX_TITLES {
                        text(dc, &format!("+{} more", titles.len() - n), &mut lr, DIM, DT_LEFT);
                        break;
                    }
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
        DefWindowProcW(h, msg, wp, lp)
    }
}

// ---------- drag ghost ----------

unsafe extern "system" fn ghost_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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
        if GHOST.get().is_invalid() {
            GHOST.set(overlay(class(w!("ScreenShifterGhost"), Some(ghost_proc)), RECT::default()));
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

        SelectObject(small_dc, old_small);
        SelectObject(full_dc, old_full);
        let _ = DeleteObject(small.into());
        let _ = DeleteObject(full.into());
        let _ = DeleteDC(small_dc);
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
    unsafe {
        let _ = ShowWindow(GHOST.get(), SW_HIDE);
    }
}
