//! Window snapshots for the exposé. Hidden windows aren't drawn by DWM, so their previews
//! are pictures taken while they were still visible: when the panel appears, a background
//! thread asks each visible window to draw itself (`PrintWindow`). Each shot comes in two
//! sizes: a downscaled one for the grid (crisp when small) and the full-resolution one, so
//! a chosen preview lands on its window without a blurry moment. They go straight to the
//! GPU (`expose::snapshots_arrived`); nothing is kept on the CPU or written to disk.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::{Receiver, Sender, channel};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Posted to the app window when snapshots are waiting in the channel.
pub const WM_SNAPSHOT: u32 = WM_APP + 20;
/// Longer side of the grid-size picture.
const GRID_SIDE: i32 = 800;
/// Full-resolution pictures beyond this are skipped (the grid one is still used).
const FULL_SIDE: i32 = 4096;

pub struct Pic {
    pub w: i32,
    pub h: i32,
    pub pixels: Vec<u32>,
}

pub struct Shot {
    batch: u64,
    pub id: u64,
    pub grid: Pic,
    pub full: Option<Pic>,
}
/// (batch, windows) to the worker; pictures back.
type Worker = (Sender<(u64, Vec<u64>)>, Receiver<Shot>);

static BUSY: AtomicBool = AtomicBool::new(false);

thread_local! {
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
    static BATCH: Cell<u64> = const { Cell::new(0) };
    /// Batch that was current when a window was hidden by a switch: a shot of it from that
    /// batch (or older) may show it already hidden (black), so it's dropped.
    static HIDDEN_AT: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
}

/// Snapshots `windows` (visible, not minimized) in the background. Skipped while the previous
/// batch is still running. `app` receives `WM_SNAPSHOT` as pictures arrive.
pub fn start_batch(app: HWND, windows: Vec<u64>) {
    if windows.is_empty() || BUSY.swap(true, SeqCst) {
        return;
    }
    let batch = BATCH.get() + 1;
    BATCH.set(batch);
    HIDDEN_AT.with(|h| h.borrow_mut().retain(|&id, _| unsafe { IsWindow(Some(crate::win::hwnd(id))).as_bool() }));
    WORKER.with(|w| {
        let mut w = w.borrow_mut();
        let (to_worker, _) = w.get_or_insert_with(|| spawn(app));
        if to_worker.send((batch, windows)).is_err() {
            BUSY.store(false, SeqCst);
        }
    });
}

fn spawn(app: HWND) -> Worker {
    let (to_worker, jobs) = channel::<(u64, Vec<u64>)>();
    let (results, from_worker) = channel::<Shot>();
    let app = app.0 as isize;
    std::thread::spawn(move || {
        for (batch, windows) in jobs {
            let t = std::time::Instant::now();
            let mut n = 0;
            for id in windows {
                if let Some((grid, full)) = capture(crate::win::hwnd(id)) {
                    n += 1;
                    if results.send(Shot { batch, id, grid, full }).is_err() {
                        return;
                    }
                    unsafe {
                        let _ = PostMessageW(Some(HWND(app as *mut _)), WM_SNAPSHOT, WPARAM(0), LPARAM(0));
                    }
                }
            }
            crate::log!("snapshots: {n} in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
            BUSY.store(false, SeqCst);
        }
    });
    (to_worker, from_worker)
}

/// Windows hidden by a switch: shots of them from the current batch are unreliable.
pub fn hidden(ids: impl IntoIterator<Item = u64>) {
    let batch = BATCH.get();
    HIDDEN_AT.with(|h| h.borrow_mut().extend(ids.into_iter().map(|id| (id, batch))));
}

/// Pictures that arrived, minus any of a window hidden after its batch started.
pub fn drain() -> Vec<Shot> {
    let shots: Vec<Shot> = WORKER.with(|w| w.borrow().as_ref().map(|(_, rx)| rx.try_iter().collect()).unwrap_or_default());
    shots
        .into_iter()
        .filter(|s| !HIDDEN_AT.with(|hd| hd.borrow().get(&s.id).is_some_and(|&at| at >= s.batch)))
        .collect()
}

fn usable(h: HWND) -> bool {
    unsafe { IsWindow(Some(h)).as_bool() && IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool() && !crate::win::is_cloaked(h) }
}

/// The window's visible frame, drawn by itself: grid-size and full resolution. None if it
/// can't be (hung, hidden, minimized) or was hidden while drawing.
fn capture(h: HWND) -> Option<(Pic, Option<Pic>)> {
    unsafe {
        if !usable(h) || IsHungAppWindow(h).as_bool() {
            return None;
        }
        let mut wr = RECT::default();
        GetWindowRect(h, &mut wr).ok()?;
        let mut fr = RECT::default();
        if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut fr as *mut _ as *mut _, size_of::<RECT>() as u32).is_err() {
            fr = wr;
        }
        let (ww, wh) = (wr.right - wr.left, wr.bottom - wr.top);
        let (fw, fh) = (fr.right - fr.left, fr.bottom - fr.top);
        let (ox, oy) = (fr.left - wr.left, fr.top - wr.top);
        if ww <= 0 || wh <= 0 || fw <= 0 || fh <= 0 || ox < 0 || oy < 0 || ox + fw > ww || oy + fh > wh {
            return None;
        }
        let screen = GetDC(None);
        let dib = |w: i32, h: i32| {
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            CreateDIBSection(Some(screen), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok().map(|b| (b, bits as *const u32))
        };
        let mut out = None;
        let full_dc = CreateCompatibleDC(Some(screen));
        let small_dc = CreateCompatibleDC(Some(screen));
        let k = (GRID_SIDE as f32 / fw.max(fh) as f32).min(1.0);
        let (sw, sh) = (((fw as f32 * k) as i32).max(1), ((fh as f32 * k) as i32).max(1));
        if let (Some((full, full_bits)), Some((small, small_bits))) = (dib(ww, wh), dib(sw, sh)) {
            let old_full = SelectObject(full_dc, full.into());
            let old_small = SelectObject(small_dc, small.into());
            let printed = PrintWindow(h, full_dc, PRINT_WINDOW_FLAGS(2 /* PW_RENDERFULLCONTENT */)).as_bool();
            SetStretchBltMode(small_dc, HALFTONE);
            let _ = SetBrushOrgEx(small_dc, 0, 0, None::<*mut POINT>);
            let ok = StretchBlt(small_dc, 0, 0, sw, sh, Some(full_dc), ox, oy, fw, fh, SRCCOPY).as_bool();
            // A switch can hide it mid-print; a hidden window prints black.
            if printed && ok && usable(h) {
                let opaque = |p: &u32| p | 0xFF00_0000; // GDI leaves alpha undefined
                let grid = Pic { w: sw, h: sh, pixels: std::slice::from_raw_parts(small_bits, (sw * sh) as usize).iter().map(opaque).collect() };
                let full_px = std::slice::from_raw_parts(full_bits, (ww * wh) as usize);
                let full = (fw.max(fh) <= FULL_SIDE).then(|| Pic {
                    w: fw,
                    h: fh,
                    pixels: (0..fh).flat_map(|y| {
                        let row = ((oy + y) * ww + ox) as usize;
                        full_px[row..row + fw as usize].iter().map(opaque)
                    }).collect(),
                });
                out = Some((grid, full));
            }
            SelectObject(small_dc, old_small);
            SelectObject(full_dc, old_full);
            let _ = DeleteObject(small.into());
            let _ = DeleteObject(full.into());
        }
        let _ = DeleteDC(small_dc);
        let _ = DeleteDC(full_dc);
        ReleaseDC(None, screen);
        out
    }
}
