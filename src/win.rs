//! Thin unsafe Win32 shell: gather windows, hide/show them, monitors, state file.

use crate::core::{self, Check, Entry, Layout, State, Tagged};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, TRUE, WPARAM};
use windows::core::BOOL;
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY, MONITOR_FROM_FLAGS, MONITORINFO,
    MONITORINFOEXW, MonitorFromPoint, MonitorFromRect, MonitorFromWindow,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::*;

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { if cfg!(debug_assertions) { eprintln!($($t)*) } };
}

const SHELL_CLASSES: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

pub fn hwnd(id: u64) -> HWND {
    HWND(id as *mut c_void)
}

/// Monotonic milliseconds since first use (wall-clock time can jump when Windows syncs it).
pub fn now_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

// ---------- monitors ----------

fn monitor_info(m: HMONITOR) -> MONITORINFOEXW {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    unsafe {
        let _ = GetMonitorInfoW(m, &mut info.monitorInfo as *mut MONITORINFO);
    }
    info
}

fn monitor_name(m: HMONITOR) -> String {
    let dev = monitor_info(m).szDevice;
    let len = dev.iter().position(|&c| c == 0).unwrap_or(dev.len());
    String::from_utf16_lossy(&dev[..len])
}

#[derive(Clone)]
pub struct MonitorInfo {
    pub name: String,
    pub rect: RECT,
    pub work: RECT,
    /// DPI scale, 1.0 = 96 dpi
    pub scale: f32,
}

pub fn monitors() -> Vec<MonitorInfo> {
    unsafe extern "system" fn cb(m: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        let (mut dx, mut dy) = (96, 96);
        unsafe {
            let _ = GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            (*(lp.0 as *mut Vec<MonitorInfo>)).push(MonitorInfo {
                name: monitor_name(m),
                rect: monitor_info(m).monitorInfo.rcMonitor,
                work: monitor_info(m).monitorInfo.rcWork,
                scale: dx as f32 / 96.0,
            });
        }
        TRUE
    }
    let mut v = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut v as *mut _ as isize));
    }
    v
}

pub fn monitor_names() -> Vec<String> {
    monitors().into_iter().map(|m| m.name).collect()
}

/// Name and full rect of the monitor containing a screen point.
pub fn monitor_at(pt: POINT) -> (String, RECT) {
    unsafe {
        let m = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        (monitor_name(m), monitor_info(m).monitorInfo.rcMonitor)
    }
}

/// Effective DPI of the primary monitor (where the tray is).
pub fn primary_dpi() -> u32 {
    let (mut x, mut y) = (96, 96);
    unsafe {
        let m = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        let _ = GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y);
    }
    x
}

/// Name and full rect of the monitor under the mouse cursor.
pub fn cursor_monitor() -> (String, RECT) {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    monitor_at(pt)
}

fn primary_work_area() -> RECT {
    unsafe { monitor_info(MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY)).monitorInfo.rcWork }
}

/// Monitor a window belongs to. Minimized windows live at -32000, so use their
/// restored position (workspace coordinates = relative to the primary work area).
fn monitor_of(h: HWND, flags: MONITOR_FROM_FLAGS) -> Option<HMONITOR> {
    unsafe {
        let m = if IsIconic(h).as_bool() {
            let p = placement(h)?;
            let wa = primary_work_area();
            let mut r = p.rcNormalPosition;
            r.left += wa.left;
            r.right += wa.left;
            r.top += wa.top;
            r.bottom += wa.top;
            MonitorFromRect(&r, flags)
        } else {
            MonitorFromWindow(h, flags)
        };
        (!m.is_invalid()).then_some(m)
    }
}

fn placement(h: HWND) -> Option<WINDOWPLACEMENT> {
    let mut p = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
    unsafe { GetWindowPlacement(h, &mut p).ok()? };
    Some(p)
}

// ---------- window identity & filter ----------

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

thread_local! {
    /// Icons looked up while the panel is open (misses too), by HWND.
    static ICONS: RefCell<HashMap<u64, HICON>> = RefCell::new(HashMap::new());
    /// When the current panel build started: asking apps for icons stops after the budget.
    static ICON_BUDGET: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
}

