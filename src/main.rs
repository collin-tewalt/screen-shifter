// Release builds have no console; debug builds keep one for `log!` output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
#[macro_use]
mod win;
mod anim;
mod expose;
mod gpu;
mod snap;
mod icons;
mod hooks;
mod panel;
mod touchpad;

use crate::core::{Dir, Entry, MoveResult, Plan, Tagged, Target};
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
use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::System::Recovery::{RESTART_NO_CRASH, RESTART_NO_HANG, RESTART_NO_REBOOT, RegisterApplicationRestart};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetSystemMetricsForDpi, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const HK_PREV: i32 = 1;
const HK_NEXT: i32 = 2;
const HK_PANIC: i32 = 3; // debug builds only: failure drill for task 6.3
const WM_TRAY: u32 = WM_APP + 1;
/// Posted once startup is done: the one-time questions (touchpad, start with Windows).
const WM_ASK: u32 = WM_APP + 2;
const CMD_RESTORE: i32 = 1;
const CMD_EXIT: i32 = 2;
const CMD_STARTUP: i32 = 3;
const CMD_TOUCHPAD: i32 = 4;
/// Must match the manifest's `desktop:StartupTask TaskId`.
const STARTUP_TASK: &str = "ScreenShifter";
/// Tray icon sizes we ship (`tools/make_icons.py`).
const TRAY_ICONS: [(i32, &[u8]); 4] = [
    (16, include_bytes!("../assets/tray-16.png")),
    (20, include_bytes!("../assets/tray-20.png")),
    (24, include_bytes!("../assets/tray-24.png")),
    (32, include_bytes!("../assets/tray-32.png")),
];
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
/// Resting on a tile this long opens its exposé.
const HOVER_TIMER: usize = 4;
/// Longest a swipe-up exposé waits for the panel's pictures before it opens with placeholders.
const EXPOSE_TIMER: usize = 5;
const EXPOSE_WAIT_MS: u32 = 300;
const HOVER_MS: u32 = 300;
/// Watches windows asked to close from the exposé.
const CLOSE_TIMER: usize = 6;
const CLOSE_POLL_MS: u32 = 250;
/// A window that hasn't closed by now gets its preview back (it keeps being watched).
const CLOSE_UNDIM_MS: u64 = 3000;
const CLOSE_GIVE_UP_MS: u64 = 60_000;

thread_local! {
    static STATE: RefCell<core::State> = RefCell::default();
    static OSD: Cell<HWND> = Cell::new(HWND::default());
    /// (slot, count) shown by the indicator
    static OSD_SLOT: Cell<(usize, usize)> = const { Cell::new((1, 1)) };
    static HOOKS: RefCell<Option<JoinHandle<()>>> = const { RefCell::new(None) };
    static WHEEL: Cell<i32> = const { Cell::new(0) };
    /// (time of last tilt event, direction) for one-switch-per-tilt bursts
    static TILT: Cell<(u64, i32)> = const { Cell::new((0, 0)) };
    /// Panel hit at left-down (for click-to-jump): Some(None) = panel gap
    static PRESSED: RefCell<Option<Option<(String, Target)>>> = const { RefCell::new(None) };
    /// The x pressed on a slot tile (acts if the release is on the same x).
    static PRESSED_CLOSE: RefCell<Option<(String, usize)>> = const { RefCell::new(None) };
    /// Where the cursor was when a slot was closed with the chord held: the panel shrinks
    /// under it, so the release must not switch to whatever tile is there now (until the
    /// mouse has moved away).
    static CLOSED_BY_X: Cell<Option<POINT>> = const { Cell::new(None) };
    /// The preview whose x was pressed (acts if the release is on the same x).
    static PRESSED_CLOSE_WINDOW: Cell<Option<u64>> = const { Cell::new(None) };
    /// Windows asked to close from the exposé, and not yet settled.
    static CLOSING: RefCell<Vec<Closing>> = const { RefCell::new(Vec::new()) };
    /// Window being dragged: its monitor and root + owned entries
    static DRAGGING: RefCell<Option<(String, Vec<Entry>)>> = const { RefCell::new(None) };
    /// A modal loop (tray menu, a question) is running: our frame loop can't step animations.
    static MODAL: Cell<bool> = const { Cell::new(false) };
    /// A one-time question is up: the tray icon brings it forward instead of opening the menu.
    static ASKING: Cell<bool> = const { Cell::new(false) };
    /// The questions arrived during another modal loop: asked once that loop ends.
    static ASK_PENDING: Cell<bool> = const { Cell::new(false) };
    /// `shutdown` has begun: a question that returns now is ignored.
    static SHUTTING_DOWN: Cell<bool> = const { Cell::new(false) };
    static APP: Cell<HWND> = Cell::new(HWND::default());
    /// The tile the mouse is resting on, waiting for HOVER_TIMER.
    static HOVER_TILE: RefCell<Option<(String, usize)>> = const { RefCell::new(None) };
    /// Exposé preview under a left-down, and where (chosen if the up lands on it too,
    /// dragged if the mouse moves first).
    static PRESSED_PREVIEW: Cell<Option<(u64, POINT)>> = const { Cell::new(None) };
    /// The screen when the current three-finger touch began.
    static SWIPE_START: Cell<Option<touchpad::Phase>> = const { Cell::new(None) };
    /// A swipe-up exposé is waiting for the panel's pictures (see `open_all_spreads`).
    static EXPOSE_WAIT: Cell<bool> = const { Cell::new(false) };
    /// A preview is being dragged (we set hooks::DRAG for it).
    static PREVIEW_DRAG: Cell<bool> = const { Cell::new(false) };
    /// The tile a preview was just dropped on: hovering it (or the other monitor) doesn't
    /// swap or collapse the spread until the mouse is back on the spread's monitor.
    static DROP_HOLD: RefCell<Option<(String, Target)>> = const { RefCell::new(None) };
    /// Each monitor's visible windows as of the panel opening (updated by switches and drops):
    /// the exposé of a current slot, without re-enumerating on every hover.
    static VISIBLE: RefCell<std::collections::HashMap<String, Vec<Entry>>> = RefCell::new(Default::default());
    /// Tray icons by size, created once.
    static ICONS: Cell<[Option<HICON>; 4]> = const { Cell::new([None; 4]) };
}
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// A window asked to close.
struct Closing {
    entry: Entry,
    started: u64,
    /// its preview already came back to normal
    undimmed: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Poll {
    Wait,
    Undim,
    Gone,
    Drop,
}

/// What to do about a window asked to close: `same` = it is still the very window.
fn poll_close(same: bool, undimmed: bool, age_ms: u64) -> Poll {
    if !same {
        Poll::Gone
    } else if age_ms >= CLOSE_GIVE_UP_MS {
        Poll::Drop
    } else if !undimmed && age_ms >= CLOSE_UNDIM_MS {
        Poll::Undim
    } else {
        Poll::Wait
    }
}

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
        // The file is kept: the next start takes the windows back into their slots.
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            win::show_from_file();
            touchpad::restore();
            default_hook(info);
        }));

        let app = create_windows();
        if let Err(msg) = register_hotkeys(app) {
            win::show_from_file(); // never leave saved windows hidden with no app running
            MessageBoxW(None, &windows::core::HSTRING::from(msg), w!("Screen Shifter"), MB_ICONERROR);
            return;
        }
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        tray(app, NIM_ADD);
        HOOKS.with(|h| *h.borrow_mut() = Some(hooks::start(app)));
        // Only now that nothing can fail: take back the saved layout.
        adopt_saved_layout();
        touchpad::register(app);
        touchpad::startup(); // applies Windows' three-finger setting if the owner said yes before
        let _ = PostMessageW(Some(app), WM_ASK, WPARAM(0), LPARAM(0)); // the one-time questions
        // If Windows closes us to update the package, it may relaunch us afterwards. Never
        // after a crash (a startup crash would loop) or a reboot (the startup task covers that).
        let _ = RegisterApplicationRestart(None, RESTART_NO_CRASH | RESTART_NO_HANG | RESTART_NO_REBOOT);
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

