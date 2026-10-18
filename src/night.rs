//! What the night brings to the sill: the moon's light, and the candle.
//!
//! The candle lights itself at sunset and burns down through the night, a
//! little shorter each hour, until it is a stub at sunrise. Then it goes out,
//! and by the time anyone looks it is a fresh one. The moon's light is cool
//! and dim, as much as its phase gives, and only while it is up.

use std::time::{Duration, SystemTime};

use crate::light::Room;
use crate::moon::Moon;
use crate::sill::SillState;
use crate::sun::{self, Place, Sun};

/// How much of the candle is left at sunrise, after a whole night.
pub const STUB: f32 = 0.4;

/// How long a night is taken to be when the sun gives no sunrise to end it.
const LONG_NIGHT: Duration = Duration::from_secs(12 * 3600);

const DAY: Duration = Duration::from_secs(86_400);

/// The candle at some moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candle {
    pub lit: bool,
    /// How much of it is left, from [`STUB`] to 1 (fresh).
    pub wax: f32,
}

impl Candle {
    /// Unlit and whole, as it is all day.
    pub const FRESH: Self = Self {
        lit: false,
        wax: 1.0,
    };
}

/// The candle at `at`: lit from sunset to sunrise, and burnt down by the
/// share of the night that has passed.
pub fn candle(place: Place, at: SystemTime) -> Candle {
    let night = match sun::day(place, at) {
        sun::Day::Rises { set, .. } if at >= set => {
            let rise = match sun::day(place, at + DAY) {
                sun::Day::Rises { rise, .. } => rise,
                _ => set + LONG_NIGHT,
            };
            (set, rise)
        }
        sun::Day::Rises { rise, .. } if at < rise => match sun::day(place, at - DAY) {
            sun::Day::Rises { set, .. } => (set, rise),
            _ => (rise - LONG_NIGHT, rise),
        },
        sun::Day::Rises { .. } | sun::Day::NeverSets => return Candle::FRESH,
        // A polar night: the candle burns on, half down.
        sun::Day::NeverRises => {
            return Candle {
                lit: true,
                wax: 1.0 - 0.5 * (1.0 - STUB),
            };
        }
    };
    let (set, rise) = night;
    let length = rise.duration_since(set).unwrap_or(LONG_NIGHT).as_secs_f64();
    let gone = at.duration_since(set).unwrap_or_default().as_secs_f64();
    let burnt = (gone / length).clamp(0.0, 1.0) as f32;
    Candle {
        lit: true,
        wax: 1.0 - burnt * (1.0 - STUB),
    }
}

/// `state` with its candle and moonlight set for `candle` and `moon`, under
/// `sun`, as they fall in `room`.
pub fn by_night(state: SillState, room: Room, sun: Sun, moon: Moon, candle: Candle) -> SillState {
    // The moon's brightness falls much faster than its lit share: a half
    // moon gives about a tenth of a full one's light. Squared is gentler than
    // that, so a half moon still shows, but a new moon gives almost nothing.
    let bright = moon.lit * moon.lit;
    let up = smoothstep(-1.0, 6.0, moon.elevation);
    // It only matters once the sky has gone dark.
    let dark = 1.0 - smoothstep(-12.0, -2.0, sun.elevation);
    // Straight in through the window, or softly off the moonlit sky.
    let through = 0.35 + 0.65 * room.sunshine(moon.azimuth) as f64;
    SillState {
        candle: if candle.lit { 1.0 } else { 0.0 },
        wax: candle.wax,
        moonlight: (bright * up * dark * through) as f32,
        moon_side: room.shadow_side(moon.azimuth),
        ..state
    }
}

/// How the flame breathes `seconds` in: a slow wander in its height and a
/// slower sway of its tip, each from -1 to 1. It is a few slow waves at about
/// the pace of breathing, so it never strobes.
pub fn flicker(seconds: f64) -> [f32; 2] {
    let wave = |hz: f64, phase: f64| (std::f64::consts::TAU * hz * seconds + phase).sin();
    let height = 0.5 * wave(0.23, 0.0) + 0.3 * wave(0.37, 1.3) + 0.2 * wave(0.11, 4.0);
    let sway = 0.6 * wave(0.17, 2.1) + 0.4 * wave(0.29, 5.2);
    [height as f32, sway as f32]
}

