//! Pure slot logic. No Win32, no I/O: the shell gathers data, this module decides.

use std::collections::BTreeMap;

pub const SLOTS: usize = 4;
/// A switch this soon after the previous one on the same monitor may see a
/// snapshot that doesn't reflect the previous (async) hides/shows yet.
pub const RACE_WINDOW_MS: u64 = 1000;

/// A window as recorded when it was hidden. `id` is the HWND.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub pid: u32,
    pub iconic: bool,
    pub class: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Prev,
    Next,
}

/// What the shell must do. Both lists are topmost-first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub hide: Vec<Entry>,
    pub show: Vec<Entry>,
}

#[derive(Default)]
struct Monitor {
    /// 0-based. The active slot's vec is always empty: its windows are on screen.
    active: usize,
    slots: [Vec<Entry>; SLOTS],
    last: Option<(Plan, u64)>,
}

#[derive(Default)]
pub struct State {
    monitors: BTreeMap<String, Monitor>,
    monitor_set: Vec<String>,
}

impl State {
    /// `visible` is every participating window currently on `monitor`, topmost first.
    pub fn switch(&mut self, monitor: &str, dir: Dir, mut visible: Vec<Entry>, now_ms: u64) -> Plan {
        let m = self.monitors.entry(monitor.to_string()).or_default();
        let target = match dir {
            Dir::Prev => m.active.checked_sub(1),
            Dir::Next => (m.active + 1 < SLOTS).then_some(m.active + 1),
        };
        let Some(target) = target else {
            return Plan::default();
        };

        if let Some((last, at)) = &m.last
            && now_ms.saturating_sub(*at) < RACE_WINDOW_MS
        {
            visible.retain(|v| !contains(&last.hide, v.id));
            for e in &last.show {
                if !contains(&visible, e.id) {
                    visible.push(e.clone());
                }
            }
        }

        // One slot per window: whatever we capture now leaves every other slot.
        for mon in self.monitors.values_mut() {
            for slot in &mut mon.slots {
                slot.retain(|e| !contains(&visible, e.id));
            }
        }

        let m = self.monitors.get_mut(monitor).expect("inserted above");
        let plan = Plan {
            show: std::mem::take(&mut m.slots[target]),
            hide: visible.clone(),
        };
        m.slots[m.active] = visible;
        m.active = target;
        m.last = Some((plan.clone(), now_ms));
        plan
    }

    /// 1-based slot number for display.
    pub fn active(&self, monitor: &str) -> usize {
        self.monitors.get(monitor).map_or(0, |m| m.active) + 1
    }

    /// Every window currently recorded as hidden, on any monitor.
    pub fn hidden(&self) -> Vec<Entry> {
        self.monitors
            .values()
            .flat_map(|m| m.slots.iter().flatten().cloned())
            .collect()
    }

    /// Forget all slots; returns everything that must be shown.
    pub fn restore_all(&mut self) -> Vec<Entry> {
        let all = self.hidden();
        self.monitors.clear();
        all
    }

    /// Restores everything if the set of monitor names differs from last time.
    pub fn reset_if_monitors_changed(&mut self, names: &[String]) -> Option<Vec<Entry>> {
        let mut names = names.to_vec();
        names.sort();
        if names == self.monitor_set {
            return None;
        }
        self.monitor_set = names;
        Some(self.restore_all())
    }
}

fn contains(list: &[Entry], id: u64) -> bool {
    list.iter().any(|e| e.id == id)
}

/// What to persist before acting: everything hidden before or after the switch.
pub fn write_ahead_set(old: &[Entry], new: &[Entry]) -> Vec<Entry> {
    let mut out = old.to_vec();
    out.extend(new.iter().filter(|e| !contains(old, e.id)).cloned());
    out
}

// ---- state file: "v1" header, then "hwnd pid iconic class" per line ----