/// Our icon at the tray's size for the primary monitor's scaling (created once per size).
fn tray_icon() -> HICON {
    let want = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, win::primary_dpi()) };
    let i = TRAY_ICONS.iter().position(|(s, _)| *s >= want).unwrap_or(TRAY_ICONS.len() - 1);
    let mut icons = ICONS.get();
    let icon = *icons[i].get_or_insert_with(|| {
        let (size, png) = TRAY_ICONS[i];
        unsafe { CreateIconFromResourceEx(png, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR) }
            .or_else(|_| unsafe { LoadIconW(None, IDI_APPLICATION) })
            .unwrap_or_default()
    });
    ICONS.set(icons);
    icon
}

unsafe fn tray(app: HWND, op: NOTIFY_ICON_MESSAGE) {
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: app,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: tray_icon(),
        ..Default::default()
    };
    let tip: Vec<u16> = "Screen Shifter (Ctrl+Alt+\u{2190}/\u{2192})".encode_utf16().collect();
    nid.szTip[..tip.len()].copy_from_slice(&tip);
    let _ = unsafe { Shell_NotifyIconW(op, &nid) };
}

/// Startup (design §6): take back windows saved in hidden slots, show the rest. Done like
/// a switch: write-ahead (everything to show or hide is recorded), act, then the exact state.
fn adopt_saved_layout() {
    let names = win::monitor_names();
    let (state, show) = core::adopt(&win::read_layout(), win::boot_time(), &names, win::check);
    let adopted = state.hidden();
    win::write_state(&state, &show);
    let show: Vec<Entry> = show.into_iter().map(|(_, _, e)| e).collect();
    win::show(&show);
    let focused = win::has_focus(&adopted);
    win::hide(&adopted);
    if focused {
        win::focus_after_switch(&[], &adopted);
    }
    win::write_state(&state, &[]);
    log!("startup: took back {}, showed {}", adopted.len(), show.len());
    STATE.with(|s| *s.borrow_mut() = state);
}

/// Saves the current state; `extra` = windows about to be shown (write-ahead).
fn persist(extra: &[Tagged]) {
    STATE.with(|s| win::write_state(&s.borrow(), extra));
}

fn on_switch(monitor: String, dir: Dir) {
    run_switch(monitor, dir, |s, m, visible, now| s.switch(m, dir, visible, now));
}

fn arm_anim_timer() {
    unsafe {
        SetTimer(Some(APP.get()), ANIM_TIMER, ANIM_TIMER_MS, None);
    }
}

fn stop_hover() {
    HOVER_TILE.with(|t| t.borrow_mut().take());
    unsafe {
        let _ = KillTimer(Some(APP.get()), HOVER_TIMER);
    }
}

/// Spreads slot `i` of `monitor` (its current slot: the windows on screen).
fn open_expose(monitor: String, i: usize) {
    let (active, slots) = STATE.with(|s| s.borrow().slots(&monitor));
    let current = i == active;
    let entries: Vec<Entry> = if current {
        VISIBLE.with(|v| v.borrow().get(&monitor).cloned().unwrap_or_default())
    } else {
        slots.get(i).cloned().unwrap_or_default()
    };
    let roots: Vec<Entry> = entries.into_iter().filter(|e| win::is_root(e.id)).collect();
    let tile = panel::tile_screen_rect(&monitor, i);
    if !hooks::PANEL_LATCH.load(SeqCst) {
        expose::collapse(motion_on()); // the Ctrl+Alt exposé has one spread at a time
    }
    if expose::open(&monitor, i, current, &roots, tile, motion_on()) {
        arm_anim_timer();
    }
}

/// Swipe up with the panel up: every monitor spreads its current slot at once. A fast swipe
/// gets here before the panel's pictures are taken, and placeholders swapped in mid-animation
/// look wrong, so this waits for the batch (up to EXPOSE_WAIT_MS) before spreading.
fn open_all_spreads() {
    if snap::busy() {
        if !EXPOSE_WAIT.replace(true) {
            unsafe {
                SetTimer(Some(APP.get()), EXPOSE_TIMER, EXPOSE_WAIT_MS, None);
            }
        }
        return;
    }
    spread_all_now();
}

fn spread_all_now() {
    cancel_expose_wait();
    for m in win::monitor_names() {
        let (active, _) = STATE.with(|s| s.borrow().slots(&m));
        open_expose(m, active);
    }
}

/// Spreads one monitor's current slot (the others stay as they are).
fn spread_monitor(monitor: String) {
    let (active, _) = STATE.with(|s| s.borrow().slots(&monitor));
    open_expose(monitor, active);
}

/// Drops a pending swipe-up exposé (the swipe reversed, or the panel closed).
fn cancel_expose_wait() {
    if EXPOSE_WAIT.replace(false) {
        unsafe {
            let _ = KillTimer(Some(APP.get()), EXPOSE_TIMER);
        }
    }
}

/// Mouse moved with the panel up (not dragging): open, swap, highlight, or collapse.
fn on_hover(pt: POINT) {
    panel::set_close_hover(pt);
    // On a preview's x: that x is hot and the preview isn't highlighted, so a release here chooses nothing.
    let on_badge = expose::close_at(pt);
    expose::set_close_hot(on_badge);
    if on_badge.is_some() {
        expose::set_hover(None);
        return;
    }
    let latched = hooks::PANEL_LATCH.load(SeqCst);
    let open = expose::open_slot();
    let hold = DROP_HOLD.with(|h| h.borrow().clone());
    if hold.is_some() && panel::hit(pt).flatten() == hold {
        return; // still on the tile we just dropped on: the spread stays as it is
    }
    match panel::hit(pt) {
        Some(Some((m, Target::Slot(i)))) => {
            // In the swipe-opened exposé each monitor has its own spread, so only this one counts.
            let here = expose::open_slot_on(&m);
            if here == Some(i) {
                expose::set_hover(None);
            } else if if latched { here.is_some() } else { open.is_some() } {
                stop_hover(); // already spreading: follow the mouse at once
                DROP_HOLD.with(|h| h.borrow_mut().take());
                open_expose(m, i);
            } else if HOVER_TILE.with(|t| t.borrow().as_ref() != Some(&(m.clone(), i))) {
                HOVER_TILE.with(|t| *t.borrow_mut() = Some((m, i)));
                unsafe {
                    SetTimer(Some(APP.get()), HOVER_TIMER, HOVER_MS, None);
                }
            }
            return;
        }
        _ => stop_hover(),
    }
    if latched {
        // Every monitor's spread stays up as the mouse moves between them.
        DROP_HOLD.with(|h| h.borrow_mut().take());
        expose::set_hover(expose::preview_at(pt));
        return;
    }
    if let Some((m, _)) = open {
        if win::monitor_at(pt).0 != m {
            if hold.is_some() {
                return; // coming back from a drop on the other monitor
            }
            expose::collapse(motion_on());
            arm_anim_timer();
        } else {
            DROP_HOLD.with(|h| h.borrow_mut().take()); // back on the spread's monitor
            expose::set_hover(expose::preview_at(pt));
        }
    }
}

