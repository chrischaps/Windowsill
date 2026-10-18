//! How much of the sill is there: it eases in when it comes to rest, so it
//! never appears all at once, and eases out when something fullscreen comes in
//! front of it. Stepping aside for a layout it can't rest on hides the window
//! outright, since a sill fading out in the wrong place would be worse than one
//! that is simply gone.

use std::time::{Duration, Instant};

/// How long the sill takes to settle in or fade out. The design floor for any
/// change is 2 s.
const EASE: Duration = Duration::from_millis(2500);

/// Which way the sill is going, from how much of it there was when it turned.
#[derive(Clone, Copy, Default)]
enum Way {
    #[default]
    Away,
    In {
        since: Instant,
        from: f32,
    },
    Out {
        since: Instant,
        from: f32,
    },
}

#[derive(Default)]
pub struct Presence {
    way: Way,
}

impl Presence {
    /// The sill came to rest at `now`, or came back from fading out, and eases
    /// in from however much of it is there.
    pub fn arrive(&mut self, now: Instant) {
        if !matches!(self.way, Way::In { .. }) {
            self.way = Way::In {
                since: now,
                from: self.at(now),
            };
        }
    }

    /// Something came in front of the sill at `now`, and it eases out from
    /// however much of it is there.
    pub fn fade(&mut self, now: Instant) {
        if let Way::In { .. } = self.way {
            self.way = Way::Out {
                since: now,
                from: self.at(now),
            };
        }
    }

    /// The sill stepped aside, all at once.
    pub fn leave(&mut self) {
        self.way = Way::Away;
    }

    /// Whether the sill has faded out, or was never there.
    pub fn gone(&self, now: Instant) -> bool {
        self.at(now) == 0.0 && !matches!(self.way, Way::In { .. })
    }

    /// From 0 (away) to 1 (fully there) at `now`.
    pub fn at(&self, now: Instant) -> f32 {
        let eased = |since: Instant| {
            let t =
                (now.saturating_duration_since(since).as_secs_f32() / EASE.as_secs_f32()).min(1.0);
            t * t * (3.0 - 2.0 * t)
        };
        match self.way {
            Way::Away => 0.0,
            Way::In { since, from } => from + (1.0 - from) * eased(since),
            Way::Out { since, from } => from * (1.0 - eased(since)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One 15 fps frame `n` frames after `start`.
    fn frame(start: Instant, n: u64) -> Instant {
        start + Duration::from_nanos(1_000_000_000 / 15 * n)
    }

    #[test]
    fn away_until_it_arrives() {
        let presence = Presence::default();
        assert_eq!(presence.at(Instant::now()), 0.0);
        assert!(presence.gone(Instant::now()));
    }

    #[test]
    fn eases_in_without_snapping() {
        let start = Instant::now();
        let mut presence = Presence::default();
        presence.arrive(start);
        assert_eq!(presence.at(start), 0.0);
        assert!(!presence.gone(start));
        let mut last = 0.0;
        // One step per 15 fps frame: never more than a small nudge at a time.
        for n in 1..=40 {
            let p = presence.at(frame(start, n));
            assert!(p >= last && p - last < 0.05, "frame {n}: {last} -> {p}");
            last = p;
        }
        assert_eq!(presence.at(start + EASE), 1.0);
        assert_eq!(presence.at(start + EASE * 4), 1.0);
    }

    #[test]
    fn fades_out_without_snapping() {
        let start = Instant::now();
        let mut presence = Presence::default();
        presence.arrive(start);
        let out = start + EASE;
        presence.fade(out);
        assert_eq!(presence.at(out), 1.0);
        let mut last = 1.0;
        for n in 1..=40 {
            let p = presence.at(frame(out, n));
            assert!(p <= last && last - p < 0.05, "frame {n}: {last} -> {p}");
            last = p;
        }
        assert_eq!(presence.at(out + EASE), 0.0);
        assert!(presence.gone(out + EASE));
    }

    #[test]
    fn turns_back_from_wherever_it_is() {
        let start = Instant::now();
        let mut presence = Presence::default();
        presence.arrive(start);
        let out = start + EASE;
        presence.fade(out);
        // Halfway out, the fullscreen app closes again.
        let back = out + EASE / 2;
        let there = presence.at(back);
        presence.arrive(back);
        assert_eq!(presence.at(back), there);
        assert!(presence.at(frame(back, 1)) > there);
        assert_eq!(presence.at(back + EASE), 1.0);
    }

    #[test]
    fn asking_twice_changes_nothing() {
        let start = Instant::now();
        let mut presence = Presence::default();
        presence.arrive(start);
        presence.arrive(start + EASE / 2);
        assert_eq!(presence.at(start + EASE), 1.0);
        let out = start + EASE;
        presence.fade(out);
        presence.fade(out + EASE / 2);
        assert_eq!(presence.at(out + EASE), 0.0);
    }

    #[test]
    fn leaving_starts_over() {
        let start = Instant::now();
        let mut presence = Presence::default();
        presence.arrive(start);
        presence.leave();
        assert_eq!(presence.at(start + EASE), 0.0);
        assert!(presence.gone(start));
    }
}
