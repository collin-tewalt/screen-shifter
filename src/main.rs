// Release builds have no console; debug builds keep one for `log!` output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
#[macro_use]
mod win;

use crate::core::{Dir, SLOTS};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const HK_PREV: i32 = 1;
const HK_NEXT: i32 = 2;
const HK_PANIC: i32 = 3; // debug builds only: failure drill for task 6.3
const WM_TRAY: u32 = WM_APP + 1;
const CMD_RESTORE: i32 = 1;
const CMD_EXIT: i32 = 2;
const OSD_TIMER: usize = 1;
const OSD_MS: u32 = 600;
const OSD_SIZE: (i32, i32) = (220, 110);

thread_local! {
    static STATE: RefCell<core::State> = RefCell::default();
    static OSD: Cell<HWND> = Cell::new(HWND::default());
    static OSD_SLOT: Cell<usize> = const { Cell::new(1) };
}
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

fn main() {
    if std::env::args().any(|a| a == "--list") {
        win::list();
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Held for the life of the process; a second instance must not touch anything.
        let _mutex = CreateMutexW(None, true, w!("Local\\screen-shifter"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        // The state file is written before anything is hidden, so it is always the
        // authoritative recovery source. Never touch STATE here: it may be borrowed.
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            win::restore_from_file();
            default_hook(info);
        }));

        win::restore_from_file();
        STATE.with(|s| s.borrow_mut().reset_if_monitors_changed(&win::monitor_names()));

        let app = create_windows();
        if let Err(msg) = register_hotkeys(app) {
            MessageBoxW(None, &windows::core::HSTRING::from(msg), w!("screen-shifter"), MB_ICONERROR);
            return;
        }
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        tray(app, NIM_ADD);
        log!("running");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The app window must be a normal top-level window (never shown), not HWND_MESSAGE:
/// message-only windows miss broadcasts like WM_QUERYENDSESSION and TaskbarCreated.
unsafe fn create_windows() -> HWND {
    unsafe {
        let inst = GetModuleHandleW(None).unwrap().into();
        let make = |name: PCWSTR, proc_: WNDPROC, ex: WINDOW_EX_STYLE, style: WINDOW_STYLE| {
            RegisterClassW(&WNDCLASSW { lpfnWndProc: proc_, hInstance: inst, lpszClassName: name, ..Default::default() });
            CreateWindowExW(ex, name, name, style, 0, 0, 0, 0, None, None, Some(inst), None).unwrap()
        };
        let app = make(w!("ScreenShifter"), Some(app_proc), WINDOW_EX_STYLE(0), WS_OVERLAPPED);
        let osd = make(
            w!("ScreenShifterOsd"),
            Some(osd_proc),
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            WS_POPUP,
        );
        let _ = SetLayeredWindowAttributes(osd, Default::default(), 220, LWA_ALPHA);
        OSD.set(osd);
        app
    }
}

unsafe fn register_hotkeys(app: HWND) -> Result<(), String> {
    let mods = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
    let keys = [(HK_PREV, VK_LEFT, "Ctrl+Alt+Left"), (HK_NEXT, VK_RIGHT, "Ctrl+Alt+Right")];
    for (id, vk, name) in keys {
        unsafe { RegisterHotKey(Some(app), id, mods, vk.0 as u32) }
            .map_err(|_| format!("{name} is already in use by another application."))?;
    }
    if cfg!(debug_assertions) {
        let _ = unsafe { RegisterHotKey(Some(app), HK_PANIC, mods | MOD_SHIFT, 'P' as u32) };
    }
    Ok(())
}

unsafe fn tray(app: HWND, op: NOTIFY_ICON_MESSAGE) {
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: app,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() },
        ..Default::default()
    };
    let tip: Vec<u16> = "screen-shifter (Ctrl+Alt+\u{2190}/\u{2192})".encode_utf16().collect();
    nid.szTip[..tip.len()].copy_from_slice(&tip);
    let _ = unsafe { Shell_NotifyIconW(op, &nid) };
}

