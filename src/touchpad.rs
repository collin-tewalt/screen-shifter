//! Three-finger swipe up opens the slot panel (openspec change panel-polish-v8).
//!
//! Raw Input delivers the touchpad's own contact reports, so the swipe is read directly.
//! Windows still acts on the same swipe (Task View by default), so the app turns off Windows'
//! "three-finger slide" setting, but only with the owner's consent, and puts it back on Exit.
//! Record of what we changed: `touchpad.txt` in the state folder, written before the change.

use crate::win;
use std::cell::RefCell;
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::Input::*;
use windows::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONQUESTION, MB_YESNO, MessageBoxW};
use windows::core::w;

/// Digitizer usage page and touch pad usage.
const USAGE_PAGE: u16 = 0x0D;
const USAGE: u16 = 0x05;
/// Report ID of the contact report on the test device (see the spike's output).
const CONTACT_REPORT: u8 = 0x03;
/// Contacts in one report, each `flags, x_lo, x_hi, y_lo, y_hi`, starting after the report ID.
const CONTACTS: usize = 5;
const CONTACT_LEN: usize = 5;
/// Fingers that make the gesture.
const FINGERS: usize = 3;
/// Travel from touchdown, in touchpad units, at which the panel comes in (and where a swipe
/// first counts as vertical or horizontal). A full swipe on the test device covered about 490.
pub const PANEL_UNITS: i32 = 100;
/// Travel at which the exposé comes in on every monitor.
pub const EXPOSE_UNITS: i32 = 250;

/// What a three-finger touch reports while it moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Update {
    /// Three fingers touched down: a swipe starts from what's on screen now.
    Begin,
    /// Vertical travel so far, positive is down.
    Vertical(i32),
    /// Horizontal travel, reported once when it first passes the threshold (negative is left).
    Horizontal(i32),
}

/// What the screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Panel,
    Expose,
}

/// The phase a vertical swipe that began in `start` has reached after `dy` of travel. Going
/// back past a threshold undoes the phase it crossed, so the state follows the fingers.
pub fn target(start: Phase, dy: i32) -> Phase {
    match start {
        Phase::Closed if dy <= -EXPOSE_UNITS => Phase::Expose,
        Phase::Closed if dy <= -PANEL_UNITS => Phase::Panel,
        Phase::Closed => Phase::Closed,
        Phase::Panel if dy <= -EXPOSE_UNITS => Phase::Expose,
        Phase::Panel if dy >= PANEL_UNITS => Phase::Closed,
        Phase::Panel => Phase::Panel,
        Phase::Expose if dy >= EXPOSE_UNITS => Phase::Closed,
        Phase::Expose if dy >= PANEL_UNITS => Phase::Panel,
        Phase::Expose => Phase::Expose,
    }
}

const KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad");
const VALUE: windows::core::PCWSTR = w!("ThreeFingerSlideEnabled");

/// The owner has said yes (and the setting is ours to change). Read on the UI thread.
static ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static TRACKER: RefCell<Tracker> = RefCell::new(Tracker::default());
}

