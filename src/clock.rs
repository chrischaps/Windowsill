//! The world's clock: the real one, or a day sped up or stood still, to check
//! how the sill looks at any hour without waiting for it.
//!
//! Only the sky's time is changed. The sun, the moon and the candle read
//! [`Clock::now`], and the ease runs at [`Clock::pace`], so a day sped up a
//! thousandfold eases a thousand times faster too and looks the way a real day
//! would, only shorter. The window's own comings and goings, and the candle's
//! flicker, keep real time: they belong to the room, not the sky.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DAY: f64 = 86_400.0;

/// When the world is, at any moment of real time.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// A real moment, and when the world was then.
    from: Instant,
    start: SystemTime,
    /// World seconds per real second: 1 for the real clock, 0 when stopped.
    speed: f64,
}

impl Clock {
    /// The real clock.
    pub fn real() -> Self {
        Self::new(SystemTime::now(), 1.0)
    }

    /// A clock that starts at `start` and runs a whole day in `day_in`.
    pub fn day_in(start: SystemTime, day_in: Duration) -> Self {
        Self::new(start, DAY / day_in.as_secs_f64())
    }

    /// A clock stopped at `at`.
    pub fn stopped(at: SystemTime) -> Self {
        Self::new(at, 0.0)
    }

    fn new(start: SystemTime, speed: f64) -> Self {
        Self {
            from: Instant::now(),
            start,
            speed,
        }
    }

    /// When the world is at the real moment `at`.
    pub fn now(&self, at: Instant) -> SystemTime {
        let real = at.saturating_duration_since(self.from).as_secs_f64();
        self.start + Duration::from_secs_f64(real * self.speed)
    }

    /// How many seconds of easing a real second is worth. A stopped clock
    /// still eases at real speed, so an edit to the config is followed.
    pub fn pace(&self) -> f32 {
        if self.speed > 0.0 {
            self.speed as f32
        } else {
            1.0
        }
    }

    /// Whether this is the real clock.
    pub fn is_real(&self) -> bool {
        self.speed == 1.0
    }
}

/// The clock time `hours:minutes` today, `offset` seconds east of UTC, as
/// it is at `now`.
pub fn today_at(now: SystemTime, offset: i64, hours: u32, minutes: u32) -> SystemTime {
    let local = seconds(now) + offset;
    let midnight = local.div_euclid(DAY as i64) * DAY as i64 - offset;
    let at = midnight + i64::from(hours * 3600 + minutes * 60);
    UNIX_EPOCH + Duration::from_secs(at.max(0) as u64)
}

/// The clock time at `at`, `offset` seconds east of UTC, as `(hours, minutes)`.
pub fn time_of_day(at: SystemTime, offset: i64) -> (u32, u32) {
    let minute = (seconds(at) + offset).div_euclid(60).rem_euclid(24 * 60) as u32;
    (minute / 60, minute % 60)
}

fn seconds(at: SystemTime) -> i64 {
    match at.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sun::tests::utc;

    #[test]
    fn the_real_clock_keeps_real_time() {
        let clock = Clock::real();
        let later = clock.now(clock.from + Duration::from_secs(90));
        let gap = later.duration_since(clock.start).unwrap();
        assert_eq!(gap, Duration::from_secs(90));
        assert!(clock.is_real());
        assert_eq!(clock.pace(), 1.0);
    }

    #[test]
    fn a_day_in_a_minute_runs_a_day_in_a_minute() {
        let start = utc(2026, 6, 21, 0.0);
        let clock = Clock::day_in(start, Duration::from_secs(60));
        assert_eq!(
            clock.now(clock.from + Duration::from_secs(30)),
            utc(2026, 6, 21, 12.0)
        );
        assert_eq!(
            clock.now(clock.from + Duration::from_secs(60)),
            utc(2026, 6, 22, 0.0)
        );
        assert_eq!(clock.pace(), 1440.0);
        assert!(!clock.is_real());
    }

    #[test]
    fn a_stopped_clock_stays_put_but_still_eases() {
        let at = utc(2026, 12, 21, 6.5);
        let clock = Clock::stopped(at);
        assert_eq!(clock.now(clock.from + Duration::from_secs(3600)), at);
        assert_eq!(clock.pace(), 1.0);
    }

    #[test]
    fn today_at_is_on_the_local_day() {
        let five_hours_west = -5 * 3600;
        // 02:00 UTC on the 21st is still the evening of the 20th, five hours west.
        let now = utc(2026, 6, 21, 2.0);
        assert_eq!(
            today_at(now, five_hours_west, 6, 30),
            utc(2026, 6, 20, 11.5)
        );
        assert_eq!(today_at(now, 0, 6, 30), utc(2026, 6, 21, 6.5));
        let ninety_minutes_east = 90 * 60;
        assert_eq!(
            today_at(now, ninety_minutes_east, 0, 0),
            utc(2026, 6, 20, 22.5)
        );
    }

    #[test]
    fn time_of_day_reads_the_local_clock() {
        let at = utc(2026, 6, 21, 2.25);
        assert_eq!(time_of_day(at, 0), (2, 15));
        assert_eq!(time_of_day(at, -5 * 3600), (21, 15));
        assert_eq!(time_of_day(today_at(at, 3600, 6, 30), 3600), (6, 30));
    }
}
