//! Pure slot logic. No Win32, no I/O: the shell gathers data, this module decides.

use std::collections::{BTreeMap, HashSet};

/// Slots per monitor at most ("+ New" disappears at the limit).
pub const MAX_SLOTS: usize = 8;
/// A switch this soon after the previous one on the same monitor may see a
/// snapshot that doesn't reflect the previous (async) hides/shows yet.
pub const RACE_WINDOW_MS: u64 = 1000;
/// Saved boot times further apart than this mean Windows restarted: saved handles are meaningless.
pub const BOOT_TOLERANCE_MS: u64 = 30_000;

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

/// A slot to switch or move to. `New` appends one; a `Slot` index past the end
/// (from a panel drawn before slots were removed) is rejected, never created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Slot(usize),
    New,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveResult {
    Rejected,
    /// Hidden in this slot (its index after empty slots were removed).
    Hidden(usize),
    /// Lands in the target monitor's current slot, on screen.
    Visible,
}

/// A closed slot: what to show, and the index (after removal) of the slot that took its windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closed {
    pub plan: Plan,
    pub receiver: usize,
}

/// What the shell must do. Both lists are topmost-first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub hide: Vec<Entry>,
    pub show: Vec<Entry>,
}

struct Monitor {
    /// 0-based. The active slot's vec is always empty: its windows are on screen.
    active: usize,
    /// Never empty. Invariant after every State mutation: no inactive slot is empty.
    slots: Vec<Vec<Entry>>,
    last: Option<(Plan, u64)>,
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor { active: 0, slots: vec![vec![]], last: None }
    }
}

impl Monitor {
    /// Index for `target`, appending a slot for `New`. None if out of range or at the limit.
    fn resolve(&mut self, target: Target) -> Option<usize> {
        match target {
            Target::Slot(i) => (i < self.slots.len()).then_some(i),
            Target::New if self.slots.len() < MAX_SLOTS => {
                self.slots.push(vec![]);
                Some(self.slots.len() - 1)
            }
            Target::New => None,
        }
    }

    /// Drops empty inactive slots; the rest renumber.
    fn prune(&mut self) {
        let active = self.active;
        for (i, slot) in std::mem::take(&mut self.slots).into_iter().enumerate() {
            if i == active {
                self.active = self.slots.len();
            }
            if i == active || !slot.is_empty() {
                self.slots.push(slot);
            }
        }
    }
}

#[derive(Default)]
pub struct State {
    monitors: BTreeMap<String, Monitor>,
    monitor_set: Vec<String>,
}

impl State {
    /// `visible` is every participating window currently on `monitor`, topmost first.
    /// Clamps at both ends: never creates a slot. None if nothing happened.
    pub fn switch(&mut self, monitor: &str, dir: Dir, visible: Vec<Entry>, now_ms: u64) -> Option<Plan> {
        let (active, len) = self.monitors.get(monitor).map_or((0, 1), |m| (m.active, m.slots.len()));
        let target = match dir {
            Dir::Prev => active.checked_sub(1)?,
            Dir::Next => Some(active + 1).filter(|&t| t < len)?,
        };
        self.switch_to(monitor, Target::Slot(target), visible, now_ms)
    }

    /// Jump straight to `target`. None for the current slot, a stale index, or `New` at the limit.
    pub fn switch_to(&mut self, monitor: &str, target: Target, mut visible: Vec<Entry>, now_ms: u64) -> Option<Plan> {
        let m = self.monitors.entry(monitor.to_string()).or_default();
        if target == Target::Slot(m.active) {
            return None;
        }
        let target = m.resolve(target)?;

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
        let active = m.active;
        m.slots[active] = visible;
        m.active = target;
        m.last = Some((plan.clone(), now_ms));
        self.prune_all();
        Some(plan)
    }

    /// Move a window (root + owned `entries`) from `from` monitor into `target` on `to`.
    /// `in_slot`: where it is now: None = on screen in `from`'s current slot; Some(i) =
    /// hidden in `from`'s slot i (dragged from the exposé).
    pub fn move_to_slot(&mut self, entries: &[Entry], from: &str, in_slot: Option<usize>, to: &str, target: Target, now_ms: u64) -> MoveResult {
        let Some(first) = entries.first() else { return MoveResult::Rejected };
        let from_active = self.monitors.get(from).map_or(0, |m| m.active);
        let m = self.monitors.entry(to.to_string()).or_default();
        let to_active = m.active;
        if from == to && target == Target::Slot(in_slot.unwrap_or(from_active)) {
            return MoveResult::Rejected; // dropped where it already is: change nothing
        }
        let Some(slot) = m.resolve(target) else { return MoveResult::Rejected };
        for m in self.monitors.values_mut() {
            for s in &mut m.slots {
                s.retain(|e| !contains(entries, e.id));
            }
            if let Some((p, _)) = &mut m.last {
                p.hide.retain(|e| !contains(entries, e.id));
                p.show.retain(|e| !contains(entries, e.id));
            }
        }
        let hide = slot != to_active;
        if hide {
            let slots = &mut self.monitors.get_mut(to).expect("inserted above").slots;
            slots[slot].splice(0..0, entries.iter().cloned());
        }
        // The async hide/relocation won't have landed before a quick next snapshot.
        self.fresh_last(from, now_ms).hide.extend(entries.iter().cloned());
        if !hide {
            self.fresh_last(to, now_ms).show.extend(entries.iter().cloned());
        }
        self.prune_all();
        if !hide {
            return MoveResult::Visible;
        }
        let i = self.monitors[to].slots.iter().position(|s| contains(s, first.id)).expect("just recorded");
        MoveResult::Hidden(i)
    }