/// Total time one panel build may spend asking apps for icons (`WM_GETICON`).
const ICON_BUDGET_MS: u128 = 100;

/// Starts a panel build's icon budget; `forget` drops cached icons (a new panel opening).
pub fn icon_budget(forget: bool) {
    if forget {
        ICONS.with(|i| i.borrow_mut().clear());
    }
    ICON_BUDGET.set(Some(std::time::Instant::now()));
}

/// A window's small icon (invalid if none). Asks the app only within the build's budget and
/// never waits on a hung one; otherwise uses the window class's icon. Handles are shared:
/// never destroyed.
pub fn icon_of(id: u64) -> HICON {
    if let Some(i) = ICONS.with(|i| i.borrow().get(&id).copied()) {
        return i;
    }
    let h = hwnd(id);
    let in_budget = ICON_BUDGET.get().is_some_and(|t| t.elapsed().as_millis() < ICON_BUDGET_MS);
    let asked = |kind: u32| {
        let mut r = 0usize;
        let ok = unsafe {
            SendMessageTimeoutW(h, WM_GETICON, WPARAM(kind as usize), LPARAM(0), SMTO_ABORTIFHUNG | SMTO_BLOCK, 50, Some(&mut r))
        };
        (ok.0 != 0 && r != 0).then_some(HICON(r as *mut c_void))
    };
    let class = |index| {
        let v = unsafe { GetClassLongPtrW(h, index) };
        (v != 0).then_some(HICON(v as *mut c_void))
    };
    let icon = in_budget
        .then(|| asked(ICON_SMALL2).or_else(|| asked(ICON_SMALL)).or_else(|| asked(ICON_BIG)))
        .flatten()
        .or_else(|| class(GCLP_HICONSM))
        .or_else(|| class(GCLP_HICON))
        .unwrap_or_default();
    ICONS.with(|i| i.borrow_mut().insert(id, icon));
    icon
}

pub fn title_of(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn pid_of(h: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

pub fn is_cloaked(h: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe {
        DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut c_void, 4).is_ok()
            && cloaked != 0
    }
}

fn entry(h: HWND) -> Entry {
    Entry {
        id: h.0 as u64,
        pid: pid_of(h),
        iconic: unsafe { IsIconic(h).as_bool() },
        class: class_of(h),
    }
}

/// Why an unowned top-level window doesn't participate, or None if it does.
fn reject_root(h: HWND) -> Option<&'static str> {
    unsafe {
        if !IsWindowVisible(h).as_bool() {
            return Some("invisible");
        }
        if GetWindow(h, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
            return Some("owned");
        }
        if GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return Some("toolwindow");
        }
        if is_cloaked(h) {
            return Some("cloaked");
        }
        if pid_of(h) == GetCurrentProcessId() {
            return Some("ours");
        }
        if SHELL_CLASSES.contains(&class_of(h).as_str()) {
            return Some("shell");
        }
    }
    None
}

/// All top-level windows, topmost first.
fn top_level_windows() -> Vec<HWND> {
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> BOOL {
        unsafe { (*(lp.0 as *mut Vec<HWND>)).push(h) };
        TRUE
    }
    let mut v = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut v as *mut _ as isize));
    }
    v
}

/// Participating window and the monitor it maps to, or the reason it doesn't participate.
/// Owned windows (dialogs) participate when their root owner does, on the owner's monitor.
fn classify(h: HWND) -> Result<String, &'static str> {
    unsafe {
        if !IsWindowVisible(h).as_bool() {
            return Err("invisible");
        }
        let root = GetAncestor(h, GA_ROOTOWNER);
        if root != h {
            if is_cloaked(h) {
                return Err("owned+cloaked");
            }
            reject_root(root).map_or(Ok(()), |_| Err("owner not managed"))?;
        } else if let Some(why) = reject_root(h) {
            return Err(why);
        }
        monitor_of(root, MONITOR_DEFAULTTONEAREST).map(monitor_name).ok_or("no monitor")
    }
}

/// The window at the top of `id`'s owner chain (itself if it isn't owned).
pub fn root_of(id: u64) -> u64 {
    unsafe { GetAncestor(hwnd(id), GA_ROOTOWNER).0 as u64 }
}

