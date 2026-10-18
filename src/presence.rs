//! How much of the sill is there: it eases in when it comes to rest, so it
//! never appears all at once. Stepping aside hides the window outright, since a
//! sill fading out in the wrong place would be worse than one that is simply gone.

use std::time::{Duration, Instant};

/// How long the sill takes to settle in. The design floor for any change is 2 s.
const ARRIVE: Duration = Duration::from_millis(2500);

#[derive(Default)]
pub struct Presence {
    since: Option<Instant>,
}

impl Presence {
    /// The sill came to rest at `now`, and starts easing in from nothing.
    pub fn arrive(&mut self, now: Instant) {
        self.since = Some(now);
    }

    /// The sill stepped aside.
    pub fn leave(&mut self) {
        self.since = None;
    }

    /// From 0 (away) to 1 (fully there) at `now`.
    pub fn at(&self, now: Instant) -> f32 {
        let Some(since) = self.since else {
            return 0.0;
        };
        let t = (now.saturating_duration_since(since).as_secs_f32() / ARRIVE.as_secs_f32()).min(1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn away_until_it_arrives() {
        assert_eq!(Presence::default().at(Instant::now()), 0.0);
    }

    #[test]
    fn eases_in_without_snapping() {
        let start = Instant::now();
        let mut presence = Presence::default();
        presence.arrive(start);
        assert_eq!(presence.at(start), 0.0);
        let mut last = 0.0;
        // One step per 15 fps frame: never more than a small nudge at a time.
        for frame in 1..=40 {
            let now = start + Duration::from_nanos(1_000_000_000 / 15 * frame);
            let p = presence.at(now);
            assert!(p >= last && p - last < 0.05, "frame {frame}: {last} -> {p}");
            last = p;
        }
        assert_eq!(presence.at(start + ARRIVE), 1.0);
        assert_eq!(presence.at(start + ARRIVE * 4), 1.0);
    }

    #[test]
    fn leaving_starts_over() {
        let start = Instant::now();
        let mut presence = Presence::default();
        presence.arrive(start);
        presence.leave();
        assert_eq!(presence.at(start + ARRIVE), 0.0);
    }
}