    /// Closes slot `i`, folding its windows into the one on its left (on its right for the first).
    /// Nothing is ever hidden: the windows that become visible are in `plan.show`. None for an
    /// unknown monitor, a single slot, or a stale index.
    pub fn close_slot(&mut self, monitor: &str, i: usize, now_ms: u64) -> Option<Closed> {
        let m = self.monitors.get_mut(monitor)?;
        if m.slots.len() < 2 || i >= m.slots.len() {
            return None;
        }
        let (active, n) = (m.active, if i == 0 { 1 } else { i - 1 });
        let receiver = n - (n > i) as usize;
        let removed = m.slots.remove(i);
        let shown = if i == active {
            m.active = receiver;
            std::mem::take(&mut m.slots[receiver])
        } else {
            m.active -= (active > i) as usize;
            if n == active {
                removed
            } else {
                m.slots[receiver].extend(removed);
                vec![]
            }
        };
        if !shown.is_empty() {
            // a quick switch must see them as on screen, even if an earlier plan hid them
            let last = self.fresh_last(monitor, now_ms);
            last.hide.retain(|e| !contains(&shown, e.id));
            last.show.retain(|e| !contains(&shown, e.id));
            last.show.extend(shown.iter().cloned());
        }
        Some(Closed { plan: Plan { show: shown, hide: vec![] }, receiver })
    }

    fn prune_all(&mut self) {
        for m in self.monitors.values_mut() {
            m.prune();
        }
    }

    /// The monitor's last plan if still trusted, else a new empty one stamped `now_ms`.
    /// Never re-stamp an old plan: its stale entries would be trusted again.
    fn fresh_last(&mut self, monitor: &str, now_ms: u64) -> &mut Plan {
        let m = self.monitors.entry(monitor.to_string()).or_default();
        let stale = m.last.as_ref().is_none_or(|(_, at)| now_ms.saturating_sub(*at) >= RACE_WINDOW_MS);
        if stale {
            m.last = Some((Plan::default(), now_ms));
        }
        &mut m.last.as_mut().expect("set above").0
    }

    /// 1-based slot number for display.
    pub fn active(&self, monitor: &str) -> usize {
        self.monitors.get(monitor).map_or(0, |m| m.active) + 1
    }

    /// Number of slots on `monitor`.
    pub fn count(&self, monitor: &str) -> usize {
        self.monitors.get(monitor).map_or(1, |m| m.slots.len())
    }

    /// For the panel: 0-based active slot and each slot's recorded windows.
    pub fn slots(&self, monitor: &str) -> (usize, Vec<Vec<Entry>>) {
        self.monitors.get(monitor).map_or((0, vec![vec![]]), |m| (m.active, m.slots.clone()))
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
        let names = sorted(names);
        if names == self.monitor_set {
            return None;
        }
        self.monitor_set = names;
        Some(self.restore_all())
    }

    /// The state file: layout plus `extra` lines (windows about to be shown, see `tag_active`).
    pub fn serialize(&self, boot: u64, extra: &[Tagged]) -> String {
        let mut s = format!("v2 {boot}\n");
        for (name, m) in &self.monitors {
            s += &format!("m {name} {}\n", m.active);
        }
        let recorded = self.monitors.iter().flat_map(|(name, m)| {
            m.slots.iter().enumerate().flat_map(move |(i, slot)| slot.iter().map(move |e| (name, i, e)))
        });
        for (name, i, e) in recorded.chain(extra.iter().map(|(n, i, e)| (n, *i, e))) {
            s += &format!("w {name} {i} {} {} {} {}\n", e.id, e.pid, e.iconic as u8, e.class);
        }
        s
    }

    /// Tags windows about to be shown on `monitor` with its active slot, which a later
    /// start reads as "belongs on screen" (write-ahead for a crash mid-switch).
    pub fn tag_active(&self, monitor: &str, entries: &[Entry]) -> Vec<Tagged> {
        let active = self.monitors.get(monitor).map_or(0, |m| m.active);
        entries.iter().map(|e| (monitor.to_string(), active, e.clone())).collect()
    }
}

fn sorted(names: &[String]) -> Vec<String> {
    let mut names = names.to_vec();
    names.sort();
    names
}

fn contains(list: &[Entry], id: u64) -> bool {
    list.iter().any(|e| e.id == id)
}

/// Windows hidden before but not after: the ones about to be shown.
pub fn leaving(old: &[Entry], new: &[Entry]) -> Vec<Entry> {
    old.iter().filter(|e| !contains(new, e.id)).cloned().collect()
}

// ---- state file v2: "v2 <boot>", then "m <monitor> <active>" and
// ---- "w <monitor> <slot> <hwnd> <pid> <iconic> <class>" lines. v1 is still read.

/// A window line: monitor, 0-based slot, entry.
pub type Tagged = (String, usize, Entry);

#[derive(Debug, Default, PartialEq)]
pub struct Layout {
    /// Boot time the file was written under; None for a v1 file (nothing to take back).
    pub boot: Option<u64>,
    pub active: BTreeMap<String, usize>,
    pub windows: Vec<Tagged>,
}