/// True for a top-level window that isn't owned by another (not a dialog/popup).
pub fn is_root(id: u64) -> bool {
    unsafe { GetAncestor(hwnd(id), GA_ROOTOWNER) == hwnd(id) }
}

/// The managed window under a screen point: its monitor and its root + visible owned
/// windows, topmost first. None for the desktop, taskbar, our own windows, etc.
pub fn root_at(pt: POINT) -> Option<(String, Vec<Entry>)> {
    let root = unsafe { GetAncestor(WindowFromPoint(pt), GA_ROOTOWNER) };
    let monitor = classify(root).ok()?;
    let entries = top_level_windows()
        .into_iter()
        .filter(|&h| unsafe { GetAncestor(h, GA_ROOTOWNER) } == root && classify(h).is_ok())
        .map(entry)
        .collect();
    Some((monitor, entries))
}

/// Participating windows on `monitor`, topmost first.
pub fn visible_windows_on(monitor: &str) -> Vec<Entry> {
    top_level_windows()
        .into_iter()
        .filter(|&h| classify(h).is_ok_and(|m| m == monitor))
        .map(entry)
        .collect()
}

/// Debug aid: print the filter decision for every visible top-level window.
pub fn list() {
    for h in top_level_windows() {
        if !unsafe { IsWindowVisible(h).as_bool() } {
            continue;
        }
        let verdict = match classify(h) {
            Ok(m) => format!("ACCEPT {m}"),
            Err(why) => format!("reject {why}"),
        };
        println!("{:>10x} {:<28} {:<40} {}", h.0 as usize, verdict, class_of(h), title_of(h));
    }
}

// ---------- hide / show / focus ----------

pub fn hide(entries: &[Entry]) {
    for e in entries {
        let h = hwnd(e.id);
        unsafe {
            if IsHungAppWindow(h).as_bool() {
                log!("skip hung {:x} {}", e.id, e.class);
                continue;
            }
            let _ = ShowWindowAsync(h, SW_HIDE);
        }
    }
}

/// Shows every entry that still belongs to the same app. Returns those shown, topmost first.
pub fn show(entries: &[Entry]) -> Vec<Entry> {
    let mut shown = Vec::new();
    for e in entries {
        let h = hwnd(e.id);
        unsafe {
            if !IsWindow(Some(h)).as_bool() || !core::should_restore(e, pid_of(h), &class_of(h)) {
                log!("skip gone/reused {:x} {}", e.id, e.class);
                continue;
            }
            // SW_SHOWNA keeps a minimized window minimized (verified in spike 2.2)
            let _ = ShowWindowAsync(h, SW_SHOWNA);
        }
        bring_on_screen(h);
        shown.push(e.clone());
    }
    shown
}

/// A window whose monitor was removed would come back unreachable: move it to the primary.
fn bring_on_screen(h: HWND) {
    if monitor_of(h, MONITOR_DEFAULTTONULL).is_some() || unsafe { IsHungAppWindow(h).as_bool() } {
        return;
    }
    let Some(mut p) = placement(h) else { return };
    let wa = primary_work_area();
    let (w, ht) = (wa.right - wa.left, wa.bottom - wa.top);
    let r = &mut p.rcNormalPosition;
    let (rw, rh) = ((r.right - r.left).min(w), (r.bottom - r.top).min(ht));
    // workspace coordinates: (0,0) is the primary work area's top-left
    *r = RECT { left: 0, top: 0, right: rw, bottom: rh };
    set_placement_async(h, &mut p, false);
    log!("moved off-screen window {:x} to primary", h.0 as usize);
}

/// Applies a placement without activating the window (where Windows allows) and without
/// waiting on its thread, keeping it minimized/maximized.
fn set_placement_async(h: HWND, p: &mut WINDOWPLACEMENT, keep_hidden: bool) {
    unsafe {
        // SW_HIDE first: it updates the restored position and keeps the min/max state.
        p.showCmd = if keep_hidden {
            SW_HIDE.0 as u32
        } else if IsIconic(h).as_bool() {
            SW_SHOWMINNOACTIVE.0 as u32
        } else if IsZoomed(h).as_bool() {
            SW_SHOWMAXIMIZED.0 as u32 // activates briefly; accepted (design risks)
        } else {
            SW_SHOWNOACTIVATE.0 as u32
        };
        p.flags |= WPF_ASYNCWINDOWPLACEMENT;
        let _ = SetWindowPlacement(h, p);
    }
}