/// Ends a preview drag: the drop target decides where the window goes.
fn drop_preview(pt: POINT) {
    PREVIEW_DRAG.set(false);
    hooks::DRAG.store(false, SeqCst);
    panel::set_drop(None);
    let motion = motion_on();
    let (Some((monitor, slot, current, e)), Some(Some((to, target)))) = (expose::drag_info(), panel::tile_hit(pt)) else {
        expose::drag_end(expose::Land::Back, motion);
        arm_anim_timer();
        return;
    };
    // the window and its owned windows, from the list the spread was built from
    let source = if current {
        VISIBLE.with(|v| v.borrow().get(&monitor).cloned().unwrap_or_default())
    } else {
        STATE.with(|s| s.borrow().slots(&monitor).1.get(slot).cloned().unwrap_or_default())
    };
    let group: Vec<Entry> = source.into_iter().filter(|g| win::root_of(g.id) == e.id).collect();
    let group = if group.is_empty() { vec![e.clone()] } else { group };
    let land = match do_move(group, monitor, (!current).then_some(slot), to.clone(), target) {
        Moved::Hidden(s) => {
            panel::pulse(&to, s);
            panel::tile_screen_rect(&to, s).map(|t| RECT { left: t.left + 20, top: t.top + 20, right: t.right - 20, bottom: t.bottom - 20 })
        }
        Moved::Relocated(r) => r.or_else(|| win::frame_rect(e.id)),
        Moved::Nothing => None,
    };
    match land {
        Some(r) => {
            expose::drag_end(expose::Land::Into(r), motion);
            DROP_HOLD.with(|h| *h.borrow_mut() = Some((to, target)));
            if expose::live() == 0 {
                expose::collapse(motion);
            }
        }
        None => expose::drag_end(expose::Land::Back, motion),
    }
    arm_anim_timer();
}

/// A preview drag that can't finish normally (chord released, display change): back it goes.
fn cancel_preview_drag() {
    if PREVIEW_DRAG.replace(false) {
        hooks::DRAG.store(false, SeqCst);
        panel::set_drop(None);
        expose::drag_end(expose::Land::Back, motion_on());
        arm_anim_timer();
    }
}

/// The user picked exposé preview `i`: bring its slot and window forward.
fn choose(id: u64) {
    let Some((monitor, slot, current, e)) = expose::chosen(id) else { return };
    if current && !win::is_visible(e.id) {
        expose::collapse(motion_on()); // the list was stale: it's no longer on screen
        arm_anim_timer();
        return;
    }
    log!("expose: choose {:x} in {monitor} slot {}", e.id, slot + 1);
    expose::choose(id, win::frame_rect(e.id), motion_on());
    arm_anim_timer();
    if current {
        win::focus(e.id);
        return;
    }
    let active = STATE.with(|s| s.borrow().active(&monitor)) - 1;
    let dir = if slot < active { Dir::Prev } else { Dir::Next };
    switch_with(monitor, dir, Some(e.id), |s, m, visible, now| s.switch_to(m, Target::Slot(slot), visible, now));
}

/// Hook-driven switches pick the monitor from the posted point, not a fresh cursor read.
fn switch_at(pt: POINT, dir: Dir) {
    on_switch(win::monitor_at(pt).0, dir);
}

/// The x on a preview: ask the window's app to close it, as its own close button does. The
/// preview dims and is inert until the window is gone (then it fades away) or stays (3 s).
fn request_close(id: u64) {
    let Some((_, _, _, entry)) = expose::chosen(id) else { return };
    if CLOSING.with(|c| c.borrow().iter().any(|x| x.entry.id == id)) {
        return;
    }
    if !win::is_same(&entry) {
        // already gone (or the handle was reused): never send a request to whatever has it now
        expose::fade_out(id, motion_on());
        reap_dead();
        collapse_if_empty();
        return;
    }
    let posted = unsafe { PostMessageW(Some(win::hwnd(id)), WM_SYSCOMMAND, WPARAM(SC_CLOSE as usize), LPARAM(0)) };
    if posted.is_err() {
        log!("close {:x}: request refused", id);
        return;
    }
    log!("close {:x}: asked", id);
    expose::dim_preview(id, true);
    CLOSING.with(|c| c.borrow_mut().push(Closing { entry, started: win::now_ms(), undimmed: false }));
    unsafe {
        SetTimer(Some(APP.get()), CLOSE_TIMER, CLOSE_POLL_MS, None);
    }
}

/// An exposé with nothing left in it folds away.
fn collapse_if_empty() {
    if panel::is_shown() && expose::open_slot().is_some() && expose::live() == 0 {
        expose::collapse(motion_on());
        arm_anim_timer();
    }
}

fn on_close_timer() {
    if hooks::DRAG.load(SeqCst) {
        return; // never rebuild the panel or spread under a drag: the next tick looks again
    }
    let now = win::now_ms();
    let (mut undim, mut gone) = (vec![], vec![]);
    CLOSING.with(|c| {
        c.borrow_mut().retain_mut(|x| match poll_close(win::is_same(&x.entry), x.undimmed, now.saturating_sub(x.started)) {
            Poll::Wait => true,
            Poll::Undim => {
                x.undimmed = true;
                undim.push(x.entry.id);
                true
            }
            Poll::Gone => {
                gone.push(x.entry.id);
                false
            }
            Poll::Drop => false,
        });
        if c.borrow().is_empty() {
            unsafe {
                let _ = KillTimer(Some(APP.get()), CLOSE_TIMER);
            }
        }
    });
    for id in undim {
        expose::dim_preview(id, false);
    }
    if !gone.is_empty() {
        for id in gone {
            expose::fade_out(id, motion_on());
        }
        reap_dead();
        collapse_if_empty();
    }
    arm_anim_timer();
}

/// Forgets windows that no longer exist: from the hidden slots (and so the state file), from
/// the visible lists, and from the panel. A slot that empties goes, and a spread that named the
/// renumbered slots folds. Only windows that are gone are ever removed.
fn reap_dead() {
    if hooks::DRAG.load(SeqCst) {
        return;
    }
    let alive = |id: u64| unsafe { IsWindow(Some(win::hwnd(id))).as_bool() };
    let names = win::monitor_names();
    let counts = |s: &core::State| -> Vec<usize> { names.iter().map(|n| s.count(n)).collect() };
    let (removed, before, after) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let before = counts(&s);
        let removed = s.forget_dead(alive);
        (removed, before, counts(&s))
    });
    let trimmed = VISIBLE.with(|v| {
        let mut changed = false;
        for list in v.borrow_mut().values_mut() {
            let n = list.len();
            list.retain(|e| alive(e.id));
            changed |= list.len() != n;
        }
        changed
    });
    if !removed && !trimmed {
        return;
    }
    if removed {
        persist(&[]); // removing windows that are gone can't strand any
    }
    let renumbered: Vec<&String> = names.iter().zip(before.iter().zip(&after)).filter(|(_, (b, a))| b != a).map(|(n, _)| n).collect();
    if !renumbered.is_empty() {
        stop_hover();
        DROP_HOLD.with(|h| h.borrow_mut().take());
        PRESSED.with(|p| p.borrow_mut().take());
        PRESSED_CLOSE.with(|p| p.borrow_mut().take());
        for m in renumbered {
            expose::collapse_monitor(m, motion_on()); // its spread names slots that renumbered
        }
        arm_anim_timer();
    }
    if panel::is_shown() {
        let data = names
            .iter()
            .map(|n| {
                let visible = VISIBLE.with(|v| v.borrow().get(n).cloned().unwrap_or_default());
                panel_data_for(n.clone(), &visible)
            })
            .collect();
        panel::update(data);
    }
}

