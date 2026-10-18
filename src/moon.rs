//! Where the real moon is, and how much of it is lit.
//!
//! The position comes from the Astronomical Almanac's low-precision lunar
//! series: six terms in longitude, four in latitude and five in parallax,
//! good to a few tenths of a degree. The phase is the moon's true angle from
//! the sun, not the mean synodic month, which can be half a day out at a
//! quarter. Like `sun.rs`, it is pure arithmetic on std.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::sun::{self, Place};

const DAY: f64 = 86_400.0;

/// Where the moon is in the sky, and its phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moon {
    /// Above the horizon as the eye sees it, in degrees, from where you
    /// stand rather than the earth's centre (the moon is close enough for
    /// that to be a degree).
    pub elevation: f64,
    /// Clockwise from north, in degrees.
    pub azimuth: f64,
    /// How much of its face is lit, from 0 at new to 1 at full.
    pub lit: f64,
}

/// Where the moon is from `place` at `at`.
pub fn position(place: Place, at: SystemTime) -> Moon {
    let d = days(at);
    let t = d / 36_525.0;
    let s = |base: f64, rate: f64| (base + rate * t).to_radians().sin();
    let c = |base: f64, rate: f64| (base + rate * t).to_radians().cos();

    // Ecliptic longitude and latitude, and the horizontal parallax.
    let longitude = 218.32 + 481_267.881 * t + 6.29 * s(135.0, 477_198.87)
        - 1.27 * s(259.3, -413_335.36)
        + 0.66 * s(235.7, 890_534.22)
        + 0.21 * s(269.9, 954_397.74)
        - 0.19 * s(357.5, 35_999.05)
        - 0.11 * s(186.5, 966_404.03);
    let latitude = 5.13 * s(93.3, 483_202.02) + 0.28 * s(228.2, 960_400.89)
        - 0.28 * s(318.3, 6_003.15)
        - 0.17 * s(217.6, -407_332.21);
    let parallax = 0.9508
        + 0.0518 * c(134.9, 477_198.85)
        + 0.0095 * c(259.2, -413_335.38)
        + 0.0078 * c(235.7, 890_534.23)
        + 0.0028 * c(269.9, 954_397.70);

    // Onto the sky's equator.
    let (lam, bet) = (longitude.to_radians(), latitude.to_radians());
    let obliq = (23.439 - 0.000_000_4 * d).to_radians();
    let x = bet.cos() * lam.cos();
    let y = obliq.cos() * bet.cos() * lam.sin() - obliq.sin() * bet.sin();
    let z = obliq.sin() * bet.cos() * lam.sin() + obliq.cos() * bet.sin();
    let ascension = y.atan2(x);
    let dec = z.clamp(-1.0, 1.0).asin();

    // Then onto the sky over `place`, the way `sun::position` does.
    let sidereal = 280.460_618_37 + 360.985_647_366_29 * d + place.longitude;
    let hour = sidereal.to_radians() - ascension;
    let lat = place.latitude.to_radians();
    let geocentric = (lat.sin() * dec.sin() + lat.cos() * dec.cos() * hour.cos())
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees();
    let azimuth = (hour
        .sin()
        .atan2(hour.cos() * lat.sin() - dec.tan() * lat.cos())
        .to_degrees()
        + 180.0)
        .rem_euclid(360.0);
    // Seen from the surface rather than the centre, it sits a little lower.
    let elevation = geocentric - parallax * geocentric.to_radians().cos();

    // The phase, from how far round from the sun it is.
    let elongation = (bet.cos() * (lam - sun_longitude(d).to_radians()).cos()).acos();
    Moon {
        elevation: elevation + sun::refraction(elevation),
        azimuth,
        lit: (1.0 - elongation.cos()) / 2.0,
    }
}

/// The sun's apparent ecliptic longitude `d` days from J2000, in degrees,
/// to about a hundredth of a degree.
fn sun_longitude(d: f64) -> f64 {
    let mean = 280.460 + 0.985_647_4 * d;
    let anomaly = (357.528 + 0.985_600_3 * d).to_radians();
    mean + 1.915 * anomaly.sin() + 0.020 * (2.0 * anomaly).sin()
}

