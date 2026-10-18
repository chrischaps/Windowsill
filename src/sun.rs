//! Where the real sun is, from NOAA's solar calculator equations.
//!
//! Given a place and a moment, this gives the sun's elevation above the horizon
//! and its compass bearing, plus the day's sunrise and sunset. It is pure
//! arithmetic on std, good to well under a degree and a minute for any date
//! this sill will see.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DAY: f64 = 86_400.0;

/// The sun's centre is this far below the horizon at sunrise and sunset, once
/// refraction and the sun's own radius are counted.
const HORIZON: f64 = 90.833;

/// Where the sill is in the world. North and east are positive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub latitude: f64,
    pub longitude: f64,
}

/// Where the sun is in the sky, in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sun {
    /// Above the horizon, as the eye sees it (refraction included). Negative
    /// at night.
    pub elevation: f64,
    /// Clockwise from north.
    pub azimuth: f64,
}

/// The sun's day at a place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Day {
    Rises {
        rise: SystemTime,
        set: SystemTime,
    },
    /// Midnight sun: it never sets.
    NeverSets,
    /// Polar night: it never rises.
    NeverRises,
}

/// Where the sun is from `place` at `at`.
pub fn position(place: Place, at: SystemTime) -> Sun {
    let s = seconds(at);
    let (declination, equation) = orbit(s);

    // True solar time in minutes, then the hour angle: 0° at solar noon.
    let solar = (s.rem_euclid(DAY) / 60.0 + equation + 4.0 * place.longitude).rem_euclid(1440.0);
    let hour = (solar / 4.0 - 180.0).to_radians();

    let lat = place.latitude.to_radians();
    let dec = declination.to_radians();
    let elevation = (lat.sin() * dec.sin() + lat.cos() * dec.cos() * hour.cos())
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees();
    let azimuth = (hour
        .sin()
        .atan2(hour.cos() * lat.sin() - dec.tan() * lat.cos())
        .to_degrees()
        + 180.0)
        .rem_euclid(360.0);

    Sun {
        elevation: elevation + refraction(elevation),
        azimuth,
    }
}

/// Sunrise and sunset on the local solar day that holds `at`.
pub fn day(place: Place, at: SystemTime) -> Day {
    // Whole days since the epoch, counted in the place's mean solar time.
    let date = (seconds(at) / DAY + place.longitude / 360.0).floor();
    let rise = event(place, date, -1.0);
    let set = event(place, date, 1.0);
    match (rise, set) {
        (Ok(rise), Ok(set)) => Day::Rises {
            rise: time(rise),
            set: time(set),
        },
        (Err(polar), _) | (_, Err(polar)) => polar,
    }
}

/// When the sun crosses the horizon on `date`: before noon for `side` -1,
/// after for +1. The sun's path is worked out again at each guess, since it
/// shifts a little over half a day.
fn event(place: Place, date: f64, side: f64) -> Result<f64, Day> {
    let mean_noon = (date + 0.5 - place.longitude / 360.0) * DAY;
    let lat = place.latitude.to_radians();
    let mut at = mean_noon;
    for _ in 0..3 {
        let (declination, equation) = orbit(at);
        let dec = declination.to_radians();
        let cos_hour =
            (HORIZON.to_radians().cos() - lat.sin() * dec.sin()) / (lat.cos() * dec.cos());
        if cos_hour > 1.0 {
            return Err(Day::NeverRises);
        }
        if cos_hour < -1.0 {
            return Err(Day::NeverSets);
        }
        // Four minutes of time per degree of hour angle.
        at = mean_noon - equation * 60.0 + side * cos_hour.acos().to_degrees() * 240.0;
    }
    Ok(at)
}