/// The x on a slot tile: the slot folds into its neighbor on the left (the right for the first).
/// Nothing is hidden, so no sheet and no refocus: windows that join the current slot just
/// appear. Write-ahead as for a switch, since windows are about to be shown.
fn close_slot_ui(monitor: String, i: usize) {
    if hooks::DRAG.load(SeqCst) {
        return;
    }
    stop_hover();
    DROP_HOLD.with(|h| h.borrow_mut().take());
    expose::collapse_monitor(&monitor, motion_on()); // its spread names slots that are about to renumber
    arm_anim_timer();
    let result = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let old = s.hidden();
        let closed = s.close_slot(&monitor, i, win::now_ms())?;
        let extra = s.tag_active(&monitor, &core::leaving(&old, &s.hidden()));
        Some((closed, extra))
    });
    let Some((closed, extra)) = result else { return };
    log!("{monitor}: closed slot {}, showing {}", i + 1, closed.plan.show.len());
    persist(&extra);
    let shown = win::show(&closed.plan.show);
    gpu::switched(&monitor);
    let visible = VISIBLE.with(|v| {
        let mut v = v.borrow_mut();
        let list = v.entry(monitor.clone()).or_default();
        list.extend(shown.iter().filter(|e| !list.iter().any(|x| x.id == e.id)).cloned().collect::<Vec<_>>());
        list.clone()
    });
    persist(&[]);
    // best effort: windows that were hidden when the panel opened have no picture yet
    snap::start_batch(APP.get(), shown.iter().filter(|e| !e.iconic && win::is_root(e.id)).map(|e| e.id).collect());
    panel::update(vec![panel_data_for(monitor.clone(), &visible)]);
    panel::pulse(&monitor, closed.receiver);
    arm_anim_timer();
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    CLOSED_BY_X.set(Some(pt));
}

/// Releasing Ctrl+Alt over another slot's tile switches to it, as a click does.
/// "+ New" and the monitor's current slot change nothing.
fn release_on_tile() {
    if CLOSED_BY_X.take().is_some() {
        return; // the panel moved under a still cursor after a close
    }
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    let Some(Some((monitor, Target::Slot(slot)))) = panel::tile_hit(pt) else { return };
    let current = STATE.with(|s| s.borrow().active(&monitor)) - 1;
    if slot != current {
        jump(monitor, Target::Slot(slot));
    }
}

fn jump(monitor: String, target: Target) {
    let active = STATE.with(|s| s.borrow().active(&monitor)) - 1;
    // Slot numbers can't tell the direction once empty slots are removed; the request can.
    let dir = match target {
        Target::Slot(i) if i < active => Dir::Prev,
        _ => Dir::Next,
    };
    run_switch(monitor, dir, |s, m, visible, now| s.switch_to(m, target, visible, now));
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
/// `dir` is the direction the user moved, for the slide.
fn run_switch(monitor: String, dir: Dir, pick: impl FnOnce(&mut core::State, &str, Vec<Entry>, u64) -> Option<Plan>) {
    switch_with(monitor, dir, None, pick);
}

/// `chosen`: a window picked in the exposé, whose own motion replaces the sheet and which
/// gets focus instead of the topmost shown window.
fn switch_with(monitor: String, dir: Dir, chosen: Option<u64>, pick: impl FnOnce(&mut core::State, &str, Vec<Entry>, u64) -> Option<Plan>) {
    if hooks::DRAG.load(SeqCst) {
        return; // never hide the window under a drag
    }
    // In swipe mode a switch keeps the exposé open on the switched monitor, spread for its new slot.
    let swipe = chosen.is_none() && hooks::PANEL_LATCH.load(SeqCst);
    let reopen = (swipe && expose::open_slot().is_some()).then(|| monitor.clone());
    if chosen.is_none() {
        // never leave a spread sitting under a sheet: in swipe mode only this monitor's
        stop_hover();
        if swipe {
            expose::collapse_monitor(&monitor, false);
        } else {
            expose::clear_all();
        }
    }
    let visible = win::visible_windows_on(&monitor);
    let names = win::monitor_names();
    let others = |s: &core::State| -> Vec<usize> { names.iter().filter(|n| **n != monitor).map(|n| s.count(n)).collect() };
    // Copy everything out so no borrow is held across Win32 calls that can re-enter.
    let result = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let before = others(&s);
        let old = s.hidden();
        let plan = pick(&mut s, &monitor, visible, win::now_ms())?;
        let extra = s.tag_active(&monitor, &core::leaving(&old, &s.hidden()));
        Some((plan, extra, s.active(&monitor), s.count(&monitor), others(&s) != before))
    });
    let Some((plan, extra, after, count, others_changed)) = result else {
        if let Some(m) = reopen {
            spread_monitor(m);
        }
        return; // clamped, current slot, or stale
    };
    log!("{monitor}: {dir:?} to slot {after} / {count}, hide {}, show {}", plan.hide.len(), plan.show.len());
    let info = win::monitors().into_iter().find(|m| m.name == monitor);
    let rect = info.as_ref().map_or(RECT::default(), |m| m.rect);
    let t_sheet = std::time::Instant::now();
    if chosen.is_none()
        && motion_on()
        && let Some(m) = &info
    {
        let slide = if dir == Dir::Next { anim::Slide::Left } else { anim::Slide::Right };
        if anim::start_sheet(&monitor, m.rect, slide, m.scale, win::now_ms()) {
            raise_overlays();
            gpu::wait_committed(); // a GPU sheet's commit is asynchronous
            unsafe {
                // the sheet must be on screen before anything underneath changes
                let _ = DwmFlush();
                SetTimer(Some(APP.get()), ANIM_TIMER, ANIM_TIMER_MS, None);
            }
            log!("  sheet up (capture + flush) {:.1} ms", t_sheet.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let t = std::time::Instant::now();
    let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;
    persist(&extra);
    let t_write = ms(t);
    win::hide(&plan.hide);
    let shown = win::show(&plan.show);
    let t_hideshow = ms(t);
    gpu::switched(&monitor);
    gpu::go(); // the compositor slides the sheet from here; nothing below can stall it
    snap::hidden(plan.hide.iter().map(|e| e.id));
    VISIBLE.with(|v| v.borrow_mut().insert(monitor.clone(), plan.show.clone()));
    persist(&[]);
    match chosen {
        Some(id) => win::focus(id),
        None => win::focus_after_switch(&shown, &plan.hide),
    }
    let t_focus = ms(t);
    if panel::is_shown() {
        // The new slot's windows are exactly the ones just shown: no need to re-enumerate
        // it. Other monitors only if capturing removed one of their slots (rare).
        let mut data = vec![panel_data_for(monitor.clone(), &plan.show)];
        if others_changed {
            data.extend(names.iter().filter(|n| **n != monitor).map(|n| panel_data_for(n.clone(), &win::visible_windows_on(n))));
        }
        panel::update(data);
    } else {
        show_osd(after, count, rect);
    }
    log!("  work: write-ahead {t_write:.1}, +hide/show {:.1}, +persist/focus {:.1}, +panel/osd {:.1} ms",
        t_hideshow - t_write, t_focus - t_hideshow, ms(t) - t_focus);
    if let Some(m) = reopen {
        spread_monitor(m);
    }
}

enum Moved {
    Nothing,
    /// Hidden in this slot of the target monitor.
    Hidden(usize),
    /// Visible on another monitor, at this rect if known.
    Relocated(Option<RECT>),
}

/// Move a dragged window into a slot (design §7): write-ahead, relocate, hide, persist.
/// `in_slot`: None = it's on screen on `from`; Some(i) = hidden in `from`'s slot i.
fn do_move(entries: Vec<Entry>, from: String, in_slot: Option<usize>, to: String, target: Target) -> Moved {
    let (result, extra) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let old = s.hidden();
        let result = s.move_to_slot(&entries, &from, in_slot, &to, target, win::now_ms());
        (result, s.tag_active(&to, &core::leaving(&old, &s.hidden())))
    });
    if result == MoveResult::Rejected {
        return Moved::Nothing; // its own monitor's current slot, or a stale tile
    }
    let hide = result != MoveResult::Visible;
    log!("move {} window(s) {from} -> {to} {result:?}", entries.len());
    let focused = win::has_focus(&entries);
    persist(&extra);
    // A hidden window moved into a hidden slot must not show while it's repositioned.
    let keep_hidden = hide && in_slot.is_some();
    let new_rect = if from != to { win::relocate(&entries, &from, &to, keep_hidden) } else { None };
    if hide {
        win::hide(&entries);
        snap::hidden(entries.iter().map(|e| e.id));
    } else if in_slot.is_some() {
        win::show(&entries); // a hidden window dropped on a current slot comes on screen
    }
    persist(&[]);
    if hide && focused {
        win::focus_after_switch(&[], &entries);
    }
    panel::update(panel_data());
    if hide || from != to {
        // the enumeration above can still see it before the async hide/move lands
        VISIBLE.with(|v| {
            if let Some(list) = v.borrow_mut().get_mut(&from) {
                list.retain(|e| !entries.iter().any(|m| m.id == e.id));
            }
        });
    }
    match result {
        MoveResult::Hidden(i) => Moved::Hidden(i),
        _ => Moved::Relocated(new_rect),
    }
}

