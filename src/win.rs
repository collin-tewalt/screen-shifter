//! Thin unsafe Win32 shell: gather windows, hide/show them, monitors, state file.

use crate::core::{self, Entry};
use std::ffi::c_void;
use std::path::PathBuf;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, TRUE};
use windows::core::BOOL;
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY, MONITOR_FROM_FLAGS, MONITORINFO,
    MONITORINFOEXW, MonitorFromPoint, MonitorFromRect, MonitorFromWindow,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::*;

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { if cfg!(debug_assertions) { eprintln!($($t)*) } };
}

const SHELL_CLASSES: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

pub fn hwnd(id: u64) -> HWND {
    HWND(id as *mut c_void)
}

pub fn now_ms() -> u64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0, |d| d.as_millis() as u64)
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

pub fn monitor_names() -> Vec<String> {
    unsafe extern "system" fn cb(m: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        unsafe { (*(lp.0 as *mut Vec<String>)).push(monitor_name(m)) };
        TRUE
    }
    let mut names = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut names as *mut _ as isize));
    }
    names
}

/// Name and full rect of the monitor under the mouse cursor.
pub fn cursor_monitor() -> (String, RECT) {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
        let m = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        (monitor_name(m), monitor_info(m).monitorInfo.rcMonitor)
    }
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

fn title_of(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn pid_of(h: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

fn is_cloaked(h: HWND) -> bool {
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
    unsafe {
        p.showCmd = if IsIconic(h).as_bool() {
            SW_SHOWMINNOACTIVE.0 as u32
        } else if IsZoomed(h).as_bool() {
            SW_SHOWMAXIMIZED.0 as u32
        } else {
            SW_SHOWNOACTIVATE.0 as u32
        };
        p.flags |= WPF_ASYNCWINDOWPLACEMENT;
        let _ = SetWindowPlacement(h, &p);
    }
    log!("moved off-screen window {:x} to primary", h.0 as usize);
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
    unsafe {
        let _ = SetForegroundWindow(target);
    }
}

// ---------- state file ----------

fn state_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("screen-shifter").join("hidden.txt")
}

/// Atomic: write a temp file, then rename over the real one.
pub fn write_state(entries: &[Entry]) {
    let path = state_path();
    let tmp = path.with_extension("txt.tmp");
    let result = std::fs::create_dir_all(path.parent().unwrap())
        .and_then(|_| std::fs::write(&tmp, core::serialize(entries)))
        .and_then(|_| std::fs::rename(&tmp, &path));
    if let Err(e) = result {
        log!("state file write failed: {e}");
    }
}

pub fn read_state() -> Vec<Entry> {
    let text = std::fs::read_to_string(state_path()).unwrap_or_default();
    let (entries, bad) = core::parse(&text);
    for line in bad {
        log!("state file: skipped {line:?}");
    }
    entries
}

/// Show everything the state file lists, then clear it. Safe to call repeatedly.
pub fn restore_from_file() {
    let entries = read_state();
    if !entries.is_empty() {
        show(&entries);
    }
    write_state(&[]);
}