/// Tip-down contacts' (x, y) in one report.
fn tip_points(report: &[u8]) -> Vec<(i32, i32)> {
    if report.first() != Some(&CONTACT_REPORT) {
        return Vec::new();
    }
    (0..CONTACTS)
        .filter_map(|i| {
            let off = 1 + i * CONTACT_LEN;
            let c = report.get(off..off + CONTACT_LEN)?;
            let tip_down = c[0] & 1 == 1;
            tip_down.then(|| (u16::from_le_bytes([c[1], c[2]]) as i32, u16::from_le_bytes([c[3], c[4]]) as i32))
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Vertical,
    Horizontal,
}

/// Follows three fingers from touchdown, reporting each change as it happens.
#[derive(Default)]
pub struct Tracker {
    start: Option<(i32, i32)>,
    axis: Option<Axis>,
    switched: bool,
}

impl Tracker {
    /// Feeds one report. Any report without three fingers down ends the swipe.
    pub fn feed(&mut self, report: &[u8]) -> Option<Update> {
        let pts = tip_points(report);
        if pts.len() != FINGERS {
            *self = Tracker::default();
            return None;
        }
        let n = FINGERS as i32;
        let avg = (pts.iter().map(|p| p.0).sum::<i32>() / n, pts.iter().map(|p| p.1).sum::<i32>() / n);
        let Some(start) = self.start else {
            *self = Tracker { start: Some(avg), ..Tracker::default() };
            return Some(Update::Begin);
        };
        let (dx, dy) = (avg.0 - start.0, avg.1 - start.1);
        // The first axis to pass the threshold is the swipe's axis for the rest of the touch.
        let axis = match self.axis {
            Some(axis) => axis,
            None => {
                if dx.abs().max(dy.abs()) < PANEL_UNITS {
                    return None;
                }
                let axis = if dx.abs() > dy.abs() { Axis::Horizontal } else { Axis::Vertical };
                self.axis = Some(axis);
                axis
            }
        };
        match axis {
            // Touchpad Y grows downward, so an upward swipe lowers it.
            Axis::Vertical => Some(Update::Vertical(dy)),
            Axis::Horizontal if !self.switched => {
                self.switched = true;
                Some(Update::Horizontal(dx))
            }
            Axis::Horizontal => None,
        }
    }
}

/// Registers for touchpad input on the app window. Returns false if Windows refuses.
pub fn register(app: HWND) -> bool {
    let dev = RAWINPUTDEVICE { usUsagePage: USAGE_PAGE, usUsage: USAGE, dwFlags: RIDEV_INPUTSINK, hwndTarget: app };
    let ok = unsafe { RegisterRawInputDevices(&[dev], size_of::<RAWINPUTDEVICE>() as u32) }.is_ok();
    crate::log!("touchpad raw input registered: {ok}");
    ok
}

/// Reads one WM_INPUT and reports what the three-finger touch did.
pub fn on_raw_input(lp: LPARAM) -> Option<Update> {
    let handle = HRAWINPUT(lp.0 as *mut _);
    let hdr = size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0u32;
    unsafe { GetRawInputData(handle, RID_INPUT, None, &mut size, hdr) };
    let mut buf = vec![0u8; size as usize];
    if unsafe { GetRawInputData(handle, RID_INPUT, Some(buf.as_mut_ptr().cast()), &mut size, hdr) } != size {
        return None;
    }
    let h = hdr as usize;
    if buf.len() < h + 8 || u32::from_ne_bytes(buf[0..4].try_into().ok()?) != RIM_TYPEHID.0 {
        return None;
    }
    // RAWHID: dwSizeHid, dwCount, then the reports back to back.
    let size_hid = u32::from_ne_bytes(buf[h..h + 4].try_into().ok()?) as usize;
    let count = u32::from_ne_bytes(buf[h + 4..h + 8].try_into().ok()?) as usize;
    let data_start = h + 8;
    let data_end = (data_start + size_hid * count).min(buf.len());
    // Only the first report of a batch: one WM_INPUT is one device report on this hardware.
    let report = buf.get(data_start..data_end.min(data_start + size_hid))?;
    TRACKER.with(|t| t.borrow_mut().feed(report))
}

pub fn enabled() -> bool {
    ENABLED.load(SeqCst)
}

/// What we changed, persisted so a run that died mid-change is put back on the next start.
#[derive(Default)]
struct Record {
    /// None: the owner hasn't been asked yet.
    consent: Option<bool>,
    /// Some while we have changed the setting. The inner Option is the value before the change:
    /// None = absent (Windows' default).
    prior: Option<Option<u32>>,
}

fn record_path() -> PathBuf {
    win::state_dir().join("touchpad.txt")
}

fn read_record() -> Record {
    let mut r = Record::default();
    let Ok(text) = std::fs::read_to_string(record_path()) else { return r };
    for line in text.lines() {
        match line.split_once('=') {
            Some(("consent", v)) => r.consent = Some(v == "yes"),
            Some(("prior", "absent")) => r.prior = Some(None),
            Some(("prior", v)) => {
                if let Ok(n) = v.parse() {
                    r.prior = Some(Some(n));
                }
            }
            _ => {}
        }
    }
    r
}

fn write_record(r: &Record) {
    let mut text = String::new();
    if let Some(c) = r.consent {
        text.push_str(if c { "consent=yes\n" } else { "consent=no\n" });
    }
    match r.prior {
        None => {}
        Some(None) => text.push_str("prior=absent\n"),
        Some(Some(v)) => text.push_str(&format!("prior={v}\n")),
    }
    let _ = std::fs::create_dir_all(win::state_dir());
    let _ = std::fs::write(record_path(), text);
}

fn current() -> Option<u32> {
    let mut v = 0u32;
    let mut size = 4u32;
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            KEY,
            VALUE,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut v as *mut u32).cast()),
            Some(&mut size),
        )
    };
    (err == ERROR_SUCCESS).then_some(v)
}