/// Days since J2000 (2000-01-01 12:00 TT, near enough UTC for this).
fn days(at: SystemTime) -> f64 {
    let s = match at.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    };
    s / DAY - 10_957.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sun::tests::utc;

    // Reference values come from Skyfield with the JPL DE421 ephemeris.

    fn place(latitude: f64, longitude: f64) -> Place {
        Place {
            latitude,
            longitude,
        }
    }

    /// Anywhere will do for the phase; it is the same the world over.
    const ANYWHERE: Place = Place {
        latitude: 0.0,
        longitude: 0.0,
    };

    #[test]
    fn the_phases_match_the_calendar() {
        // The phases from now to the end of 2026, in UTC.
        for (y, m, d, h, min, lit) in [
            (2026, 10, 10, 15, 50, 0.0),
            (2026, 10, 18, 16, 13, 0.5),
            (2026, 10, 26, 4, 12, 1.0),
            (2026, 11, 1, 20, 28, 0.5),
            (2026, 11, 9, 7, 2, 0.0),
            (2026, 11, 17, 11, 48, 0.5),
            (2026, 11, 24, 14, 54, 1.0),
            (2026, 12, 1, 6, 9, 0.5),
            (2026, 12, 9, 0, 52, 0.0),
            (2026, 12, 17, 5, 43, 0.5),
            (2026, 12, 24, 1, 28, 1.0),
        ] {
            let at = utc(y, m, d, h as f64 + min as f64 / 60.0);
            let moon = position(ANYWHERE, at);
            // A quarter's lit share changes about 1.7% an hour, so this is
            // within an hour or so either way.
            assert!(
                (moon.lit - lit).abs() < 0.02,
                "{y}-{m}-{d}: {} lit, expected {lit}",
                moon.lit
            );
        }
    }

    #[test]
    fn it_waxes_and_wanes() {
        let new = utc(2026, 11, 9, 7.0);
        let mut last = position(ANYWHERE, new).lit;
        for day in 1..=15 {
            let lit = position(ANYWHERE, utc(2026, 11, 9 + day, 7.0)).lit;
            assert!(lit > last, "day {day}: {lit} after {last}");
            last = lit;
        }
        let after_full = position(ANYWHERE, utc(2026, 11, 26, 7.0)).lit;
        assert!(after_full < last);
    }

    fn moon_near(at: Place, when: SystemTime, elevation: f64, azimuth: f64, lit: f64) {
        let moon = position(at, when);
        let bearing = (moon.azimuth - azimuth + 180.0).rem_euclid(360.0) - 180.0;
        assert!(
            (moon.elevation - elevation).abs() < 0.7
                && bearing.abs() < 1.0
                && (moon.lit - lit).abs() < 0.01,
            "{at:?}: got {moon:?}, expected elevation {elevation} azimuth {azimuth} lit {lit}"
        );
    }

    #[test]
    fn full_moon_high_over_seattle_at_midnight() {
        moon_near(
            place(47.6062, -122.3321),
            utc(2026, 10, 26, 8.0),
            59.472,
            182.39,
            0.998,
        );
    }

    #[test]
    fn first_quarter_in_the_south_over_london_at_dusk() {
        moon_near(
            place(51.5074, -0.1278),
            utc(2026, 11, 17, 18.0),
            25.816,
            175.969,
            0.5266,
        );
    }

    #[test]
    fn christmas_eve_full_moon_in_the_north_over_sydney() {
        moon_near(
            place(-33.8688, 151.2093),
            utc(2026, 12, 24, 12.0),
            19.746,
            33.955,
            0.9961,
        );
    }

    #[test]
    fn a_half_moon_setting_in_the_west_over_quito() {
        moon_near(
            place(-0.1807, -78.4678),
            utc(2026, 10, 18, 3.0),
            24.104,
            241.953,
            0.4492,
        );
    }
}