/// Returns what it could parse and the lines it couldn't.
pub fn parse(text: &str) -> (Layout, Vec<String>) {
    let mut layout = Layout::default();
    let mut lines = text.lines();
    let v2 = match lines.next() {
        None => return (layout, vec![]),
        Some("v1") => false,
        Some(h) => match h.strip_prefix("v2 ").and_then(|b| b.parse().ok()) {
            Some(boot) => {
                layout.boot = Some(boot);
                true
            }
            None => return (layout, vec![format!("unknown header: {h}")]),
        },
    };
    let mut bad = vec![];
    for line in lines.filter(|l| !l.is_empty()) {
        let ok = if v2 {
            parse_v2(line, &mut layout)
        } else {
            parse_entry(line).map(|e| layout.windows.push((String::new(), 0, e))).is_some()
        };
        if !ok {
            bad.push(line.to_string());
        }
    }
    (layout, bad)
}

fn parse_v2(line: &str, layout: &mut Layout) -> bool {
    let mut p = line.splitn(4, ' ');
    match (p.next(), p.next(), p.next(), p.next()) {
        (Some("m"), Some(name), Some(active), None) => {
            active.parse().map(|a| layout.active.insert(name.to_string(), a)).is_ok()
        }
        (Some("w"), Some(name), Some(slot), Some(rest)) => match (slot.parse(), parse_entry(rest)) {
            (Ok(slot), Some(e)) => {
                layout.windows.push((name.to_string(), slot, e));
                true
            }
            _ => false,
        },
        _ => false,
    }
}

/// "hwnd pid iconic class"; the class may contain spaces.
fn parse_entry(line: &str) -> Option<Entry> {
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

/// What the shell found for a saved window at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// Closed, or the handle now belongs to another app.
    Gone,
    /// Live (entry re-read), and its root is on the recorded monitor.
    Here(Entry),
    /// Live, but on another monitor (moved while we were off, or the monitor is gone).
    Elsewhere(Entry),
}