fn set(v: u32) {
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, KEY, VALUE, REG_DWORD.0, Some((&v as *const u32).cast()), 4);
    }
}

fn delete() {
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, KEY, VALUE);
    }
}

/// Turns Windows' three-finger slide off, remembering the value first (written ahead).
fn apply(r: &mut Record) {
    if r.prior.is_none() {
        r.prior = Some(current());
        write_record(r);
    }
    set(0);
}

/// Puts Windows' setting back as it was before `apply`.
fn put_back(r: &mut Record) {
    if let Some(prior) = r.prior.take() {
        match prior {
            None => delete(),
            Some(v) => set(v),
        }
        write_record(r);
    }
}

fn ask_owner() -> bool {
    let text = w!(
        "Let a three-finger swipe up open the slot panel?\n\n\
         Windows uses that swipe for Task View. Yes turns off Windows' three-finger slide \
         (Settings > Bluetooth & devices > Touch) while screen-shifter runs, and puts it back \
         when screen-shifter exits. You can change this from the tray menu."
    );
    unsafe { MessageBoxW(None, text, w!("screen-shifter"), MB_YESNO | MB_ICONQUESTION) == IDYES }
}

/// Startup: put back anything a run that died left changed, ask once, and apply if allowed.
pub fn startup() {
    let mut r = read_record();
    put_back(&mut r);
    if r.consent.is_none() {
        r.consent = Some(ask_owner());
        write_record(&r);
    }
    let on = r.consent == Some(true);
    ENABLED.store(on, SeqCst);
    if on {
        apply(&mut r);
    }
}

/// The tray's on/off item.
pub fn set_enabled(on: bool) {
    let mut r = read_record();
    r.consent = Some(on);
    write_record(&r);
    ENABLED.store(on, SeqCst);
    if on {
        apply(&mut r);
    } else {
        put_back(&mut r);
    }
}

