// Release builds have no console; debug builds keep one for `log!` output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
#[macro_use]
mod win;
mod anim;
mod hooks;
mod panel;

use crate::core::{Dir, Entry, Plan, SLOTS};
use std::sync::atomic::Ordering::SeqCst;
use std::thread::JoinHandle;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::DwmFlush;
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
const PANEL_TIMER: usize = 2;
const PANEL_DELAY_MS: u32 = 250;
const TILT_GAP_MS: u64 = 400;
const WHEEL_NOTCH: i32 = 120;
/// Backstop for the animation hard limit; WM_TIMER still arrives inside modal loops.
const ANIM_TIMER: usize = 3;
const ANIM_TIMER_MS: u32 = 250;

thread_local! {
    static STATE: RefCell<core::State> = RefCell::default();
    static OSD: Cell<HWND> = Cell::new(HWND::default());
    static OSD_SLOT: Cell<usize> = const { Cell::new(1) };
    static HOOKS: RefCell<Option<JoinHandle<()>>> = const { RefCell::new(None) };
    static WHEEL: Cell<i32> = const { Cell::new(0) };
    /// (time of last tilt event, direction) for one-switch-per-tilt bursts
    static TILT: Cell<(u64, i32)> = const { Cell::new((0, 0)) };
    /// Panel hit at left-down (for click-to-jump): Some(None) = panel gap
    static PRESSED: RefCell<Option<Option<(String, usize)>>> = const { RefCell::new(None) };
    /// Window being dragged: its monitor and root + owned entries
    static DRAGGING: RefCell<Option<(String, Vec<Entry>)>> = const { RefCell::new(None) };
    /// A modal loop (tray menu) is running: our frame loop can't step animations.
    static MODAL: Cell<bool> = const { Cell::new(false) };
    static APP: Cell<HWND> = Cell::new(HWND::default());
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
        HOOKS.with(|h| *h.borrow_mut() = Some(hooks::start(app)));
        log!("running");

        message_loop();
    }
}