fn smoothstep(from: f64, to: f64, x: f64) -> f64 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sun::tests::utc;

    const SEATTLE: Place = Place {
        latitude: 47.6062,
        longitude: -122.3321,
    };
    const ROOM: Room = Room {
        facing: 180.0,
        window: None,
    };

    fn set_on(place: Place, at: SystemTime) -> (SystemTime, SystemTime) {
        match sun::day(place, at) {
            sun::Day::Rises { rise, set } => (rise, set),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn it_lights_within_a_minute_of_sunset() {
        let (_, set) = set_on(SEATTLE, utc(2026, 10, 18, 20.0));
        let minute = Duration::from_secs(60);
        assert!(!candle(SEATTLE, set - minute).lit);
        assert!(candle(SEATTLE, set + minute).lit);
        assert!(candle(SEATTLE, set + minute).wax > 0.999);
    }

    #[test]
    fn it_burns_down_through_the_night_and_is_fresh_by_morning() {
        let (_, set) = set_on(SEATTLE, utc(2026, 10, 18, 20.0));
        let mut last = 1.0;
        for hour in 1..=13 {
            let now = candle(SEATTLE, set + Duration::from_secs(hour * 3600));
            assert!(now.lit, "hour {hour}");
            assert!(now.wax < last, "hour {hour}: {} after {last}", now.wax);
            last = now.wax;
        }
        let (rise, _) = set_on(SEATTLE, utc(2026, 10, 19, 20.0));
        let before = candle(SEATTLE, rise - Duration::from_secs(60));
        assert!(before.lit && (before.wax - STUB).abs() < 0.01, "{before:?}");
        assert_eq!(
            candle(SEATTLE, rise + Duration::from_secs(60)),
            Candle::FRESH
        );
    }

    #[test]
    fn after_midnight_it_carries_on_from_the_evening() {
        // 23:30 and 00:30 local, either side of the day's turn.
        let evening = candle(SEATTLE, utc(2026, 10, 19, 6.5));
        let night = candle(SEATTLE, utc(2026, 10, 19, 7.5));
        assert!(evening.lit && night.lit);
        assert!(night.wax < evening.wax && evening.wax - night.wax < 0.06);
    }

    #[test]
    fn polar_days_and_nights() {
        let tromso = Place {
            latitude: 69.6492,
            longitude: 18.9553,
        };
        assert!(!candle(tromso, utc(2026, 6, 21, 12.0)).lit);
        assert!(candle(tromso, utc(2026, 12, 21, 12.0)).lit);
    }

    fn sun(elevation: f64) -> Sun {
        Sun {
            elevation,
            azimuth: 0.0,
        }
    }

    fn moon(elevation: f64, lit: f64) -> Moon {
        Moon {
            elevation,
            azimuth: 180.0,
            lit,
        }
    }

    fn moonlight(sun_up: f64, moon: Moon) -> f32 {
        by_night(SillState::default(), ROOM, sun(sun_up), moon, Candle::FRESH).moonlight
    }

    #[test]
    fn a_full_moon_lights_the_night_and_a_new_one_barely_does() {
        let full = moonlight(-30.0, moon(40.0, 1.0));
        let half = moonlight(-30.0, moon(40.0, 0.5));
        let new = moonlight(-30.0, moon(40.0, 0.01));
        assert!(full > 0.99, "{full}");
        assert!(half > 0.15 && half < 0.4, "{half}");
        assert!(new < 0.001, "{new}");
    }

    #[test]
    fn no_moonlight_while_it_is_down_or_the_sun_is_up() {
        assert_eq!(moonlight(-30.0, moon(-5.0, 1.0)), 0.0);
        assert_eq!(moonlight(20.0, moon(40.0, 1.0)), 0.0);
    }

    #[test]
    fn the_flame_breathes_slowly() {
        let step = 1.0 / 30.0;
        let mut last = flicker(0.0);
        for frame in 1..30 * 120 {
            let now = flicker(frame as f64 * step);
            for (a, b) in now.iter().zip(last) {
                assert!(a.abs() <= 1.0);
                // Never faster than one full swing a second.
                assert!((a - b).abs() < 2.0 * step as f32, "frame {frame}");
            }
            last = now;
        }
    }
}
