//! When to look at the taskbar again after the desktop changes.
//!
//! Windows broadcasts a display, DPI or settings change before Explorer has
//! finished moving the taskbar to match, so one look straight away can see the
//! old layout. The sill looks once at once, then again a little later, and
//! then stops looking until the next change. No polling in between.

use std::time::{Duration, Instant};

/// How long after a change to look again.
const AFTER: [Duration; 2] = [Duration::from_millis(500), Duration::from_secs(2)];

/// The looks still to take after the most recent change.
#[derive(Default)]
pub struct Settle {
    pending: Vec<Instant>,
}

impl Settle {
    /// Something changed at `now`. Any looks from an earlier change are replaced.
    pub fn poke(&mut self, now: Instant) {
        self.pending = AFTER.iter().map(|after| now + *after).collect();
    }

    /// Whether a look is due at `now`. Each one is taken only once.
    pub fn due(&mut self, now: Instant) -> bool {
        let before = self.pending.len();
        self.pending.retain(|at| *at > now);
        self.pending.len() != before
    }

    /// When the next look is due, if any is left.
    pub fn next(&self) -> Option<Instant> {
        self.pending.iter().min().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_until_poked() {
        let mut settle = Settle::default();
        assert_eq!(settle.next(), None);
        assert!(!settle.due(Instant::now()));
    }

    #[test]
    fn looks_twice_after_a_change_then_rests() {
        let start = Instant::now();
        let mut settle = Settle::default();
        settle.poke(start);

        assert!(
            !settle.due(start),
            "nothing due at the moment of the change"
        );
        assert_eq!(settle.next(), Some(start + AFTER[0]));

        assert!(settle.due(start + AFTER[0]));
        assert!(!settle.due(start + AFTER[0]), "each look is taken once");
        assert_eq!(settle.next(), Some(start + AFTER[1]));

        assert!(settle.due(start + AFTER[1]));
        assert_eq!(settle.next(), None);
    }

    #[test]
    fn a_later_change_pushes_the_looks_back() {
        let start = Instant::now();
        let mut settle = Settle::default();
        settle.poke(start);
        let later = start + Duration::from_millis(400);
        settle.poke(later);
        assert!(!settle.due(start + AFTER[0]));
        assert_eq!(settle.next(), Some(later + AFTER[0]));
    }

    #[test]
    fn a_late_wake_takes_every_overdue_look_at_once() {
        let start = Instant::now();
        let mut settle = Settle::default();
        settle.poke(start);
        assert!(settle.due(start + Duration::from_secs(10)));
        assert_eq!(settle.next(), None);
    }
}