/// Rebuilds the slots from a saved layout at startup. Returns the new state (its hidden
/// windows are to be hidden) and the windows to show, tagged for the startup write-ahead.
pub fn adopt(layout: &Layout, boot: u64, names: &[String], check: impl Fn(&str, &Entry) -> Check) -> (State, Vec<Tagged>) {
    let mut state = State { monitor_set: sorted(names), ..Default::default() };
    let same_boot = layout.boot.is_some_and(|b| b.abs_diff(boot) <= BOOT_TOLERANCE_MS);
    let mut seen = HashSet::new();
    let mut show: Vec<(String, Entry)> = vec![];
    for (mon, slot, e) in &layout.windows {
        if !seen.insert(e.id) {
            continue; // a damaged file listing a window twice: first wins
        }
        let live = match check(mon, e) {
            Check::Gone => continue,
            Check::Elsewhere(live) => {
                show.push((mon.clone(), live));
                continue;
            }
            Check::Here(live) => live,
        };
        let active = layout.active.get(mon).copied().unwrap_or(0).min(MAX_SLOTS - 1);
        if !same_boot || *slot == active || *slot >= MAX_SLOTS || !names.contains(mon) {
            show.push((mon.clone(), live));
            continue;
        }
        let m = state.monitors.entry(mon.clone()).or_default();
        m.active = active;
        let len = (*slot).max(active) + 1;
        if m.slots.len() < len {
            m.slots.resize(len, vec![]);
        }
        m.slots[*slot].push(live);
    }
    state.prune_all();
    let tagged = show
        .into_iter()
        .map(|(mon, e)| {
            let active = state.monitors.get(&mon).map_or(0, |m| m.active);
            (mon, active, e)
        })
        .collect();
    (state, tagged)
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
    const BOOT: u64 = 1_700_000_000_000;

    fn new(s: &mut State, m: &str, visible: Vec<Entry>, now: u64) -> Plan {
        s.switch_to(m, Target::New, visible, now).expect("created")
    }
    fn next(s: &mut State, m: &str, visible: Vec<Entry>, now: u64) -> Plan {
        s.switch(m, Dir::Next, visible, now).expect("switched")
    }
    fn prev(s: &mut State, m: &str, visible: Vec<Entry>, now: u64) -> Plan {
        s.switch(m, Dir::Prev, visible, now).expect("switched")
    }
    fn names() -> Vec<String> {
        vec![A.to_string(), B.to_string()]
    }

    #[test]
    fn starts_with_one_slot_and_monitors_are_independent() {
        let mut s = State::default();
        assert_eq!((s.count(A), s.active(A)), (1, 1));
        let p = new(&mut s, A, vec![w(1)], 0);
        assert_eq!(ids(&p.hide), [1]);
        assert!(p.show.is_empty());
        assert_eq!((s.active(A), s.count(A)), (2, 2));
        assert_eq!((s.active(B), s.count(B)), (1, 1));
    }

    #[test]
    fn clamps_at_both_ends_and_never_creates() {
        let mut s = State::default();
        assert_eq!(s.switch(A, Dir::Prev, vec![w(1)], 0), None);
        assert_eq!(s.switch(A, Dir::Next, vec![w(1)], 0), None);
        assert_eq!(s.count(A), 1);
        new(&mut s, A, vec![w(1)], LATER);
        assert_eq!(s.switch(A, Dir::Next, vec![w(2)], 2 * LATER), None);
        assert_eq!((s.active(A), s.count(A)), (2, 2));
    }

    #[test]
    fn round_trip_keeps_order_and_drops_the_empty_slot() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1), w(2)], 0);
        let p = prev(&mut s, A, vec![], LATER);
        assert_eq!(ids(&p.show), [1, 2]);
        assert!(s.hidden().is_empty());
        assert_eq!(s.count(A), 1, "slot 2 was left empty");
    }

    #[test]
    fn new_window_joins_current_slot() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        prev(&mut s, A, vec![], LATER);
        // w(3) opened while back on slot 1
        new(&mut s, A, vec![w(1), w(3)], 2 * LATER);
        let p = prev(&mut s, A, vec![], 3 * LATER);
        assert_eq!(ids(&p.show), [1, 3]);
    }

    #[test]
    fn dragged_window_stays_on_its_new_monitor() {
        let mut s = State::default();
        // w(1) was on A, user dragged it to B: A's snapshot no longer contains it
        let p = new(&mut s, A, vec![], 0);
        assert!(p.hide.is_empty());
        assert!(s.hidden().is_empty());
    }

    #[test]
    fn window_reshown_elsewhere_is_in_one_slot() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0); // w(1) hidden in A slot 1, A on 2 of 2
        // its app re-shows it, user moves it to B, B switches away
        new(&mut s, B, vec![w(1)], LATER);
        assert_eq!((s.count(A), s.active(A)), (1, 1), "A's emptied slot 1 is removed");
        assert_eq!(s.switch(A, Dir::Prev, vec![], 2 * LATER), None);
        assert_eq!(ids(&s.hidden()), [1]);
        assert_eq!(s.slots(B).1[0], vec![w(1)]);
    }

    #[test]
    fn rapid_presses_with_stale_snapshots_round_trip() {
        let mut s = State::default();
        new(&mut s, A, vec![w(10)], 0);
        new(&mut s, A, vec![w(11)], LATER);
        new(&mut s, A, vec![w(12)], 2 * LATER);
        prev(&mut s, A, vec![w(13)], 3 * LATER);
        prev(&mut s, A, vec![w(12)], 4 * LATER);
        prev(&mut s, A, vec![w(11)], 5 * LATER); // slot 1 of 4, w(10) shown
        // Right x3, each snapshot taken before previous async ops landed
        let t = 6 * LATER;
        next(&mut s, A, vec![w(10)], t);
        next(&mut s, A, vec![w(10)], t + 50); // stale: w(10) still looks visible
        next(&mut s, A, vec![], t + 100);
        assert_eq!((s.active(A), s.count(A)), (4, 4));
        prev(&mut s, A, vec![], t + 150);
        prev(&mut s, A, vec![], t + 200);
        let p = prev(&mut s, A, vec![], t + 250);
        assert_eq!((s.active(A), s.count(A)), (1, 4));
        assert_eq!(ids(&p.show), [10]);
        let mut hidden = ids(&s.hidden());
        hidden.sort();
        assert_eq!(hidden, [11, 12, 13]);
    }

    #[test]
    fn pending_show_is_captured_by_quick_switch() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        let p = prev(&mut s, A, vec![], LATER);
        assert_eq!(ids(&p.show), [1]);
        // quickly forward again: w(1) not yet visible in the snapshot
        let p = new(&mut s, A, vec![], LATER + 20);
        assert_eq!(ids(&p.hide), [1], "pending show must be treated as visible");
        assert_eq!(ids(&s.hidden()), [1]);
    }

    #[test]
    fn late_switch_trusts_raw_snapshot() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        prev(&mut s, A, vec![], LATER); // shows w(1)
        // user closed w(1) long ago: raw snapshot is empty and must win
        let p = new(&mut s, A, vec![], 3 * LATER);
        assert!(p.hide.is_empty());
    }

    #[test]
    fn restore_all_resets_and_clears_race_state() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        new(&mut s, B, vec![w(2)], 0);
        assert_eq!(ids(&s.restore_all()), [1, 2]);
        assert_eq!((s.active(A), s.active(B), s.count(A), s.count(B)), (1, 1, 1, 1));
        assert!(s.hidden().is_empty());
        // right after: no stale correction re-adds or drops anything
        let p = new(&mut s, A, vec![w(3)], 10);
        assert_eq!(ids(&p.hide), [3]);
    }

    #[test]
    fn monitor_set_change_resets_only_on_change() {
        let mut s = State::default();
        assert!(s.reset_if_monitors_changed(&names()).is_some()); // first call initializes
        new(&mut s, A, vec![w(1)], 0);
        let reordered = vec![B.to_string(), A.to_string()];
        assert_eq!(s.reset_if_monitors_changed(&reordered), None);
        assert_eq!(ids(&s.hidden()), [1]);
        let restored = s.reset_if_monitors_changed(&[A.to_string()]).unwrap();
        assert_eq!(ids(&restored), [1]);
        assert_eq!((s.active(A), s.count(A)), (1, 1));
    }

    #[test]
    fn switch_to_jumps_and_ignores_current_and_stale() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        new(&mut s, A, vec![w(2)], LATER); // on 3 of 3
        assert_eq!(s.switch_to(A, Target::Slot(2), vec![w(3)], 2 * LATER), None);
        assert_eq!(s.switch_to(A, Target::Slot(5), vec![w(3)], 2 * LATER), None);
        let p = s.switch_to(A, Target::Slot(0), vec![w(3)], 2 * LATER).unwrap();
        assert_eq!((ids(&p.show), ids(&p.hide)), (vec![1], vec![3]));
        assert_eq!((s.active(A), s.count(A)), (1, 3));
    }

    #[test]
    fn leaving_an_empty_slot_renumbers_but_still_switches() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        new(&mut s, A, vec![w(2)], LATER); // slots: [w1] [w2] [active, empty]
        prev(&mut s, A, vec![], 2 * LATER); // empty slot 3 pruned; on slot 2 showing w2
        // move w2 out to a new slot: slot 2 stays (active) but is now empty
        assert_eq!(s.move_to_slot(&[w(2)], A, None, A, Target::New, 3 * LATER), MoveResult::Hidden(2));
        let p = next(&mut s, A, vec![], 4 * LATER);
        assert_eq!(ids(&p.show), [2]);
        assert_eq!((s.active(A), s.count(A)), (2, 2), "same number, different slot");
    }

    #[test]
    fn close_slot_folds_hidden_into_hidden_left_neighbor() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        new(&mut s, A, vec![w(2)], LATER); // [w1] [w2] [active]
        let c = s.close_slot(A, 1, 2 * LATER).unwrap();
        assert_eq!((c.receiver, c.plan), (0, Plan::default()));
        assert_eq!(ids(&s.slots(A).1[0]), [1, 2], "neighbor's windows first");
        assert_eq!((s.active(A), s.count(A)), (2, 2));
    }

    #[test]
    fn close_slot_right_of_active_neighbor_shows_its_windows() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        new(&mut s, A, vec![w(2)], LATER);
        prev(&mut s, A, vec![w(9)], 2 * LATER); // [w1] [active] [w9]
        let c = s.close_slot(A, 2, 3 * LATER).unwrap();
        assert_eq!((ids(&c.plan.show), c.receiver), (vec![9], 1));
        assert_eq!((s.active(A), s.count(A)), (2, 2));
        assert_eq!(ids(&s.hidden()), [1]);
    }

    #[test]
    fn close_first_slot_folds_right() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0); // [w1] [active]
        let c = s.close_slot(A, 0, LATER).unwrap();
        assert_eq!((ids(&c.plan.show), c.receiver), (vec![1], 0));
        assert_eq!((s.active(A), s.count(A)), (1, 1));
        assert!(s.hidden().is_empty());
    }

    #[test]
    fn close_active_first_slot_shows_the_right_neighbor() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        prev(&mut s, A, vec![w(5)], LATER); // [active] [w5]
        let c = s.close_slot(A, 0, 2 * LATER).unwrap();
        assert_eq!(ids(&c.plan.show), [5]);
        assert_eq!((s.active(A), s.count(A)), (1, 1));
        assert!(s.hidden().is_empty());
    }

    #[test]
    fn close_active_slot_keeps_its_windows_and_shows_the_neighbors() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        new(&mut s, A, vec![w(2)], LATER); // [w1] [w2] [active]
        let c = s.close_slot(A, 2, 2 * LATER).unwrap();
        assert_eq!((ids(&c.plan.show), c.receiver), (vec![2], 1));
        assert!(c.plan.hide.is_empty());
        assert_eq!((s.active(A), s.count(A)), (2, 2));
        assert_eq!(ids(&s.hidden()), [1]);
    }

    #[test]
    fn close_slot_rejects_single_stale_and_unknown() {
        let mut s = State::default();
        assert_eq!(s.close_slot(A, 0, 0), None, "unknown monitor");
        new(&mut s, A, vec![w(1)], 0);
        assert_eq!(s.close_slot(A, 5, LATER), None);
        assert_eq!(s.close_slot(B, 0, LATER), None);
        assert_eq!(s.monitors.len(), 1, "no monitor created");
        s.close_slot(A, 0, LATER).unwrap();
        assert_eq!(s.close_slot(A, 0, 2 * LATER), None, "the last slot stays");
    }

    #[test]
    fn close_slot_invariants_hold_everywhere() {
        for slots in 2..=5_usize {
            for active in 0..slots {
                for i in 0..slots {
                    let mut s = State::default();
                    for k in 1..slots as u64 {
                        new(&mut s, A, vec![w(k)], k * LATER);
                    }
                    if active != slots - 1 {
                        s.switch_to(A, Target::Slot(active), vec![w(100)], 50 * LATER).unwrap();
                    }
                    let hidden = s.hidden().len();
                    let c = s.close_slot(A, i, 100 * LATER).unwrap();
                    let (a, after) = s.slots(A);
                    let at = format!("{slots} slots, active {active}, close {i}");
                    assert_eq!(after.len(), slots - 1, "{at}");
                    assert!(after[a].is_empty(), "active vec stays empty: {at}");
                    assert!(after.iter().enumerate().all(|(k, v)| k == a || !v.is_empty()), "no empty inactive slot: {at}");
                    assert!(c.receiver < after.len(), "{at}");
                    assert_eq!(s.hidden().len() + c.plan.show.len(), hidden, "no window lost or duplicated: {at}");
                }
            }
        }
    }

    #[test]
    fn close_without_a_shown_window_keeps_the_race_plan() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        new(&mut s, A, vec![w(2)], LATER); // [w1] [w2] [active]; last hides w2
        s.close_slot(A, 0, LATER + 10).unwrap(); // hidden into hidden: shows nothing
        let p = prev(&mut s, A, vec![w(2), w(7)], LATER + 20); // a stale snapshot still lists w2
        assert_eq!((ids(&p.hide), ids(&p.show)), (vec![7], vec![2, 1]));
    }

    #[test]
    fn window_shown_by_a_close_is_seen_by_a_quick_switch() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0); // [w1] [active]; last hides w1
        s.close_slot(A, 0, 10).unwrap(); // w1 is shown again
        let p = new(&mut s, A, vec![], 20); // enumeration doesn't list it yet
        assert_eq!(ids(&p.hide), [1]);
    }

    #[test]
    fn limit_of_eight() {
        let mut s = State::default();
        for i in 1..MAX_SLOTS as u64 {
            new(&mut s, A, vec![w(i)], i * LATER);
        }
        assert_eq!((s.active(A), s.count(A)), (MAX_SLOTS, MAX_SLOTS));
        assert_eq!(s.switch_to(A, Target::New, vec![w(99)], 99 * LATER), None);
        assert_eq!(s.move_to_slot(&[w(99)], A, None, A, Target::New, 99 * LATER), MoveResult::Rejected);
        assert_eq!(s.count(A), MAX_SLOTS);
    }

    #[test]
    fn create_by_move_keeps_active() {
        let mut s = State::default();
        assert_eq!(s.move_to_slot(&[w(1)], A, None, A, Target::New, 0), MoveResult::Hidden(1));
        assert_eq!((s.active(A), s.count(A)), (1, 2));
        assert_eq!(s.slots(A).1[1], vec![w(1)]);
    }

    #[test]
    fn hidden_window_moves_from_its_slot() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0); // w(1) hidden in A's slot 1, A on slot 2
        // dropped on its own slot: nothing
        assert_eq!(s.move_to_slot(&[w(1)], A, Some(0), A, Target::Slot(0), LATER), MoveResult::Rejected);
        // dropped on its own monitor's current slot: comes on screen there
        assert_eq!(s.move_to_slot(&[w(1)], A, Some(0), A, Target::Slot(1), LATER), MoveResult::Visible);
        assert!(s.hidden().is_empty());
        assert_eq!(s.count(A), 1, "its emptied slot is removed");
        // a quick switch on A captures it as on screen
        let p = new(&mut s, A, vec![], LATER + 100);
        assert_eq!(ids(&p.hide), [1]);
    }

    #[test]
    fn hidden_window_to_other_monitors_hidden_slot() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        assert_eq!(s.move_to_slot(&[w(1)], A, Some(0), B, Target::New, LATER), MoveResult::Hidden(1));
        assert_eq!((s.count(A), s.count(B)), (1, 2));
        assert_eq!(s.slots(B).1[1], vec![w(1)]);
    }

    #[test]
    fn stale_targets_are_rejected() {
        let mut s = State::default();
        assert_eq!(s.move_to_slot(&[w(1)], A, None, B, Target::Slot(1), 0), MoveResult::Rejected);
        assert_eq!(s.count(B), 1);
        assert!(s.hidden().is_empty());
    }

    #[test]
    fn leaving_extras_are_old_minus_new() {
        assert_eq!(ids(&leaving(&[w(1), w(2)], &[w(2), w(3)])), [1]);
    }

    #[test]
    fn state_file_round_trip() {
        let odd = Entry { id: 2, pid: 3, iconic: true, class: "Has Spaces In It".into() };
        let mut s = State::default();
        new(&mut s, A, vec![w(1), odd.clone()], 0);
        let text = s.serialize(BOOT, &s.tag_active(A, &[w(5)]));
        let (layout, bad) = parse(&text);
        assert!(bad.is_empty());
        assert_eq!(layout.boot, Some(BOOT));
        assert_eq!(layout.active, BTreeMap::from([(A.to_string(), 1)]));
        let a = A.to_string();
        assert_eq!(layout.windows, vec![(a.clone(), 0, w(1)), (a.clone(), 0, odd), (a, 1, w(5))]);
    }

    #[test]
    fn state_file_bad_input() {
        assert_eq!(parse(""), (Layout::default(), vec![]));
        let (layout, bad) = parse("v3\n1 2 0 X\n");
        assert!(layout.windows.is_empty());
        assert_eq!(bad.len(), 1);
        let (layout, bad) = parse("v2 5\nm A 1\nm B\nm C 1 x\nw A z 1 2 0 X\nw A 1 1 2 0 X\ngarbage\n");
        assert_eq!(layout.active.len(), 1);
        assert_eq!(layout.windows.len(), 1);
        assert_eq!(bad.len(), 4);
        // v1 is still read: every entry is to be shown
        let (layout, bad) = parse("v1\n1 2 0 X\ngarbage\n3 x 0 Y\n4 5 7 Z\n5 6 1\n");
        assert_eq!(layout.boot, None);
        assert_eq!(layout.windows.len(), 1);
        assert_eq!(bad.len(), 4);
    }

    #[test]
    fn restore_check() {
        let e = w(1);
        assert!(should_restore(&e, 10, "C1"));
        assert!(!should_restore(&e, 11, "C1"));
        assert!(!should_restore(&e, 10, "Other"));
    }

    #[test]
    fn move_same_monitor_active_slot_is_noop() {
        let mut s = State::default();
        assert_eq!(s.move_to_slot(&[w(1)], A, None, A, Target::Slot(0), 0), MoveResult::Rejected);
        assert!(s.hidden().is_empty());
        // and it leaves the race state alone: a quick switch still captures the window
        let p = new(&mut s, A, vec![w(1)], 10);
        assert_eq!(ids(&p.hide), [1]);
    }

    #[test]
    fn move_same_monitor_other_slot() {
        let mut s = State::default();
        assert_eq!(s.move_to_slot(&[w(1), w(2)], A, None, A, Target::New, 0), MoveResult::Hidden(1));
        let p = s.switch_to(A, Target::Slot(1), vec![], LATER).unwrap();
        assert_eq!(ids(&p.show), [1, 2], "owned entries move with the root");
    }

    #[test]
    fn move_to_other_monitor_active_slot_records_nothing() {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0); // w(1) hidden in A slot 1
        assert_eq!(s.move_to_slot(&[w(1)], A, None, B, Target::Slot(0), LATER), MoveResult::Visible);
        assert!(s.hidden().is_empty());
        assert_eq!(s.count(A), 1, "A's slot emptied by the move is removed");
    }

    #[test]
    fn move_to_other_monitor_hidden_slot() {
        let mut s = State::default();
        assert_eq!(s.move_to_slot(&[w(1)], A, None, B, Target::New, 0), MoveResult::Hidden(1));
        let p = next(&mut s, B, vec![], LATER);
        assert_eq!(ids(&p.show), [1]);
    }

    #[test]
    fn quick_switch_after_move_keeps_new_slot() {
        let mut s = State::default();
        s.move_to_slot(&[w(1)], A, None, A, Target::New, 100);
        // hide hasn't landed: snapshot still lists w(1)
        let p = new(&mut s, A, vec![w(1), w(2)], 200);
        assert_eq!(ids(&p.hide), [2]);
        assert_eq!(s.slots(A).1[1], vec![w(1)]);
    }

    #[test]
    fn quick_switch_on_target_after_cross_monitor_move() {
        let mut s = State::default();
        s.move_to_slot(&[w(1)], A, None, B, Target::Slot(0), 0); // relocation not landed
        let p = new(&mut s, B, vec![], 100);
        assert_eq!(ids(&p.hide), [1], "pending arrival must count as visible on B");
    }

    #[test]
    fn move_does_not_revive_stale_plan() {
        let mut s = State::default();
        new(&mut s, B, vec![w(5)], 0);
        prev(&mut s, B, vec![], LATER); // B's last.show = [w(5)], soon stale
        // much later: user dragged w(5) to A by hand; then a move onto B's active slot
        s.move_to_slot(&[w(1)], A, None, B, Target::Slot(0), 5 * LATER);
        let p = new(&mut s, B, vec![], 5 * LATER + 100);
        assert_eq!(ids(&p.hide), [1], "stale w(5) must not be resurrected");
    }

    /// A: slot 1 [w1], slot 2 [w2], on slot 3 (empty). B: on slot 1, slot 2 [w3].
    fn sample() -> State {
        let mut s = State::default();
        new(&mut s, A, vec![w(1)], 0);
        new(&mut s, A, vec![w(2)], LATER);
        s.move_to_slot(&[w(3)], B, None, B, Target::New, 2 * LATER);
        s
    }

    fn here(_: &str, e: &Entry) -> Check {
        Check::Here(e.clone())
    }

    #[test]
    fn adopt_restores_the_layout() {
        let s = sample();
        let (layout, _) = parse(&s.serialize(BOOT, &[]));
        let (t, show) = adopt(&layout, BOOT + 5_000, &names(), here);
        assert!(show.is_empty());
        assert_eq!(t.slots(A), s.slots(A));
        assert_eq!(t.slots(B), s.slots(B));
        // monitor_set is stored sorted: a later display change with the same monitors keeps it
        let mut t = t;
        assert_eq!(t.reset_if_monitors_changed(&[B.to_string(), A.to_string()]), None);
    }

    #[test]
    fn adopt_skips_closed_and_shows_moved() {
        let (layout, _) = parse(&sample().serialize(BOOT, &[]));
        let (t, show) = adopt(&layout, BOOT, &names(), |_, e| match e.id {
            1 => Check::Gone,
            2 => Check::Elsewhere(e.clone()),
            _ => Check::Here(e.clone()),
        });
        assert_eq!(t.slots(A), (0, vec![vec![]]), "A's slots emptied and removed");
        assert_eq!(show, vec![(A.to_string(), 0, w(2))]);
        assert_eq!(ids(&t.hidden()), [3]);
    }

    #[test]
    fn adopt_shows_instead_of_hiding_when_unsure() {
        let a = A.to_string();
        let layout = Layout {
            boot: Some(BOOT),
            active: BTreeMap::from([(a.clone(), 1)]),
            windows: vec![
                (a.clone(), 1, w(1)),               // in the active slot: belongs on screen
                (a.clone(), 9, w(2)),               // impossible slot
                (B.to_string(), 1, w(3)),           // monitor not connected
                (a.clone(), 0, w(4)),               // adopted
                (a.clone(), 2, w(4)),               // duplicate: first wins
            ],
        };
        let (t, show) = adopt(&layout, BOOT, std::slice::from_ref(&a), here);
        assert_eq!(t.slots(A), (1, vec![vec![w(4)], vec![]]));
        assert_eq!(show, vec![(a.clone(), 1, w(1)), (a, 1, w(2)), (B.to_string(), 0, w(3))]);
    }

    #[test]
    fn adopt_nothing_after_a_reboot_or_from_v1() {
        let (layout, _) = parse(&sample().serialize(BOOT, &[]));
        let (t, show) = adopt(&layout, BOOT + 60_000, &names(), here);
        assert!(t.hidden().is_empty());
        assert_eq!(show.len(), 3);
        let (layout, _) = parse("v1\n1 10 0 C1\n");
        let (t, show) = adopt(&layout, BOOT, &names(), here);
        assert!(t.hidden().is_empty());
        assert_eq!(show.len(), 1);
    }

    /// The startup write-ahead, read back after a crash, shows the same windows and
    /// takes back the same ones.
    #[test]
    fn adopt_write_ahead_is_stable() {
        let mut layout = parse(&sample().serialize(BOOT, &[])).0;
        layout.windows.push((r"\\.\GONE".to_string(), 3, w(7))); // from an unplugged monitor
        let check = |m: &str, e: &Entry| if m.contains("GONE") { Check::Elsewhere(e.clone()) } else { Check::Here(e.clone()) };
        let (t, show) = adopt(&layout, BOOT, &names(), check);
        assert_eq!(show, vec![(r"\\.\GONE".to_string(), 0, w(7))]);
        let (layout2, bad) = parse(&t.serialize(BOOT, &show));
        assert!(bad.is_empty());
        let (t2, show2) = adopt(&layout2, BOOT, &names(), check);
        assert_eq!((t2.slots(A), t2.slots(B)), (t.slots(A), t.slots(B)));
        assert_eq!(show2, show);
    }

    /// Random switches, creates, and moves in a simulated world: no window is ever lost or
    /// duplicated, slots stay within the invariants, and the saved layout reproduces the slots.
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

        for step in 0..5000 {
            now += LATER;
            let mon = mons[rand(2) as usize];
            let target = |rand: &mut dyn FnMut(u64) -> u64, s: &State, m: &str| {
                if rand(4) == 0 { Target::New } else { Target::Slot(rand(s.count(m) as u64 + 1) as usize) }
            };
            let plan = match rand(10) {
                0 => {
                    screen.get_mut(mon).unwrap().insert(0, w(next_id));
                    next_id += 1;
                    None
                }
                1 => {
                    // drag the bottom window to the other monitor by hand
                    let other = if mon == A { B } else { A };
                    if let Some(e) = screen.get_mut(mon).unwrap().pop() {
                        screen.get_mut(other).unwrap().insert(0, e);
                    }
                    None
                }
                2 => {
                    // panel drag: move a visible window to a slot (maybe stale, maybe new)
                    let to = mons[rand(2) as usize];
                    let t = target(&mut rand, &s, to);
                    if let Some(e) = screen[mon].first().cloned() {
                        match s.move_to_slot(std::slice::from_ref(&e), mon, None, to, t, now) {
                            MoveResult::Rejected => {}
                            MoveResult::Hidden(i) => {
                                assert!(contains(&s.slots(to).1[i], e.id));
                                screen.get_mut(mon).unwrap().retain(|v| v.id != e.id);
                                ever_hidden.insert(e.id);
                            }
                            MoveResult::Visible => {
                                screen.get_mut(mon).unwrap().retain(|v| v.id != e.id);
                                screen.get_mut(to).unwrap().insert(0, e);
                            }
                        }
                    }
                    None
                }
                3 => {
                    let t = target(&mut rand, &s, mon);
                    s.switch_to(mon, t, screen[mon].clone(), now)
                }
                _ => {
                    let dir = if rand(2) == 0 { Dir::Prev } else { Dir::Next };
                    s.switch(mon, dir, screen[mon].clone(), now)
                }
            };
            if let Some(p) = plan {
                for e in &p.show {
                    assert!(ever_hidden.contains(&e.id), "showed a window never hidden");
                }
                let scr = screen.get_mut(mon).unwrap();
                scr.retain(|e| !contains(&p.hide, e.id));
                scr.extend(p.show.iter().cloned());
                ever_hidden.extend(p.hide.iter().map(|e| e.id));
            }
            // Invariant: every window is on screen or hidden, exactly once.
            let mut all: Vec<u64> = screen.values().flatten().map(|e| e.id).collect();
            all.extend(ids(&s.hidden()));
            all.sort();
            assert_eq!(all, (1..next_id).collect::<Vec<_>>());
            for m in mons {
                let (active, slots) = s.slots(m);
                assert!(slots.len() <= MAX_SLOTS && active < slots.len());
                assert!(slots.iter().enumerate().all(|(i, s)| i == active || !s.is_empty()), "empty inactive slot");
            }
            if step % 250 == 0 {
                let (layout, bad) = parse(&s.serialize(BOOT, &[]));
                assert!(bad.is_empty());
                let (t, show) = adopt(&layout, BOOT, &names(), here);
                assert!(show.is_empty());
                for m in mons {
                    assert_eq!(t.slots(m), s.slots(m));
                }
            }
        }
        let restored = s.restore_all();
        let total = screen.values().map(Vec::len).sum::<usize>() + restored.len();
        assert_eq!(total as u64, next_id - 1);
    }
}