fn on_switch(dir: Dir) {
    let (monitor, rect) = win::cursor_monitor();
    let visible = win::visible_windows_on(&monitor);
    // Copy everything out so no borrow is held across Win32 calls that can re-enter.
    let (old, plan, new, before, after) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let before = s.active(&monitor);
        let old = s.hidden();
        let plan = s.switch(&monitor, dir, visible, win::now_ms());
        (old, plan, s.hidden(), before, s.active(&monitor))
    });
    if before == after {
        return; // clamped
    }
    log!("{monitor}: slot {before} -> {after}, hide {}, show {}", plan.hide.len(), plan.show.len());
    win::write_state(&core::write_ahead_set(&old, &new));
    win::hide(&plan.hide);
    let shown = win::show(&plan.show);
    win::write_state(&new);
    win::focus_after_switch(&shown, &plan.hide);
    show_osd(after, rect);
}

fn restore_all() {
    let entries = STATE.with(|s| s.borrow_mut().restore_all());
    win::show(&entries);
    win::write_state(&[]);
}

fn show_osd(slot: usize, monitor: RECT) {
    OSD_SLOT.set(slot);
    let osd = OSD.get();
    let (w, h) = OSD_SIZE;
    let x = (monitor.left + monitor.right - w) / 2;
    let y = (monitor.top + monitor.bottom - h) / 2;
    unsafe {
        let _ = SetWindowPos(osd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        let _ = InvalidateRect(Some(osd), None, true);
        SetTimer(Some(osd), OSD_TIMER, OSD_MS, None);
    }
}

unsafe extern "system" fn osd_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_TIMER && wp.0 == OSD_TIMER {
            let _ = KillTimer(Some(h), OSD_TIMER);
            let _ = ShowWindow(h, SW_HIDE);
            return LRESULT(0);
        }
        if msg == WM_PAINT {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let bg = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x202020));
            FillRect(dc, &rc, bg);
            let _ = DeleteObject(bg.into());
            let font = CreateFontW(56, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0, DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"));
            let old = SelectObject(dc, font.into());
            SetTextColor(dc, windows::Win32::Foundation::COLORREF(0xFFFFFF));
            SetBkMode(dc, TRANSPARENT);
            let mut text: Vec<u16> = format!("{} / {SLOTS}", OSD_SLOT.get()).encode_utf16().collect();
            DrawTextW(dc, &mut text, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            SelectObject(dc, old);
            let _ = DeleteObject(font.into());
            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }
        DefWindowProcW(h, msg, wp, lp)
    }
}

unsafe extern "system" fn app_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_HOTKEY => match wp.0 as i32 {
                HK_PREV => on_switch(Dir::Prev),
                HK_NEXT => on_switch(Dir::Next),
                HK_PANIC => panic!("debug panic drill"),
                _ => {}
            },
            WM_TRAY if matches!(lp.0 as u32, WM_RBUTTONUP | WM_CONTEXTMENU) => tray_menu(h),
            WM_QUERYENDSESSION => {
                restore_all(); // so "save changes?" prompts are visible
                return LRESULT(1);
            }
            WM_DISPLAYCHANGE => {
                let reset = STATE.with(|s| s.borrow_mut().reset_if_monitors_changed(&win::monitor_names()));
                if let Some(entries) = reset {
                    log!("monitor set changed: restoring {}", entries.len());
                    win::show(&entries);
                    win::write_state(&[]);
                }
            }
            WM_DESTROY => PostQuitMessage(0),
            m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => tray(h, NIM_ADD),
            _ => return DefWindowProcW(h, msg, wp, lp),
        }
        LRESULT(0)
    }
}

unsafe fn tray_menu(app: HWND) {
    unsafe {
        let menu = CreatePopupMenu().unwrap();
        let _ = AppendMenuW(menu, MF_STRING, CMD_RESTORE as usize, w!("Restore all windows"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, w!("Exit"));
        let mut pt = Default::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(app); // required for the menu to close on outside click
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, None, app, None);
        let _ = DestroyMenu(menu);
        match cmd.0 {
            CMD_RESTORE => restore_all(),
            CMD_EXIT => {
                restore_all();
                let _ = UnregisterHotKey(Some(app), HK_PREV);
                let _ = UnregisterHotKey(Some(app), HK_NEXT);
                tray(app, NIM_DELETE);
                let _ = DestroyWindow(app);
            }
            _ => {}
        }
    }
}