/// The sun's declination (degrees) and the equation of time (minutes) at `s`
/// seconds since the epoch.
fn orbit(s: f64) -> (f64, f64) {
    // Julian centuries since J2000.
    let t = (s / DAY + 2_440_587.5 - 2_451_545.0) / 36_525.0;

    let mean_long = (280.46646 + t * (36000.76983 + t * 0.0003032)).rem_euclid(360.0);
    let mean_anom = 357.52911 + t * (35999.05029 - 0.0001537 * t);
    let ecc = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
    let m = mean_anom.to_radians();
    let centre = m.sin() * (1.914602 - t * (0.004817 + 0.000014 * t))
        + (2.0 * m).sin() * (0.019993 - 0.000101 * t)
        + (3.0 * m).sin() * 0.000289;
    let omega = (125.04 - 1934.136 * t).to_radians();
    let apparent = (mean_long + centre - 0.00569 - 0.00478 * omega.sin()).to_radians();

    let mean_obliq =
        23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60.0) / 60.0;
    let obliq = (mean_obliq + 0.00256 * omega.cos()).to_radians();
    let declination = (obliq.sin() * apparent.sin()).asin().to_degrees();

    let y = (obliq / 2.0).tan().powi(2);
    let l = mean_long.to_radians();
    let equation = 4.0
        * (y * (2.0 * l).sin() - 2.0 * ecc * m.sin() + 4.0 * ecc * y * m.sin() * (2.0 * l).cos()
            - 0.5 * y * y * (4.0 * l).sin()
            - 1.25 * ecc * ecc * (2.0 * m).sin())
        .to_degrees();

    (declination, equation)
}

/// How far the air lifts the sun at true `elevation`, in degrees.
fn refraction(elevation: f64) -> f64 {
    let te = elevation.to_radians().tan();
    let arcsec = if elevation > 85.0 {
        0.0
    } else if elevation > 5.0 {
        58.1 / te - 0.07 / te.powi(3) + 0.000086 / te.powi(5)
    } else if elevation > -0.575 {
        1735.0
            + elevation * (-518.2 + elevation * (103.4 + elevation * (-12.79 + elevation * 0.711)))
    } else {
        -20.774 / te
    };
    arcsec / 3600.0
}

fn seconds(at: SystemTime) -> f64 {
    match at.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    }
}