/// Blocks in GetMessage when idle; while anything animates, drains messages, steps one
/// frame, and paces itself to the display with DwmFlush. Never nested inside a wndproc.
fn message_loop() {
    let mut msg = MSG::default();
    unsafe {
        loop {
            if anim::active() || panel::step(win::now_ms()) {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let now = win::now_ms();
                anim::step(now);
                panel::step(now);
                let t = std::time::Instant::now();
                let _ = DwmFlush();
                if t.elapsed() < std::time::Duration::from_millis(1) {
                    std::thread::sleep(std::time::Duration::from_millis(1)); // nothing composed: don't spin
                }
                anim::frame_composed();
            } else {
                if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    return;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
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
        anim::exclude_from_capture(osd);
        OSD.set(osd);
        APP.set(app);
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

fn on_switch(monitor: String, dir: Dir) {
    run_switch(monitor, |s, m, visible, now| s.switch(m, dir, visible, now));
}

/// Hook-driven switches pick the monitor from the posted point, not a fresh cursor read.
fn switch_at(pt: POINT, dir: Dir) {
    on_switch(win::monitor_at(pt).0, dir);
}

fn jump(monitor: String, slot: usize) {
    run_switch(monitor, |s, m, visible, now| s.switch_to(m, slot, visible, now));
}

fn motion_on() -> bool {
    !MODAL.get() && anim::enabled()
}

/// Keeps panels, OSD, and ghost above a sheet that was just created.
fn raise_overlays() {
    let mut overlays = panel::overlay_hwnds();
    let osd = OSD.get();
    if unsafe { IsWindowVisible(osd).as_bool() } {
        overlays.push(osd);
    }
    for h in overlays {
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// Shared switch path: write-ahead, hide, show, persist, focus, then panel or OSD.
fn run_switch(monitor: String, pick: impl FnOnce(&mut core::State, &str, Vec<Entry>, u64) -> Plan) {
    if hooks::DRAG.load(SeqCst) {
        return; // never hide the window under a drag
    }
    let visible = win::visible_windows_on(&monitor);
    // Copy everything out so no borrow is held across Win32 calls that can re-enter.
    let (old, plan, new, before, after) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let before = s.active(&monitor);
        let old = s.hidden();
        let plan = pick(&mut s, &monitor, visible, win::now_ms());
        (old, plan, s.hidden(), before, s.active(&monitor))
    });
    if before == after {
        return; // clamped
    }
    log!("{monitor}: slot {before} -> {after}, hide {}, show {}", plan.hide.len(), plan.show.len());
    let info = win::monitors().into_iter().find(|m| m.name == monitor);
    let rect = info.as_ref().map_or(RECT::default(), |m| m.rect);
    if motion_on()
        && let Some(m) = &info
    {
        let slide = if after > before { anim::Slide::Left } else { anim::Slide::Right };
        if anim::start_sheet(&monitor, m.rect, slide, m.scale, win::now_ms()) {
            raise_overlays();
            unsafe {
                // the sheet must be on screen before anything underneath changes
                let _ = DwmFlush();
                SetTimer(Some(APP.get()), ANIM_TIMER, ANIM_TIMER_MS, None);
            }
        }
    }
    win::write_state(&core::write_ahead_set(&old, &new));
    win::hide(&plan.hide);
    let shown = win::show(&plan.show);
    win::write_state(&new);
    win::focus_after_switch(&shown, &plan.hide);
    if panel::is_shown() {
        panel::update(panel_data());
    } else {
        show_osd(after, rect);
    }
}

enum Moved {
    Nothing,
    Hidden,
    /// Visible on another monitor, at this rect if known.
    Relocated(Option<RECT>),
}

/// Move a dragged window into a slot (design §7): write-ahead, relocate, hide, persist.
fn do_move(entries: Vec<Entry>, from: String, to: String, slot: usize) -> Moved {
    let (old, hide, new) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let old = s.hidden();
        let hide = s.move_to_slot(&entries, &from, &to, slot, win::now_ms());
        (old, hide, s.hidden())
    });
    if from == to && !hide {
        return Moved::Nothing; // dropped on its own monitor's current slot
    }
    log!("move {} window(s) {from} -> {to} slot {}", entries.len(), slot + 1);
    let focused = win::has_focus(&entries);
    win::write_state(&core::write_ahead_set(&old, &new));
    let new_rect = if from != to { win::relocate(&entries, &from, &to) } else { None };
    if hide {
        win::hide(&entries);
    }
    win::write_state(&new);
    if hide && focused {
        win::focus_after_switch(&[], &entries);
    }
    panel::update(panel_data());
    if hide { Moved::Hidden } else { Moved::Relocated(new_rect) }
}

/// The ghost finishes its trip: back over the window, or nowhere if motion is off.
fn soul_home(entries: &[Entry]) {
    let root = entries.iter().find(|e| win::is_root(e.id)).or(entries.first());
    match root.and_then(|e| win::window_rect(e.id)) {
        Some(r) if motion_on() => panel::ghost_fly(r),
        _ => panel::ghost_end(),
    }
}

/// Titles per slot for every monitor. Owned dialogs are left out of the lists.
fn panel_data() -> Vec<panel::PanelData> {
    let titles = |entries: &[Entry]| -> Vec<String> {
        entries
            .iter()
            .filter(|e| win::is_root(e.id))
            .map(|e| win::title_of(win::hwnd(e.id)))
            .filter(|t| !t.is_empty())
            .collect()
    };
    win::monitor_names()
        .into_iter()
        .map(|monitor| {
            let (active, slots) = STATE.with(|s| s.borrow().slots(&monitor));
            let visible = win::visible_windows_on(&monitor);
            let titles = (0..SLOTS)
                .map(|i| if i == active { titles(&visible) } else { titles(&slots[i]) })
                .collect();
            panel::PanelData { monitor, active, titles }
        })
        .collect()
}

fn show_panels() {
    panel::show(&win::monitors(), panel_data());
}

fn cancel_drag() {
    if let Some((_, entries)) = DRAGGING.with(|d| d.borrow_mut().take()) {
        hooks::DRAG.store(false, SeqCst);
        panel::set_drop(None);
        soul_home(&entries);
    }
}

fn on_wheel(delta: i32, pt: POINT) {
    if hooks::DRAG.load(SeqCst) {
        return;
    }
    let mut acc = WHEEL.get() + delta; // up (positive) = next
    while acc >= WHEEL_NOTCH {
        switch_at(pt, Dir::Next);
        acc -= WHEEL_NOTCH;
    }
    while acc <= -WHEEL_NOTCH {
        switch_at(pt, Dir::Prev);
        acc += WHEEL_NOTCH;
    }
    WHEEL.set(acc);
}

/// A held tilt repeats; switch once per burst (right = next).
fn on_tilt(delta: i32, pt: POINT) {
    let dir = delta.signum();
    let now = win::now_ms();
    let (last, last_dir) = TILT.replace((now, dir));
    if dir != 0 && (dir != last_dir || now.saturating_sub(last) > TILT_GAP_MS) && !hooks::DRAG.load(SeqCst) {
        switch_at(pt, if dir > 0 { Dir::Next } else { Dir::Prev });
    }
}

fn on_ldown(pt: POINT) {
    if hooks::DRAG.load(SeqCst) {
        return;
    }
    if panel::is_shown()
        && let Some(hit) = panel::hit(pt)
    {
        PRESSED.with(|p| *p.borrow_mut() = Some(hit));
        return;
    }
    let Some((monitor, entries)) = win::root_at(pt) else { return };
    let root = entries.iter().find(|e| win::is_root(e.id)).unwrap_or(&entries[0]).id;
    log!("drag start {:x} on {monitor}", root);
    hooks::DRAG.store(true, SeqCst);
    DRAGGING.with(|d| *d.borrow_mut() = Some((monitor, entries)));
    anim::clear_all(); // PrintWindow below can be slow; never leave a sheet frozen mid-slide
    panel::ghost_start(win::hwnd(root), pt);
}

fn on_lup(pt: POINT) {
    if let Some((from, entries)) = DRAGGING.with(|d| d.borrow_mut().take()) {
        hooks::DRAG.store(false, SeqCst);
        panel::set_drop(None);
        let Some(Some((to, slot))) = panel::hit(pt) else {
            soul_home(&entries);
            return;
        };
        let tile = panel::tile_screen_rect(&to, slot);
        match do_move(entries.clone(), from, to.clone(), slot) {
            Moved::Hidden if motion_on() => {
                match tile {
                    // fly into the tile, shrinking to a sliver inside it
                    Some(t) => panel::ghost_fly(RECT { left: t.left + 20, top: t.top + 20, right: t.right - 20, bottom: t.bottom - 20 }),
                    None => panel::ghost_end(),
                }
                panel::pulse(&to, slot);
            }
            Moved::Relocated(Some(r)) if motion_on() => panel::ghost_fly(r),
            Moved::Nothing => soul_home(&entries),
            _ => panel::ghost_end(),
        }
        return;
    }
    if let Some(Some(pressed)) = PRESSED.with(|p| p.borrow_mut().take())
        && panel::hit(pt) == Some(Some(pressed.clone()))
    {
        jump(pressed.0, pressed.1);
    }
}

fn on_move() {
    hooks::MOVE_PENDING.store(false, SeqCst);
    let pt = hooks::unpack(hooks::MOVE_PT.load(SeqCst));
    panel::ghost_move(pt);
    panel::set_drop(panel::hit(pt).flatten());
}

fn restore_all() {
    anim::clear_all();
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
                HK_PREV => on_switch(win::cursor_monitor().0, Dir::Prev),
                HK_NEXT => on_switch(win::cursor_monitor().0, Dir::Next),
                HK_PANIC => panic!("debug panic drill"),
                _ => {}
            },
            WM_TRAY if matches!(lp.0 as u32, WM_RBUTTONUP | WM_CONTEXTMENU) => tray_menu(h),
            hooks::WM_CHORD_DOWN => {
                SetTimer(Some(h), PANEL_TIMER, PANEL_DELAY_MS, None);
            }
            WM_TIMER if wp.0 == ANIM_TIMER => {
                anim::enforce_limit(win::now_ms());
                if !anim::active() {
                    let _ = KillTimer(Some(h), ANIM_TIMER);
                }
            }
            WM_TIMER if wp.0 == PANEL_TIMER => {
                let _ = KillTimer(Some(h), PANEL_TIMER);
                if hooks::CHORD.load(SeqCst) {
                    show_panels();
                }
            }
            hooks::WM_CHORD_UP => {
                let _ = KillTimer(Some(h), PANEL_TIMER);
                cancel_drag();
                panel::hide();
                WHEEL.set(0);
                PRESSED.with(|p| p.borrow_mut().take());
            }
            hooks::WM_HOOK_WHEEL => on_wheel(wp.0 as isize as i32, hooks::unpack(lp.0 as u64)),
            hooks::WM_HOOK_HWHEEL => on_tilt(wp.0 as isize as i32, hooks::unpack(lp.0 as u64)),
            hooks::WM_HOOK_LDOWN => on_ldown(hooks::unpack(lp.0 as u64)),
            hooks::WM_HOOK_LUP => on_lup(hooks::unpack(lp.0 as u64)),
            hooks::WM_HOOK_MOVE => on_move(),
            WM_QUERYENDSESSION => {
                restore_all(); // so "save changes?" prompts are visible
                return LRESULT(1);
            }
            WM_DISPLAYCHANGE => {
                anim::clear_all();
                let reset = STATE.with(|s| s.borrow_mut().reset_if_monitors_changed(&win::monitor_names()));
                if let Some(entries) = reset {
                    log!("monitor set changed: restoring {}", entries.len());
                    win::show(&entries);
                    win::write_state(&[]);
                }
                if panel::is_shown() {
                    show_panels(); // rebuild for the new layout
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
        // The menu runs its own modal loop, so our frame loop can't advance sheets meanwhile.
        anim::clear_all();
        MODAL.set(true);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, None, app, None);
        MODAL.set(false);
        let _ = DestroyMenu(menu);
        match cmd.0 {
            CMD_RESTORE => restore_all(),
            CMD_EXIT => {
                restore_all();
                cancel_drag();
                panel::hide();
                if let Some(h) = HOOKS.with(|h| h.borrow_mut().take()) {
                    hooks::stop(h);
                }
                let _ = UnregisterHotKey(Some(app), HK_PREV);
                let _ = UnregisterHotKey(Some(app), HK_NEXT);
                tray(app, NIM_DELETE);
                let _ = DestroyWindow(app);
            }
            _ => {}
        }
    }
}