/// The ghost finishes its trip: back over the window, or nowhere if motion is off.
fn soul_home(entries: &[Entry]) {
    let root = entries.iter().find(|e| win::is_root(e.id)).or(entries.first());
    match root.and_then(|e| win::window_rect(e.id)) {
        Some(r) if motion_on() => panel::ghost_fly(r),
        _ => panel::ghost_end(),
    }
}

/// Icons and titles of root windows (owned dialogs are left out of the lists).
fn titles(entries: &[Entry]) -> Vec<(HICON, String)> {
    entries
        .iter()
        .filter(|e| win::is_root(e.id))
        .map(|e| (e.id, win::title_of(win::hwnd(e.id))))
        .filter(|(_, t)| !t.is_empty())
        .map(|(id, t)| (win::icon_of(id), t))
        .collect()
}

/// One monitor's panel contents; `visible` = what's on screen in its current slot.
fn panel_data_for(monitor: String, visible: &[Entry]) -> panel::PanelData {
    win::icon_budget(false);
    let (active, slots) = STATE.with(|s| s.borrow().slots(&monitor));
    let titles = slots.iter().enumerate().map(|(i, s)| if i == active { titles(visible) } else { titles(s) }).collect();
    panel::PanelData { monitor, active, titles }
}

/// Every monitor, enumerating what's visible (tens of ms: use only when the panel opens).
fn panel_data() -> Vec<panel::PanelData> {
    win::monitor_names()
        .into_iter()
        .map(|monitor| {
            let visible = win::visible_windows_on(&monitor);
            VISIBLE.with(|v| v.borrow_mut().insert(monitor.clone(), visible.clone()));
            panel_data_for(monitor, &visible)
        })
        .collect()
}

/// Asks for the app icons of every root window the exposé can show, hidden ones too (an icon
/// needs no visible window). The window's own icon goes along as the fallback source.
fn request_icons() {
    win::icon_budget(false);
    let mut items = Vec::new();
    for monitor in win::monitor_names() {
        let (active, slots) = STATE.with(|s| s.borrow().slots(&monitor));
        let on_screen = VISIBLE.with(|v| v.borrow().get(&monitor).cloned().unwrap_or_default());
        for (i, slot) in slots.iter().enumerate() {
            let entries = if i == active { &on_screen } else { slot };
            items.extend(entries.iter().filter(|e| win::is_root(e.id)).map(|e| (e.id, win::icon_of(e.id).0 as isize)));
        }
    }
    icons::request(APP.get(), items);
}