/// Move windows from one monitor to another, keeping their position relative to the work area.
/// Returns where the root window will appear (screen coordinates), for the soul to fly to.
/// Moves windows from monitor `from` to `to`, keeping their size and min/max state.
/// `keep_hidden`: they're hidden and must stay so (moved into a hidden slot).
pub fn relocate(entries: &[Entry], from: &str, to: &str, keep_hidden: bool) -> Option<RECT> {
    let mons = monitors();
    let work = |name: &str| mons.iter().find(|m| m.name == name).map(|m| m.work);
    let (Some(a), Some(b)) = (work(from), work(to)) else { return None };
    let pwa = primary_work_area();
    let mut root_rect = None;
    for e in entries {
        let h = hwnd(e.id);
        if unsafe { IsHungAppWindow(h).as_bool() } {
            continue;
        }
        let Some(mut p) = placement(h) else { continue };
        // rcNormalPosition is in workspace coords (relative to the primary work area)
        let r = p.rcNormalPosition;
        let (w, ht) = ((r.right - r.left).min(b.right - b.left), (r.bottom - r.top).min(b.bottom - b.top));
        let left = (r.left + b.left - a.left).clamp(b.left - pwa.left, b.right - pwa.left - w);
        let top = (r.top + b.top - a.top).clamp(b.top - pwa.top, b.bottom - pwa.top - ht);
        p.rcNormalPosition = RECT { left, top, right: left + w, bottom: top + ht };
        let zoomed = unsafe { IsZoomed(h).as_bool() };
        if is_root(e.id) {
            root_rect = Some(if zoomed {
                b
            } else {
                RECT { left: left + pwa.left, top: top + pwa.top, right: left + w + pwa.left, bottom: top + ht + pwa.top }
            });
        }
        set_placement_async(h, &mut p, keep_hidden);
        if keep_hidden && zoomed {
            // A hidden maximized window keeps its old maximized rect until shown: move it
            // too, keeping its overhang past the work area (the resize borders).
            if let Some(wr) = window_rect(e.id) {
                let r = RECT { left: b.left + wr.left - a.left, top: b.top + wr.top - a.top, right: b.right + wr.right - a.right, bottom: b.bottom + wr.bottom - a.bottom };
                unsafe {
                    let _ = SetWindowPos(h, None, r.left, r.top, r.right - r.left, r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
                }
            }
        }
    }
    root_rect
}

pub fn window_rect(id: u64) -> Option<RECT> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd(id), &mut r) }.ok().map(|_| r)
}

/// Where a window is on screen (visible frame, without the invisible resize borders);
/// for a minimized window, where it would be restored. None if it's gone.
pub fn frame_rect(id: u64) -> Option<RECT> {
    let h = hwnd(id);
    unsafe {
        if !IsWindow(Some(h)).as_bool() {
            return None;
        }
        if IsIconic(h).as_bool() {
            let r = placement(h)?.rcNormalPosition; // workspace coordinates
            let wa = primary_work_area();
            return Some(RECT { left: r.left + wa.left, top: r.top + wa.top, right: r.right + wa.left, bottom: r.bottom + wa.top });
        }
        let mut r = RECT::default();
        // Meaningless for a hidden window: fall back to its window rect.
        if !IsWindowVisible(h).as_bool()
            || DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as *mut c_void, size_of::<RECT>() as u32).is_err()
        {
            GetWindowRect(h, &mut r).ok()?;
        }
        Some(r)
    }
}

pub fn is_visible(id: u64) -> bool {
    unsafe { IsWindowVisible(hwnd(id)).as_bool() }
}

/// Gives a specific window keyboard focus, restoring it first if it's minimized.
pub fn focus(id: u64) {
    let h = hwnd(id);
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
    }
    set_foreground(h);
}