fn time(s: f64) -> SystemTime {
    if s >= 0.0 {
        UNIX_EPOCH + Duration::from_secs_f64(s)
    } else {
        UNIX_EPOCH - Duration::from_secs_f64(-s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values come from Skyfield with the JPL DE421 ephemeris, unless
    // a test says otherwise.

    /// A UTC moment, from a civil date.
    fn utc(y: i64, m: i64, d: i64, h: f64) -> SystemTime {
        // Days from the epoch to a civil date (Howard Hinnant's algorithm).
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        time(days as f64 * DAY + h * 3600.0)
    }

    fn place(latitude: f64, longitude: f64) -> Place {
        Place {
            latitude,
            longitude,
        }
    }

    fn sun_near(at: Place, when: SystemTime, elevation: f64, azimuth: f64) {
        let sun = position(at, when);
        let bearing = (sun.azimuth - azimuth + 180.0).rem_euclid(360.0) - 180.0;
        assert!(
            (sun.elevation - elevation).abs() < 0.5 && bearing.abs() < 0.5,
            "{at:?}: got {sun:?}, expected elevation {elevation} azimuth {azimuth}"
        );
    }

    fn rises_near(at: Place, when: SystemTime, rise: SystemTime, set: SystemTime) {
        let Day::Rises { rise: r, set: s } = day(at, when) else {
            panic!("{at:?}: expected a sunrise and sunset");
        };
        for (got, want) in [(r, rise), (s, set)] {
            let off = (seconds(got) - seconds(want)).abs();
            assert!(off < 120.0, "{at:?}: {off:.0} s out ({got:?} vs {want:?})");
        }
    }

    #[test]
    fn equinox_midday_in_seattle() {
        sun_near(
            place(47.6062, -122.3321),
            utc(2026, 3, 20, 20.0),
            42.358,
            174.359,
        );
    }

    #[test]
    fn june_solstice_noon_in_sydney_faces_north() {
        sun_near(
            place(-33.8688, 151.2093),
            utc(2026, 6, 21, 2.0),
            32.712,
            359.152,
        );
    }

    #[test]
    fn equinox_morning_on_the_equator_rises_due_east() {
        sun_near(
            place(-0.1807, -78.4678),
            utc(2026, 3, 20, 15.0),
            54.692,
            89.738,
        );
    }

    #[test]
    fn december_solstice_noon_in_reykjavik_barely_clears_the_horizon() {
        sun_near(
            place(64.1466, -21.9426),
            utc(2026, 12, 21, 13.0),
            2.534,
            174.067,
        );
    }

    #[test]
    fn midnight_sun_in_tromso() {
        sun_near(
            place(69.6492, 18.9553),
            utc(2026, 6, 21, 22.0),
            3.661,
            349.421,
        );
    }

    #[test]
    fn late_afternoon_in_london_is_low_in_the_west() {
        sun_near(
            place(51.5074, -0.1278),
            utc(2026, 9, 1, 17.5),
            11.129,
            269.117,
        );
    }

    #[test]
    fn matches_nrel_spa_published_example() {
        // Reda & Andreas, "Solar Position Algorithm" (NREL, 2008), table A4.1:
        // zenith 50.11162°, azimuth 194.34024° at 2003-10-17 12:30:30 MST.
        let golden = place(39.742476, -105.1786);
        sun_near(
            golden,
            utc(2003, 10, 17, 19.0 + 30.5 / 60.0),
            90.0 - 50.11162,
            194.34024,
        );
    }

    #[test]
    fn below_the_horizon_at_night() {
        let sun = position(place(51.5074, -0.1278), utc(2026, 1, 1, 0.0));
        assert!(sun.elevation < -50.0, "{sun:?}");
    }

    #[test]
    fn seattle_equinox_sunrise_and_sunset() {
        rises_near(
            place(47.6062, -122.3321),
            utc(2026, 3, 20, 20.0),
            utc(2026, 3, 20, 14.0 + 11.0 / 60.0 + 51.0 / 3600.0),
            utc(2026, 3, 21, 2.0 + 22.0 / 60.0 + 21.0 / 3600.0),
        );
    }

    #[test]
    fn sydney_winter_sunrise_and_sunset_cross_utc_midnight() {
        rises_near(
            place(-33.8688, 151.2093),
            utc(2026, 6, 20, 2.0),
            utc(2026, 6, 19, 20.0 + 59.0 / 60.0 + 43.0 / 3600.0),
            utc(2026, 6, 20, 6.0 + 53.0 / 60.0 + 36.0 / 3600.0),
        );
    }

    #[test]
    fn reykjavik_shortest_day() {
        rises_near(
            place(64.1466, -21.9426),
            utc(2026, 12, 21, 13.0),
            utc(2026, 12, 21, 11.0 + 22.0 / 60.0 + 21.0 / 3600.0),
            utc(2026, 12, 21, 15.0 + 29.0 / 60.0 + 22.0 / 3600.0),
        );
    }

    #[test]
    fn london_longest_day() {
        rises_near(
            place(51.5074, -0.1278),
            utc(2026, 6, 21, 12.0),
            utc(2026, 6, 21, 3.0 + 43.0 / 60.0 + 6.0 / 3600.0),
            utc(2026, 6, 21, 20.0 + 21.0 / 60.0 + 33.0 / 3600.0),
        );
    }

    #[test]
    fn the_same_day_from_early_morning_or_late_evening() {
        let london = place(51.5074, -0.1278);
        assert_eq!(
            day(london, utc(2026, 6, 21, 0.5)),
            day(london, utc(2026, 6, 21, 23.5))
        );
    }

    #[test]
    fn polar_day_and_night_in_tromso() {
        let tromso = place(69.6492, 18.9553);
        assert_eq!(day(tromso, utc(2026, 6, 21, 12.0)), Day::NeverSets);
        assert_eq!(day(tromso, utc(2026, 12, 21, 12.0)), Day::NeverRises);
    }

    #[test]
    fn the_poles_do_not_break_the_arithmetic() {
        assert_eq!(
            day(place(90.0, 0.0), utc(2026, 6, 21, 12.0)),
            Day::NeverSets
        );
        assert_eq!(
            day(place(-90.0, 0.0), utc(2026, 6, 21, 12.0)),
            Day::NeverRises
        );
        assert!(position(place(90.0, 0.0), utc(2026, 6, 21, 12.0)).elevation > 23.0);
    }
}