fn show_panels() {
    CLOSED_BY_X.set(None);
    reap_dead(); // windows that closed while hidden
    win::icon_budget(true); // icons are looked up afresh each time the panel opens
    panel::show(&win::monitors(), panel_data());
    hooks::HOVER.store(true, SeqCst);
    // Pictures for the exposé, taken now (not at bare Ctrl+Alt: AltGr is Ctrl+Alt).
    let ids: Vec<u64> = VISIBLE.with(|v| {
        v.borrow().values().flatten().filter(|e| !e.iconic && win::is_root(e.id)).map(|e| e.id).collect()
    });
    snap::start_batch(APP.get(), ids);
    request_icons();
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    on_hover(pt); // resting on a tile as the panel appears counts
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
    // The exposé first: its overlay is click-through, so the window under it would start a drag.
    if let Some(id) = expose::close_at(pt) {
        PRESSED_CLOSE_WINDOW.set(Some(id));
        return;
    }
    if let Some(i) = expose::preview_at(pt) {
        PRESSED_PREVIEW.set(Some((i, pt)));
        return;
    }
    if expose::dim_at(pt) {
        if hooks::PANEL_LATCH.load(SeqCst) {
            end_panel_mode(); // empty space in swipe mode: the exposé and the panel close
        } else {
            expose::collapse(motion_on());
            arm_anim_timer();
        }
        return;
    }
    if panel::is_shown()
        && let Some(x) = panel::hit_close(pt)
    {
        PRESSED_CLOSE.with(|p| *p.borrow_mut() = Some(x));
        return;
    }
    if panel::is_shown()
        && let Some(hit) = panel::tile_hit(pt)
    {
        if hit.is_none() && hooks::PANEL_LATCH.load(SeqCst) {
            end_panel_mode(); // the gap between tiles is empty space: it closes a swipe-opened panel
            return;
        }
        PRESSED.with(|p| *p.borrow_mut() = Some(hit));
        return;
    }
    if hooks::PANEL_LATCH.load(SeqCst) {
        // Empty space in swipe mode closes the panel; nothing under it may start a drag.
        end_panel_mode();
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
    if PREVIEW_DRAG.get() {
        PRESSED_PREVIEW.set(None);
        drop_preview(pt);
        after_panel_click();
        return;
    }
    if let Some((from, entries)) = DRAGGING.with(|d| d.borrow_mut().take()) {
        hooks::DRAG.store(false, SeqCst);
        panel::set_drop(None);
        let Some(Some((to, target))) = panel::tile_hit(pt) else {
            soul_home(&entries);
            return;
        };
        match do_move(entries.clone(), from, None, to.clone(), target) {
            Moved::Hidden(slot) if motion_on() => {
                // after do_move's panel update, so a new tile is where the panel now draws it
                match panel::tile_screen_rect(&to, slot) {
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
    if let Some(id) = PRESSED_CLOSE_WINDOW.take() {
        if expose::close_at(pt) == Some(id) {
            request_close(id); // the exposé stays up, in swipe mode too
        }
        return;
    }
    if let Some((i, _)) = PRESSED_PREVIEW.take() {
        if expose::preview_at(pt) == Some(i) {
            choose(i);
        }
        after_panel_click();
        return;
    }
    if let Some(x) = PRESSED_CLOSE.with(|p| p.borrow_mut().take()) {
        if panel::hit_close(pt).as_ref() == Some(&x) {
            close_slot_ui(x.0, x.1); // the panel stays up, in swipe mode too
        }
        return;
    }
    if let Some(Some(pressed)) = PRESSED.with(|p| p.borrow_mut().take())
        && panel::tile_hit(pt) == Some(Some(pressed.clone()))
    {
        jump(pressed.0, pressed.1);
        after_panel_click();
    }
}

/// Swipe mode ends with the click that acted: the panel closes, input goes back to normal.
fn after_panel_click() {
    if hooks::PANEL_LATCH.load(SeqCst) {
        end_panel_mode();
    }
}

/// Leaves swipe mode (Esc, three-finger swipe down, the Ctrl+Alt release, a click on empty
/// space or after a pick). Closes the panel and switches nothing.
fn end_panel_mode() {
    cancel_expose_wait();
    hooks::PANEL_LATCH.store(false, SeqCst);
    hooks::HOVER.store(false, SeqCst);
    stop_hover();
    PRESSED_PREVIEW.set(None);
    PRESSED_CLOSE_WINDOW.set(None);
    PRESSED.with(|p| p.borrow_mut().take());
    PRESSED_CLOSE.with(|p| p.borrow_mut().take());
    CLOSED_BY_X.set(None);
    expose::collapse(motion_on());
    arm_anim_timer();
    panel::hide();
}

/// What the screen shows right now, for a three-finger swipe to move between.
fn current_phase() -> touchpad::Phase {
    if !panel::is_shown() {
        touchpad::Phase::Closed
    } else if expose::open_slot().is_some() {
        touchpad::Phase::Expose
    } else {
        touchpad::Phase::Panel
    }
}

/// Moves the screen to `target` one step at a time (at most two steps are ever needed).
fn goto(target: touchpad::Phase) {
    use touchpad::Phase::*;
    if target != Expose {
        cancel_expose_wait();
    }
    for _ in 0..2 {
        match (current_phase(), target) {
            (cur, want) if cur == want => return,
            (Closed, _) => {
                hooks::PANEL_LATCH.store(true, SeqCst);
                show_panels();
            }
            (Panel, Expose) => open_all_spreads(),
            (Expose, Panel) => {
                // A preview under the mouse is chosen as the swipe leaves the exposé, as a
                // Ctrl+Alt release over it does; this touch is then finished.
                if let Some(id) = expose::hovered() {
                    SWIPE_START.set(None);
                    choose(id);
                    end_panel_mode();
                    return;
                }
                expose::collapse(motion_on());
                arm_anim_timer();
            }
            (Panel | Expose, Closed) => {
                if let Some(id) = expose::hovered() {
                    choose(id);
                }
                end_panel_mode();
            }
            (Panel, Panel) | (Expose, Expose) => return,
        }
    }
}

/// A three-finger touch, live. The panel and the exposé come in as the fingers pass each
/// distance, and back out as they reverse; left and right switch the slot under the cursor
/// once, as the swipe passes its threshold (negative left: next, as the slide animation runs).
fn on_swipe(u: touchpad::Update) {
    if !touchpad::enabled() || hooks::CHORD.load(SeqCst) {
        return;
    }
    match u {
        touchpad::Update::Begin => SWIPE_START.set(Some(current_phase())),
        touchpad::Update::Vertical(dy) => {
            if let Some(start) = SWIPE_START.get() {
                goto(touchpad::target(start, dy));
            }
        }
        touchpad::Update::Horizontal(dx) => {
            on_switch(win::cursor_monitor().0, if dx < 0 { Dir::Next } else { Dir::Prev });
        }
    }
}

fn on_move() {
    hooks::MOVE_PENDING.store(false, SeqCst);
    let pt = hooks::unpack(hooks::MOVE_PT.load(SeqCst));
    if let Some(at) = CLOSED_BY_X.get()
        && (pt.x - at.x).pow(2) + (pt.y - at.y).pow(2) > 64
    {
        CLOSED_BY_X.set(None); // the mouse went elsewhere: a release over a tile means it
    }
    if PREVIEW_DRAG.get() {
        expose::drag_move(pt);
        panel::set_drop(panel::tile_hit(pt).flatten());
    } else if hooks::DRAG.load(SeqCst) {
        panel::ghost_move(pt);
        panel::set_drop(panel::tile_hit(pt).flatten());
    } else if let Some((i, down)) = PRESSED_PREVIEW.get() {
        // a press on a preview that moves more than a few pixels becomes a drag
        let scale = win::monitors().into_iter().find(|m| m.name == win::monitor_at(down).0).map_or(1.0, |m| m.scale);
        let far = ((pt.x - down.x).pow(2) + (pt.y - down.y).pow(2)) as f32 > (6.0 * scale).powi(2);
        if far && expose::drag_start(i, down, motion_on()) {
            PREVIEW_DRAG.set(true);
            hooks::DRAG.store(true, SeqCst); // no switches mid-drag, as for window drags
            stop_hover();
            expose::drag_move(pt);
            arm_anim_timer();
        }
    } else if panel::is_shown() {
        on_hover(pt);
    }
}

/// Shows every hidden window and resets to one slot per monitor, keeping the saved layout
/// for the next start (Exit, logoff).
fn show_all() {
    cancel_preview_drag();
    anim::clear_all();
    let entries = STATE.with(|s| s.borrow_mut().restore_all());
    win::show(&entries);
}

/// Shows everything and forgets the layout.
fn restore_all() {
    show_all();
    win::clear_state();
}

fn show_osd(slot: usize, count: usize, monitor: RECT) {
    OSD_SLOT.set((slot, count));
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
        if msg == WM_CLOSE {
            return LRESULT(0); // only the app window decides when we close
        }
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
            let (slot, count) = OSD_SLOT.get();
            let mut text: Vec<u16> = format!("{slot} / {count}").encode_utf16().collect();
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
            WM_TRAY if ASKING.get() && matches!(lp.0 as u32, WM_LBUTTONUP | WM_RBUTTONUP | WM_CONTEXTMENU) => raise_question(),
            WM_TRAY if matches!(lp.0 as u32, WM_RBUTTONUP | WM_CONTEXTMENU) => tray_menu(h),
            WM_ASK => ask_questions(h),
            hooks::WM_CHORD_DOWN => {
                SetTimer(Some(h), PANEL_TIMER, PANEL_DELAY_MS, None);
            }
            WM_TIMER if wp.0 == ANIM_TIMER => {
                anim::enforce_limit(win::now_ms());
                if !anim::any() {
                    let _ = KillTimer(Some(h), ANIM_TIMER);
                }
            }
            WM_TIMER if wp.0 == HOVER_TIMER => {
                let _ = KillTimer(Some(h), HOVER_TIMER);
                let tile = HOVER_TILE.with(|t| t.borrow_mut().take());
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                if let Some((m, i)) = tile
                    && panel::is_shown()
                    && !hooks::DRAG.load(SeqCst)
                    && panel::hit(pt) == Some(Some((m.clone(), Target::Slot(i))))
                {
                    open_expose(m, i);
                }
            }
            snap::WM_SNAPSHOT => {
                expose::snapshots_arrived(snap::drain());
                expose::icons_arrived(icons::drain());
                if EXPOSE_WAIT.get() && !snap::busy() {
                    spread_all_now();
                }
            }
            WM_TIMER if wp.0 == CLOSE_TIMER => on_close_timer(),
            WM_TIMER if wp.0 == EXPOSE_TIMER => {
                let _ = KillTimer(Some(h), EXPOSE_TIMER);
                if EXPOSE_WAIT.replace(false) {
                    spread_all_now(); // the pictures are late: open with what there is
                }
            }
            WM_TIMER if wp.0 == PANEL_TIMER => {
                let _ = KillTimer(Some(h), PANEL_TIMER);
                if hooks::CHORD.load(SeqCst) {
                    show_panels();
                }
            }
            // A swipe-opened panel ends with the Ctrl+Alt release too: clicks decide, not keys.
            hooks::WM_CHORD_UP if hooks::PANEL_LATCH.load(SeqCst) => end_panel_mode(),
            hooks::WM_PANEL_ESC => {
                if hooks::PANEL_LATCH.load(SeqCst) {
                    end_panel_mode();
                }
            }
            WM_INPUT => {
                if let Some(u) = touchpad::on_raw_input(lp) {
                    on_swipe(u);
                }
                return DefWindowProcW(h, msg, wp, lp);
            }
            hooks::WM_CHORD_UP => {
                let _ = KillTimer(Some(h), PANEL_TIMER);
                stop_hover();
                hooks::HOVER.store(false, SeqCst);
                DROP_HOLD.with(|d| d.borrow_mut().take());
                // A release mid-drag cancels the drag; it never chooses.
                let was_dragging = PREVIEW_DRAG.get();
                cancel_preview_drag();
                // Only a real release chooses (not Ctrl+Alt+Del, an AltGr key, or a reset).
                match expose::hovered() {
                    Some(i) if wp.0 == hooks::RELEASED && !was_dragging => choose(i),
                    _ => {
                        expose::collapse(motion_on());
                        arm_anim_timer();
                        if wp.0 == hooks::RELEASED && !was_dragging {
                            release_on_tile();
                        }
                    }
                }
                PRESSED_PREVIEW.set(None);
                PRESSED_CLOSE_WINDOW.set(None);
                cancel_drag();
                panel::hide();
                WHEEL.set(0);
                PRESSED.with(|p| p.borrow_mut().take());
                PRESSED_CLOSE.with(|p| p.borrow_mut().take());
                CLOSED_BY_X.set(None);
            }
            hooks::WM_HOOK_WHEEL => on_wheel(wp.0 as isize as i32, hooks::unpack(lp.0 as u64)),
            hooks::WM_HOOK_HWHEEL => on_tilt(wp.0 as isize as i32, hooks::unpack(lp.0 as u64)),
            hooks::WM_HOOK_LDOWN => on_ldown(hooks::unpack(lp.0 as u64)),
            hooks::WM_HOOK_LUP => on_lup(hooks::unpack(lp.0 as u64)),
            hooks::WM_HOOK_MOVE => on_move(),
            // Logoff, shutdown, and Windows closing us to update or remove the package.
            WM_QUERYENDSESSION => {
                show_all(); // "save changes?" prompts visible; layout kept for the next start
                return LRESULT(1);
            }
            WM_ENDSESSION => {
                if wp.0 != 0 {
                    shutdown(h);
                }
            }
            WM_CLOSE => {
                show_all();
                shutdown(h);
            }
            WM_SETTINGCHANGE => tray(h, NIM_MODIFY), // scaling may have changed
            WM_DISPLAYCHANGE => {
                tray(h, NIM_MODIFY);
                cancel_preview_drag();
                PRESSED_CLOSE.with(|p| p.borrow_mut().take());
                anim::clear_all();
                gpu::reset(); // overlays are sized to the old monitors
                let reset = STATE.with(|s| s.borrow_mut().reset_if_monitors_changed(&win::monitor_names()));
                if let Some(entries) = reset {
                    log!("monitor set changed: restoring {}", entries.len());
                    win::show(&entries);
                    win::clear_state();
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

/// Exit without showing anything (callers show first): unhook, unregister, remove the tray
/// icon, destroy the app window. Safe to call twice. At logoff the process may be killed
/// as soon as this returns, so nothing may follow DestroyWindow.
fn shutdown(app: HWND) {
    SHUTTING_DOWN.set(true); // a question still up must not act on its answer
    unsafe {
        let _ = EndMenu(); // a close request can arrive while the tray menu is open
        let _ = KillTimer(Some(app), CLOSE_TIMER);
        cancel_drag();
        panel::hide();
        if let Some(h) = HOOKS.with(|h| h.borrow_mut().take()) {
            hooks::stop(h);
        }
        let _ = UnregisterHotKey(Some(app), HK_PREV);
        let _ = UnregisterHotKey(Some(app), HK_NEXT);
        hooks::PANEL_LATCH.store(false, SeqCst);
        touchpad::restore(); // Windows' three-finger setting goes back as it was
        tray(app, NIM_DELETE);
        let _ = DestroyWindow(app);
    }
}

/// The package's startup task, or None in a dev build (no package identity).
/// Blocking waits are fine here: desktop apps get no consent UI from these calls.
fn startup_task() -> Option<StartupTask> {
    StartupTask::GetAsync(&windows::core::HSTRING::from(STARTUP_TASK)).ok()?.join().ok()
}

/// Tray menu item for the startup task: label, flags.
fn startup_item(state: Option<StartupTaskState>) -> (PCWSTR, MENU_ITEM_FLAGS) {
    match state {
        None => (w!("Start with Windows (installed app only)"), MF_GRAYED),
        Some(StartupTaskState::Enabled) => (w!("Start with Windows"), MF_CHECKED),
        Some(StartupTaskState::EnabledByPolicy) => (w!("Start with Windows"), MF_CHECKED | MF_GRAYED),
        Some(StartupTaskState::DisabledByUser) => (w!("Start with Windows (turn on in Settings)"), MF_UNCHECKED),
        Some(StartupTaskState::DisabledByPolicy) => (w!("Start with Windows"), MF_GRAYED),
        Some(_) => (w!("Start with Windows"), MF_UNCHECKED),
    }
}

fn toggle_startup(task: &StartupTask) {
    match task.State() {
        Ok(StartupTaskState::Enabled) => {
            let _ = task.Disable();
        }
        Ok(StartupTaskState::Disabled) => {
            let _ = task.RequestEnableAsync().and_then(|op| op.join());
        }
        // Windows won't let an app re-enable what the user turned off: send them there.
        // (the WinRT launcher: the Store's certification kit flags ShellExecute in packaged apps)
        Ok(StartupTaskState::DisabledByUser) => {
            if let Ok(uri) = windows::Foundation::Uri::CreateUri(&windows::core::HSTRING::from("ms-settings:startupapps")) {
                let _ = windows::System::Launcher::LaunchUriAsync(&uri);
            }
        }
        _ => {}
    }
}

/// Runs a modal loop (a menu, a message box): our frame loop can't step animations meanwhile,
/// so none run, and motion stays off until it returns. Safe to nest.
fn modal<T>(app: HWND, f: impl FnOnce() -> T) -> T {
    unsafe {
        let _ = SetForegroundWindow(app);
    }
    anim::clear_all();
    let was = MODAL.replace(true);
    let r = f();
    MODAL.set(was);
    r
}

/// Asks a Yes/No question in a box with its own taskbar button. None if it was ended any other
/// way, or the app is shutting down (the answer must not act then).
fn ask(app: HWND, text: PCWSTR) -> Option<bool> {
    let answer = modal(app, || unsafe { MessageBoxW(None, text, w!("Screen Shifter"), MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND) });
    if SHUTTING_DOWN.get() || !unsafe { IsWindow(Some(app)).as_bool() } {
        return None;
    }
    match answer {
        IDYES => Some(true),
        IDNO => Some(false),
        _ => None,
    }
}

/// The one-time questions, once the app is running: the touchpad, then start with Windows.
fn ask_questions(app: HWND) {
    if MODAL.get() {
        ASK_PENDING.set(true); // asked when the menu that's open now closes
        return;
    }
    ASKING.set(true);
    let mut go_on = true;
    if touchpad::needs_question() {
        match ask(app, touchpad::QUESTION) {
            Some(yes) => touchpad::set_enabled(yes),
            None => go_on = false,
        }
    }
    if go_on && startup_record().is_none() {
        ask_startup(app);
    }
    ASKING.set(false);
}

/// Start with Windows, asked once on an installed package (store policy: consent first).
fn ask_startup(app: HWND) {
    let Some(task) = startup_task() else { return }; // a development build
    let Ok(state) = task.State() else { return };
    if state != StartupTaskState::Disabled {
        write_startup_record(Asked::Skipped); // already on, or decided in Settings or by policy
        return;
    }
    let Some(yes) = ask(app, w!("Start Screen Shifter when you sign in?\n\nYou can change this any time from the tray icon.")) else { return };
    if yes {
        let _ = task.RequestEnableAsync().and_then(|op| op.join());
    }
    write_startup_record(if yes { Asked::Yes } else { Asked::No });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Yes,
    No,
    /// not asked: the setting was already decided
    Skipped,
}

fn parse_asked(text: &str) -> Option<Asked> {
    text.lines().find_map(|l| match l.trim() {
        "asked=yes" => Some(Asked::Yes),
        "asked=no" => Some(Asked::No),
        "asked=skipped" => Some(Asked::Skipped),
        _ => None,
    })
}

fn format_asked(a: Asked) -> &'static str {
    match a {
        Asked::Yes => "asked=yes\n",
        Asked::No => "asked=no\n",
        Asked::Skipped => "asked=skipped\n",
    }
}

fn startup_record_path() -> std::path::PathBuf {
    win::state_dir().join("startup.txt")
}

fn startup_record() -> Option<Asked> {
    parse_asked(&std::fs::read_to_string(startup_record_path()).ok()?)
}

fn write_startup_record(a: Asked) {
    let _ = std::fs::create_dir_all(win::state_dir());
    let _ = std::fs::write(startup_record_path(), format_asked(a));
}

/// The tray icon while a question is up: bring the question forward (it may be behind
/// other windows) instead of opening the menu over it.
fn raise_question() {
    unsafe extern "system" fn find(h: HWND, found: LPARAM) -> windows::core::BOOL {
        let mut class = [0u16; 16];
        let n = unsafe { GetClassNameW(h, &mut class) };
        if String::from_utf16_lossy(&class[..n.max(0) as usize]) == "#32770" && unsafe { IsWindowVisible(h).as_bool() } {
            unsafe { *(found.0 as *mut HWND) = h };
            return false.into();
        }
        true.into()
    }
    let mut found = HWND::default();
    unsafe {
        let _ = EnumThreadWindows(windows::Win32::System::Threading::GetCurrentThreadId(), Some(find), LPARAM(&mut found as *mut HWND as isize));
        if !found.is_invalid() {
            let _ = SetForegroundWindow(found);
        }
    }
}

unsafe fn tray_menu(app: HWND) {
    unsafe {
        let task = startup_task();
        let (label, flags) = startup_item(task.as_ref().and_then(|t| t.State().ok()));
        let menu = CreatePopupMenu().unwrap();
        let _ = AppendMenuW(menu, MF_STRING, CMD_RESTORE as usize, w!("Restore all windows"));
        let _ = AppendMenuW(menu, MF_STRING | flags, CMD_STARTUP as usize, label);
        let touchpad_flags = if touchpad::enabled() { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | touchpad_flags, CMD_TOUCHPAD as usize, w!("Three-finger swipe opens panel"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, w!("Exit"));
        let mut pt = Default::default();
        let _ = GetCursorPos(&mut pt);
        // (foreground is also what makes the menu close on an outside click)
        let cmd = modal(app, || TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, None, app, None));
        let _ = DestroyMenu(menu);
        if ASK_PENDING.take() {
            let _ = PostMessageW(Some(app), WM_ASK, WPARAM(0), LPARAM(0));
        }
        match cmd.0 {
            CMD_RESTORE => restore_all(),
            CMD_STARTUP => {
                if let Some(t) = &task {
                    toggle_startup(t);
                }
            }
            CMD_TOUCHPAD => touchpad::set_enabled(!touchpad::enabled()),
            CMD_EXIT => {
                show_all(); // the next start takes them back
                shutdown(app);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod close_tests {
    use super::*;

    #[test]
    fn a_window_asked_to_close_is_gone_undimmed_or_given_up_on() {
        assert_eq!(poll_close(true, false, 100), Poll::Wait);
        assert_eq!(poll_close(false, false, 100), Poll::Gone, "gone, or its handle was reused");
        assert_eq!(poll_close(true, false, CLOSE_UNDIM_MS), Poll::Undim);
        assert_eq!(poll_close(true, true, CLOSE_UNDIM_MS + 1000), Poll::Wait, "undimming is once");
        assert_eq!(poll_close(false, true, 10_000), Poll::Gone, "still noticed after the undim");
        assert_eq!(poll_close(true, true, CLOSE_GIVE_UP_MS), Poll::Drop);
    }
}

#[cfg(test)]
mod startup_record_tests {
    use super::*;

    #[test]
    fn the_startup_answer_round_trips() {
        for a in [Asked::Yes, Asked::No, Asked::Skipped] {
            assert_eq!(parse_asked(format_asked(a)), Some(a));
        }
        assert_eq!(parse_asked(""), None, "never asked");
        assert_eq!(parse_asked("asked=maybe
"), None, "unknown means not asked");
    }
}