/// Exit, the panic hook: put Windows' setting back. Safe to call repeatedly.
pub fn restore() {
    let mut r = read_record();
    put_back(&mut r);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a contact report like the test device's: `(flags, x, y)` per contact.
    fn report(contacts: &[(u8, u16, u16)]) -> Vec<u8> {
        let mut r = vec![CONTACT_REPORT];
        for i in 0..CONTACTS {
            match contacts.get(i) {
                Some(&(flags, x, y)) => {
                    r.push(flags);
                    r.extend_from_slice(&x.to_le_bytes());
                    r.extend_from_slice(&y.to_le_bytes());
                }
                None => r.extend_from_slice(&[0; CONTACT_LEN]),
            }
        }
        r.resize(30, 0);
        r
    }

    /// Three fingers at `(x, y)`; flags 0x03/0x07/0x0b as on the device.
    fn three_at(x: u16, y: u16) -> Vec<u8> {
        report(&[(0x03, x, y), (0x07, x + 200, y), (0x0b, x - 200, y)])
    }

    fn lifted() -> Vec<u8> {
        report(&[])
    }

    #[test]
    fn panel_comes_in_then_exposé_while_moving_up() {
        use Phase::*;
        assert_eq!(target(Closed, -(PANEL_UNITS - 1)), Closed);
        assert_eq!(target(Closed, -PANEL_UNITS), Panel);
        assert_eq!(target(Closed, -(EXPOSE_UNITS - 1)), Panel);
        assert_eq!(target(Closed, -EXPOSE_UNITS), Expose);
    }

    #[test]
    fn reversing_undoes_the_phase_crossed() {
        use Phase::*;
        assert_eq!(target(Closed, -400), Expose);
        assert_eq!(target(Closed, -200), Panel); // back below the exposé threshold
        assert_eq!(target(Closed, -50), Closed); // back below the panel threshold
    }

    #[test]
    fn from_the_panel_a_further_swipe_up_spreads_and_down_closes() {
        use Phase::*;
        assert_eq!(target(Panel, -100), Panel);
        assert_eq!(target(Panel, -EXPOSE_UNITS), Expose);
        assert_eq!(target(Panel, PANEL_UNITS), Closed);
        assert_eq!(target(Panel, PANEL_UNITS - 1), Panel);
    }

    #[test]
    fn from_the_exposé_a_swipe_down_goes_back_then_closes() {
        use Phase::*;
        assert_eq!(target(Expose, -300), Expose);
        assert_eq!(target(Expose, PANEL_UNITS), Panel);
        assert_eq!(target(Expose, EXPOSE_UNITS), Closed);
    }

    #[test]
    fn touchdown_begins_then_vertical_travel_is_live() {
        let mut t = Tracker::default();
        assert_eq!(t.feed(&three_at(1000, 500)), Some(Update::Begin));
        assert_eq!(t.feed(&three_at(1000, 450)), None); // under the threshold
        assert_eq!(t.feed(&three_at(1000, 340)), Some(Update::Vertical(-160)));
        assert_eq!(t.feed(&three_at(1000, 200)), Some(Update::Vertical(-300)));
    }

    #[test]
    fn lifting_ends_the_swipe_without_an_update() {
        let mut t = Tracker::default();
        t.feed(&three_at(1000, 500));
        t.feed(&three_at(1000, 100));
        assert_eq!(t.feed(&lifted()), None);
        // a new touchdown starts afresh
        assert_eq!(t.feed(&three_at(1000, 100)), Some(Update::Begin));
    }

    #[test]
    fn horizontal_travel_is_reported_once() {
        let mut t = Tracker::default();
        t.feed(&three_at(1000, 500));
        assert_eq!(t.feed(&three_at(920, 500)), None); // 80 across: under the threshold
        assert_eq!(t.feed(&three_at(700, 500)), Some(Update::Horizontal(-300)));
        assert_eq!(t.feed(&three_at(400, 500)), None);
    }

    #[test]
    fn diagonal_motion_follows_the_axis_it_passed_first() {
        let mut t = Tracker::default();
        t.feed(&three_at(1000, 500));
        // 160 up while only 40 across: vertical, and the horizontal part is ignored after
        assert_eq!(t.feed(&three_at(960, 340)), Some(Update::Vertical(-160)));
        assert_eq!(t.feed(&three_at(600, 340)), Some(Update::Vertical(-160)));
    }

    #[test]
    fn released_contacts_do_not_count() {
        // flags bit 0 (tip switch) clear: the finger is hovering or lifted.
        let not_touching = report(&[(0x02, 1000, 100), (0x06, 1200, 100), (0x0a, 800, 100)]);
        assert!(tip_points(&not_touching).is_empty());
    }

    #[test]
    fn non_contact_reports_are_ignored() {
        let mut other = three_at(1000, 500);
        other[0] = 0x01;
        assert!(tip_points(&other).is_empty());
    }
}