pub fn serialize(entries: &[Entry]) -> String {
    let mut s = String::from("v1\n");
    for e in entries {
        s += &format!("{} {} {} {}\n", e.id, e.pid, e.iconic as u8, e.class);
    }
    s
}

/// Returns the entries it could parse and the lines it couldn't.
pub fn parse(text: &str) -> (Vec<Entry>, Vec<String>) {
    let mut lines = text.lines();
    match lines.next() {
        None => return (vec![], vec![]),
        Some("v1") => {}
        Some(other) => return (vec![], vec![format!("unknown header: {other}")]),
    }
    let (mut ok, mut bad) = (vec![], vec![]);
    for line in lines.filter(|l| !l.is_empty()) {
        match parse_line(line) {
            Some(e) => ok.push(e),
            None => bad.push(line.to_string()),
        }
    }
    (ok, bad)
}

fn parse_line(line: &str) -> Option<Entry> {
    let mut parts = line.splitn(4, ' ');
    Some(Entry {
        id: parts.next()?.parse().ok()?,
        pid: parts.next()?.parse().ok()?,
        iconic: match parts.next()? {
            "0" => false,
            "1" => true,
            _ => return None,
        },
        class: parts.next()?.to_string(),
    })
}

/// Guards against HWND reuse: only show a handle that still belongs to the same app.
pub fn should_restore(e: &Entry, live_pid: u32, live_class: &str) -> bool {
    e.pid == live_pid && e.class == live_class
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(id: u64) -> Entry {
        Entry { id, pid: id as u32 * 10, iconic: false, class: format!("C{id}") }
    }
    fn ids(v: &[Entry]) -> Vec<u64> {
        v.iter().map(|e| e.id).collect()
    }
    const A: &str = r"\\.\DISPLAY1";
    const B: &str = r"\\.\DISPLAY2";
    const LATER: u64 = 10_000; // spacing that keeps switches outside the race window

    #[test]
    fn monitors_are_independent() {
        let mut s = State::default();
        let p = s.switch(A, Dir::Next, vec![w(1)], 0);
        assert_eq!(ids(&p.hide), [1]);
        assert!(p.show.is_empty());
        assert_eq!(s.active(A), 2);
        assert_eq!(s.active(B), 1);
    }

    #[test]
    fn clamps_at_both_ends() {
        let mut s = State::default();
        assert_eq!(s.switch(A, Dir::Prev, vec![w(1)], 0), Plan::default());
        assert_eq!(s.active(A), 1);
        for i in 1..SLOTS as u64 {
            s.switch(A, Dir::Next, vec![], i * LATER);
        }
        assert_eq!(s.active(A), SLOTS);
        assert_eq!(s.switch(A, Dir::Next, vec![w(9)], 99 * LATER), Plan::default());
        assert_eq!(s.active(A), SLOTS);
    }

    #[test]
    fn round_trip_keeps_order() {
        let mut s = State::default();
        s.switch(A, Dir::Next, vec![w(1), w(2)], 0);
        let p = s.switch(A, Dir::Prev, vec![], LATER);
        assert_eq!(ids(&p.show), [1, 2]);
        assert!(s.hidden().is_empty());
    }

    #[test]
    fn new_window_joins_current_slot() {
        let mut s = State::default();
        s.switch(A, Dir::Next, vec![w(1)], 0);
        s.switch(A, Dir::Prev, vec![], LATER);
        // w(3) opened while back on slot 1
        s.switch(A, Dir::Next, vec![w(1), w(3)], 2 * LATER);
        let p = s.switch(A, Dir::Prev, vec![], 3 * LATER);
        assert_eq!(ids(&p.show), [1, 3]);
    }

    #[test]
    fn dragged_window_stays_on_its_new_monitor() {
        let mut s = State::default();
        // w(1) was on A, user dragged it to B: A's snapshot no longer contains it
        let p = s.switch(A, Dir::Next, vec![], 0);
        assert!(p.hide.is_empty());
        assert!(s.hidden().is_empty());
    }

    #[test]
    fn empty_target_slot() {
        let mut s = State::default();
        let p = s.switch(A, Dir::Next, vec![w(1)], 0);
        assert!(p.show.is_empty());
    }

    #[test]
    fn window_reshown_elsewhere_is_in_one_slot() {
        let mut s = State::default();
        s.switch(A, Dir::Next, vec![w(1)], 0); // w(1) hidden in A slot 1
        // its app re-shows it, user moves it to B, B switches away
        s.switch(B, Dir::Next, vec![w(1)], LATER);
        let p = s.switch(A, Dir::Prev, vec![], 2 * LATER);
        assert!(p.show.is_empty(), "A must not re-show a window now owned by B");
        assert_eq!(ids(&s.hidden()), [1]);
    }

    #[test]
    fn rapid_presses_with_stale_snapshots_round_trip() {
        let mut s = State::default();
        s.switch(A, Dir::Next, vec![w(2)], 0); // slot 1 holds w(2)
        s.switch(A, Dir::Prev, vec![], LATER); // back on slot 1, w(2) shown
        // Right x3, each snapshot taken before previous async ops landed
        s.switch(A, Dir::Next, vec![w(2), w(1)], 2 * LATER);
        s.switch(A, Dir::Next, vec![w(2), w(1)], 2 * LATER + 50); // stale: still looks visible
        s.switch(A, Dir::Next, vec![], 2 * LATER + 100);
        assert_eq!(s.active(A), 4);
        s.switch(A, Dir::Prev, vec![], 2 * LATER + 150);
        s.switch(A, Dir::Prev, vec![], 2 * LATER + 200);
        let p = s.switch(A, Dir::Prev, vec![], 2 * LATER + 250);
        assert_eq!(s.active(A), 1);
        assert_eq!(ids(&p.show), [2, 1]);
        assert!(s.hidden().is_empty());
    }

    #[test]
    fn pending_show_is_captured_by_quick_switch_back() {
        let mut s = State::default();
        s.switch(A, Dir::Next, vec![w(1)], 0);
        s.switch(A, Dir::Next, vec![], LATER); // slot 2 empty
        s.switch(A, Dir::Prev, vec![], 2 * LATER); // slot 2 shown: nothing
        let p = s.switch(A, Dir::Prev, vec![], 2 * LATER + 10); // back to slot 1
        assert_eq!(ids(&p.show), [1]);
        // quickly forward again: w(1) not yet visible in the snapshot
        let p = s.switch(A, Dir::Next, vec![], 2 * LATER + 20);
        assert_eq!(ids(&p.hide), [1], "pending show must be treated as visible");
        assert_eq!(ids(&s.hidden()), [1]);
    }

    #[test]
    fn late_switch_trusts_raw_snapshot() {
        let mut s = State::default();
        s.switch(A, Dir::Next, vec![w(1)], 0);
        s.switch(A, Dir::Prev, vec![], LATER); // shows w(1)
        // user closed w(1) long ago: raw snapshot is empty and must win
        let p = s.switch(A, Dir::Next, vec![], 3 * LATER);
        assert!(p.hide.is_empty());
    }

    #[test]
    fn restore_all_resets_and_clears_race_state() {
        let mut s = State::default();
        s.switch(A, Dir::Next, vec![w(1)], 0);
        s.switch(B, Dir::Next, vec![w(2)], 0);
        assert_eq!(ids(&s.restore_all()), [1, 2]);
        assert_eq!((s.active(A), s.active(B)), (1, 1));
        assert!(s.hidden().is_empty());
        // right after: no stale correction re-adds or drops anything
        let p = s.switch(A, Dir::Next, vec![w(3)], 10);
        assert_eq!(ids(&p.hide), [3]);
    }

    #[test]
    fn monitor_set_change_resets_only_on_change() {
        let mut s = State::default();
        let names = vec![A.to_string(), B.to_string()];
        assert!(s.reset_if_monitors_changed(&names).is_some()); // first call initializes
        s.switch(A, Dir::Next, vec![w(1)], 0);
        let reordered = vec![B.to_string(), A.to_string()];
        assert_eq!(s.reset_if_monitors_changed(&reordered), None);
        assert_eq!(ids(&s.hidden()), [1]);
        let restored = s.reset_if_monitors_changed(&[A.to_string()]).unwrap();
        assert_eq!(ids(&restored), [1]);
        assert_eq!(s.active(A), 1);
    }

    #[test]
    fn write_ahead_is_union() {
        assert_eq!(ids(&write_ahead_set(&[w(1), w(2)], &[w(2), w(3)])), [1, 2, 3]);
    }

    #[test]
    fn state_file_round_trip() {
        let entries = vec![
            w(1),
            Entry { id: 2, pid: 3, iconic: true, class: "Has Spaces In It".into() },
        ];
        let (parsed, bad) = parse(&serialize(&entries));
        assert_eq!(parsed, entries);
        assert!(bad.is_empty());
    }

    #[test]
    fn state_file_bad_input() {
        assert_eq!(parse(""), (vec![], vec![]));
        let (ok, bad) = parse("v2\n1 2 0 X\n");
        assert!(ok.is_empty());
        assert_eq!(bad.len(), 1);
        let (ok, bad) = parse("v1\n1 2 0 X\ngarbage\n3 x 0 Y\n4 5 7 Z\n5 6 1\n");
        assert_eq!(ids(&ok), [1]);
        assert_eq!(bad.len(), 4);
    }

    #[test]
    fn restore_check() {
        let e = w(1);
        assert!(should_restore(&e, 10, "C1"));
        assert!(!should_restore(&e, 11, "C1"));
        assert!(!should_restore(&e, 10, "Other"));
    }

    /// Random switches in a simulated world: no window is ever lost or duplicated.
    #[test]
    fn never_lose_a_window() {
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut rand = |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let mons = [A, B];
        let mut s = State::default();
        let mut screen: BTreeMap<&str, Vec<Entry>> = mons.iter().map(|m| (*m, vec![])).collect();
        let mut ever_hidden = std::collections::BTreeSet::new();
        let mut next_id = 1;
        let mut now = 0;

        for _ in 0..5000 {
            now += LATER;
            let mon = mons[rand(2) as usize];
            match rand(10) {
                0 => {
                    screen.get_mut(mon).unwrap().insert(0, w(next_id));
                    next_id += 1;
                }
                1 => {
                    // drag the top window to the other monitor
                    let other = if mon == A { B } else { A };
                    if let Some(e) = screen.get_mut(mon).unwrap().pop() {
                        screen.get_mut(other).unwrap().insert(0, e);
                    }
                }
                _ => {
                    let dir = if rand(2) == 0 { Dir::Prev } else { Dir::Next };
                    let visible = screen[mon].clone();
                    let p = s.switch(mon, dir, visible, now);
                    for e in &p.show {
                        assert!(ever_hidden.contains(&e.id), "showed a window never hidden");
                    }
                    let scr = screen.get_mut(mon).unwrap();
                    scr.retain(|e| !contains(&p.hide, e.id));
                    scr.extend(p.show.iter().cloned());
                    ever_hidden.extend(p.hide.iter().map(|e| e.id));
                }
            }
            // Invariant: every window is on screen or hidden, exactly once.
            let mut all: Vec<u64> = screen.values().flatten().map(|e| e.id).collect();
            all.extend(ids(&s.hidden()));
            all.sort();
            assert_eq!(all, (1..next_id).collect::<Vec<_>>());
        }
        let restored = s.restore_all();
        let total = screen.values().map(Vec::len).sum::<usize>() + restored.len();
        assert_eq!(total as u64, next_id - 1);
    }
}
