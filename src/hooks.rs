//! Low-level keyboard + mouse hooks on their own thread.
//!
//! Hook procs run inside this thread's message loop and stall the whole system's input
//! while they run, so they only touch atomics and PostMessage to the app window.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering::SeqCst};
use std::thread::JoinHandle;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_CHORD_DOWN: u32 = WM_APP + 10;
pub const WM_CHORD_UP: u32 = WM_APP + 11; // wparam = why (RELEASED / TAINTED / RESET)
pub const WM_HOOK_WHEEL: u32 = WM_APP + 12; // wparam = delta (i16), lparam = point
pub const WM_HOOK_HWHEEL: u32 = WM_APP + 13;
pub const WM_HOOK_LDOWN: u32 = WM_APP + 14;
pub const WM_HOOK_LUP: u32 = WM_APP + 15;
pub const WM_HOOK_MOVE: u32 = WM_APP + 16; // point in MOVE_PT; clear MOVE_PENDING when handled
pub const WM_PANEL_ESC: u32 = WM_APP + 18; // Esc while a swipe-opened panel is up (the key still goes on)

/// Ctrl+Alt held with no other key: mouse input is ours.
pub static CHORD: AtomicBool = AtomicBool::new(false);
/// A window drag is in progress (moves are posted).
pub static DRAG: AtomicBool = AtomicBool::new(false);
/// The panel is up: moves are posted while the chord is held (for the exposé).
pub static HOVER: AtomicBool = AtomicBool::new(false);
/// The panel was opened by a three-finger swipe: mouse input is ours until it closes, with no keys
/// held. Swallowing checks treat it like CHORD; the chord-reset stays CHORD-only.
pub static PANEL_LATCH: AtomicBool = AtomicBool::new(false);

/// Why the chord ended. Only a real release may act (choose a preview).
pub const RELEASED: usize = 0;
/// Another key was pressed (Ctrl+Alt+Del, an AltGr character).
pub const TAINTED: usize = 1;
/// Keys found up without us seeing it (secure desktop).
pub const RESET: usize = 2;
pub static MOVE_PENDING: AtomicBool = AtomicBool::new(false);
pub static MOVE_PT: AtomicU64 = AtomicU64::new(0);

static CTRL: AtomicBool = AtomicBool::new(false);
static ALT: AtomicBool = AtomicBool::new(false);
static TAINT: AtomicBool = AtomicBool::new(false);
/// We swallowed a left-down, so we must swallow its left-up too (and only then).
static SWALLOWED_DOWN: AtomicBool = AtomicBool::new(false);
static APP: AtomicIsize = AtomicIsize::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);

pub fn pack(pt: POINT) -> u64 {
    (pt.y as u32 as u64) << 32 | pt.x as u32 as u64
}

pub fn unpack(v: u64) -> POINT {
    POINT { x: v as u32 as i32, y: (v >> 32) as u32 as i32 }
}

fn post(msg: u32, wp: usize, pt: POINT) {
    let app = HWND(APP.load(SeqCst) as *mut _);
    unsafe {
        let _ = PostMessageW(Some(app), msg, WPARAM(wp), LPARAM(pack(pt) as isize));
    }
}

fn update_chord(why: usize) {
    let now = CTRL.load(SeqCst) && ALT.load(SeqCst) && !TAINT.load(SeqCst);
    if CHORD.swap(now, SeqCst) != now {
        post(if now { WM_CHORD_DOWN } else { WM_CHORD_UP }, if now { 0 } else { why }, POINT::default());
    }
}

unsafe extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if k.flags & LLKHF_INJECTED != LLKHF_INJECTED {
            let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            if down && k.vkCode == VK_ESCAPE.0 as u32 && PANEL_LATCH.load(SeqCst) {
                post(WM_PANEL_ESC, 0, POINT::default());
            }
            match VIRTUAL_KEY(k.vkCode as u16) {
                VK_CONTROL | VK_LCONTROL | VK_RCONTROL => CTRL.store(down, SeqCst),
                VK_MENU | VK_LMENU | VK_RMENU => ALT.store(down, SeqCst),
                VK_LEFT | VK_RIGHT => {} // arrow switching keeps the chord (and panel)
                _ if down && (CTRL.load(SeqCst) || ALT.load(SeqCst)) => TAINT.store(true, SeqCst),
                _ => {}
            }
            let why = if TAINT.load(SeqCst) { TAINTED } else { RELEASED };
            if !CTRL.load(SeqCst) && !ALT.load(SeqCst) {
                TAINT.store(false, SeqCst);
            }
            update_chord(why);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

unsafe extern "system" fn mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let m = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        let pt = m.pt;
        // Key-ups are missed on the secure desktop / elevated focus: never stay stuck.
        if CHORD.load(SeqCst) && !(key_down(VK_CONTROL) && key_down(VK_MENU)) {
            CTRL.store(false, SeqCst);
            ALT.store(false, SeqCst);
            TAINT.store(false, SeqCst);
            update_chord(RESET);
        }
        let chord = CHORD.load(SeqCst) || PANEL_LATCH.load(SeqCst);
        let delta = (m.mouseData >> 16) as u16 as i16 as isize as usize;
        let swallow = match wp.0 as u32 {
            WM_LBUTTONUP => SWALLOWED_DOWN.swap(false, SeqCst) && {
                post(WM_HOOK_LUP, 0, pt);
                true
            },
            WM_LBUTTONDOWN if chord => {
                SWALLOWED_DOWN.store(true, SeqCst);
                post(WM_HOOK_LDOWN, 0, pt);
                true
            }
            WM_MOUSEWHEEL if chord => {
                post(WM_HOOK_WHEEL, delta, pt);
                true
            }
            WM_MOUSEHWHEEL if chord => {
                post(WM_HOOK_HWHEEL, delta, pt);
                true
            }
            WM_MOUSEMOVE if DRAG.load(SeqCst) || (chord && HOVER.load(SeqCst)) => {
                MOVE_PT.store(pack(pt), SeqCst);
                if !MOVE_PENDING.swap(true, SeqCst) {
                    post(WM_HOOK_MOVE, 0, pt);
                }
                false // never swallow movement
            }
            _ => false,
        };
        if swallow {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

pub fn start(app: HWND) -> JoinHandle<()> {
    APP.store(app.0 as isize, SeqCst);
    std::thread::spawn(|| unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        THREAD.store(GetCurrentThreadId(), SeqCst);
        let module = GetModuleHandleW(None).ok().map(Into::into);
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), module, 0);
        let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), module, 0);
        crate::log!("hooks installed: keyboard {}, mouse {}", kb.is_ok(), ms.is_ok());
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
        if let Ok(h) = kb {
            let _ = UnhookWindowsHookEx(h);
        }
        if let Ok(h) = ms {
            let _ = UnhookWindowsHookEx(h);
        }
    })
}

pub fn stop(handle: JoinHandle<()>) {
    unsafe {
        let _ = PostThreadMessageW(THREAD.load(SeqCst), WM_QUIT, WPARAM(0), LPARAM(0));
    }
    let _ = handle.join();
}