/// Keystrokes must never go to a hidden window.
pub fn focus_after_switch(shown: &[Entry], just_hidden: &[Entry]) {
    let usable = |h: HWND| unsafe { !is_cloaked(h) && !IsIconic(h).as_bool() };
    let target = shown
        .iter()
        .map(|e| hwnd(e.id))
        .find(|&h| usable(h))
        .or_else(|| {
            top_level_windows().into_iter().find(|&h| {
                classify(h).is_ok() && usable(h) && !just_hidden.iter().any(|e| e.id == h.0 as u64)
            })
        })
        .unwrap_or_else(|| unsafe { GetShellWindow() });
    set_foreground(target);
}

fn set_foreground(target: HWND) {
    unsafe {
        if !SetForegroundWindow(target).as_bool() {
            // Hook-driven switches may lack the foreground right; any input event from our
            // process grants it. 0xE8 is unassigned, so no app reacts to it.
            let key = |flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0xE8), dwFlags: flags, ..Default::default() } },
            };
            SendInput(&[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)], size_of::<INPUT>() as i32);
            let _ = SetForegroundWindow(target);
        }
    }
}

/// Is `id` the foreground window or one of its owned windows?
pub fn has_focus(entries: &[Entry]) -> bool {
    let fg = unsafe { GetForegroundWindow() }.0 as u64;
    entries.iter().any(|e| e.id == fg)
}

// ---------- state file ----------

/// Running as the installed MSIX package (has package identity), not a dev build.
pub fn packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut len = 0;
        // fails with APPMODEL_ERROR_NO_PACKAGE without identity, else "buffer too small"
        unsafe { GetCurrentPackageFullName(&mut len, None) }.0 != 15700
    })
}

/// The package gets its own folder: Windows only redirects *new* AppData files into the
/// package's private storage, so sharing a pre-existing dev folder would split the file.
pub fn state_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join(if packaged() { "screen-shifter-pkg" } else { "screen-shifter" })
}

fn state_path() -> PathBuf {
    state_dir().join("hidden.txt")
}


/// When Windows started, in ms since the Unix epoch (fixed for the life of the process).
/// Saved window handles mean nothing after a restart.
pub fn boot_time() -> u64 {
    static BOOT: OnceLock<u64> = OnceLock::new();
    *BOOT.get_or_init(|| {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        now.saturating_sub(unsafe { GetTickCount64() })
    })
}

/// Atomic: write a temp file, then rename over the real one. `extra` = windows about to be shown.
pub fn write_state(state: &State, extra: &[Tagged]) {
    let path = state_path();
    let tmp = path.with_extension("txt.tmp");
    let result = std::fs::create_dir_all(path.parent().unwrap())
        .and_then(|_| std::fs::write(&tmp, state.serialize(boot_time(), extra)))
        .and_then(|_| std::fs::rename(&tmp, &path));
    if let Err(e) = result {
        log!("state file write failed: {e}");
    }
}

/// Forget the saved layout (Restore all, monitor set changed).
pub fn clear_state() {
    write_state(&State::default(), &[]);
}

pub fn read_layout() -> Layout {
    let text = std::fs::read_to_string(state_path()).unwrap_or_default();
    let (layout, bad) = core::parse(&text);
    for line in bad {
        log!("state file: skipped {line:?}");
    }
    layout
}

/// Show everything the state file lists, keeping the file so the next start can take
/// the windows back. Safe to call repeatedly; used by the panic hook and failed starts.
pub fn show_from_file() {
    let entries: Vec<Entry> = read_layout().windows.into_iter().map(|(_, _, e)| e).collect();
    if !entries.is_empty() {
        show(&entries);
    }
}

/// Startup: is a saved window still there, still the same app, and (judged by its root
/// owner, so dialogs go with their app) on the monitor it was saved on?
pub fn check(monitor: &str, e: &Entry) -> Check {
    let h = hwnd(e.id);
    unsafe {
        if !IsWindow(Some(h)).as_bool() || !core::should_restore(e, pid_of(h), &class_of(h)) {
            return Check::Gone;
        }
        let live = entry(h);
        let root = GetAncestor(h, GA_ROOTOWNER);
        match monitor_of(root, MONITOR_DEFAULTTONULL).map(monitor_name) {
            Some(m) if m == monitor => Check::Here(live),
            _ => Check::Elsewhere(live),
        }
    }
}
