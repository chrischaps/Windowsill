//! The room's light through the day and night: Fishbowl's palette keyframes,
//! keyed by how high the sun is rather than by the clock.
//!
//! Each key says, for one height of the sun, what colour the room's light is
//! and how much colour things keep in it. Between keys the palette eases with a
//! smoothstep, as Fishbowl's does, so it never steps.
//!
//! Dawn and dusk come at the same height but are not the same light, so a key
//! can belong to the rising sun, the setting sun or both. The two halves are
//! blended by which side of the sky the sun is on, so the palette passes
//! through noon and midnight without a seam.

use serde::Deserialize;

use crate::palette::{Colour, Mix};
use crate::sun::Sun;

/// Which half of the day a key belongs to.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum When {
    #[default]
    Both,
    Rising,
    Setting,
}

impl When {
    fn includes(self, half: When) -> bool {
        self == When::Both || self == half
    }
}

/// The room's light at one height of the sun.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Key {
    /// The sun's height, in degrees above the horizon.
    pub elevation: f32,
    #[serde(default)]
    pub when: When,
    /// The colour every lit thing is multiplied by.
    pub ambient: Colour,
    /// How much of their own colour things keep, from 0 (grey) to 1.
    pub vivid: f32,
}

/// What the room's light does to the things in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shade {
    pub ambient: Colour,
    pub vivid: f32,
}

impl Default for Shade {
    /// Plain daylight: everything as it is.
    fn default() -> Self {
        Self {
            ambient: Colour::rgb(1.0, 1.0, 1.0),
            vivid: 1.0,
        }
    }
}

impl Mix for Shade {
    fn mix(&self, other: &Self, t: f32) -> Self {
        Self {
            ambient: self.ambient.mix(&other.ambient, t),
            vivid: self.vivid.mix(&other.vivid, t),
        }
    }
}

/// The day's keys, in any order.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct Sky(pub Vec<Key>);

impl Default for Sky {
    /// Fishbowl's tested palette, each key pinned to the height of the sun at
    /// the hour Fishbowl gave it.
    fn default() -> Self {
        let key = |elevation, when, (r, g, b), vivid| Key {
            elevation,
            when,
            ambient: Colour::rgb(r, g, b),
            vivid,
        };
        use When::*;
        Self(vec![
            key(-40.0, Both, (0.28, 0.35, 0.52), 0.35),   // late night
            key(-18.0, Both, (0.30, 0.38, 0.56), 0.35),   // night
            key(0.0, Rising, (0.80, 0.76, 0.78), 0.75),   // dawn
            key(12.0, Rising, (0.98, 0.98, 0.96), 1.0),   // morning
            key(30.0, Both, (1.0, 1.0, 1.0), 1.0),        // day
            key(15.0, Setting, (1.0, 0.97, 0.92), 1.0),   // afternoon
            key(2.0, Setting, (1.0, 0.84, 0.74), 0.95),   // sunset
            key(-5.0, Setting, (0.56, 0.52, 0.66), 0.65), // dusk
        ])
    }
}

impl Sky {
    /// The room's light with the sun at `sun`.
    pub fn at(&self, sun: Sun) -> Shade {
        let elevation = sun.elevation as f32;
        // 0 with the sun in the east, 1 in the west, a half at noon and
        // midnight, where the two halves agree.
        let west = 0.5 - 0.5 * (sun.azimuth.to_radians().sin() as f32);
        let west = smoothstep(0.2, 0.8, west);
        let rising = self.half(When::Rising, elevation);
        let setting = self.half(When::Setting, elevation);
        rising.mix(&setting, west)
    }

    /// The light along one half of the day, at `elevation`.
    fn half(&self, half: When, elevation: f32) -> Shade {
        let mut keys: Vec<&Key> = self.0.iter().filter(|k| k.when.includes(half)).collect();
        keys.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
        let shade = |k: &Key| Shade {
            ambient: k.ambient,
            vivid: k.vivid,
        };
        let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
            return Shade::default();
        };
        if elevation <= first.elevation {
            return shade(first);
        }
        if elevation >= last.elevation {
            return shade(last);
        }
        let below = keys.iter().rposition(|k| k.elevation <= elevation).unwrap();
        let (a, b) = (keys[below], keys[below + 1]);
        let t = (elevation - a.elevation) / (b.elevation - a.elevation);
        shade(a).mix(&shade(b), smoothstep(0.0, 1.0, t))
    }

    /// What is wrong with the keys, if anything.
    pub fn check(&self) -> Result<(), String> {
        for half in [When::Rising, When::Setting] {
            if !self.0.iter().any(|k| k.when.includes(half)) {
                return Err(format!("the {half:?} sun has no sky keys").to_lowercase());
            }
        }
        for key in &self.0 {
            if !(-90.0..=90.0).contains(&key.elevation) {
                return Err(format!(
                    "sky elevation {} is not between -90 and 90",
                    key.elevation
                ));
            }
            if !(0.0..=1.0).contains(&key.vivid) {
                return Err(format!("sky vivid {} is not between 0 and 1", key.vivid));
            }
        }
        Ok(())
    }
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sun(elevation: f64, azimuth: f64) -> Sun {
        Sun { elevation, azimuth }
    }

    /// The largest change between two shades, in any channel.
    fn gap(a: Shade, b: Shade) -> f32 {
        (0..3)
            .map(|i| (a.ambient.0[i] - b.ambient.0[i]).abs())
            .fold((a.vivid - b.vivid).abs(), f32::max)
    }

    #[test]
    fn moves_through_every_height_without_a_step() {
        let sky = Sky::default();
        for azimuth in [60.0, 90.0, 180.0, 270.0, 300.0] {
            let mut last = sky.at(sun(-90.0, azimuth));
            for tenth in -900..=900 {
                let now = sky.at(sun(tenth as f64 / 10.0, azimuth));
                // A tenth of a degree is about two seconds of the sun's travel.
                assert!(gap(last, now) < 0.01, "at {tenth}/10° from {azimuth}°");
                last = now;
            }
        }
    }

    #[test]
    fn moves_round_the_sky_without_a_step() {
        let sky = Sky::default();
        for elevation in [-30.0, -5.0, 0.0, 5.0, 40.0] {
            let mut last = sky.at(sun(elevation, 0.0));
            for degree in 1..=360 {
                let now = sky.at(sun(elevation, degree as f64));
                assert!(gap(last, now) < 0.02, "at {degree}° and {elevation}° up");
                last = now;
            }
        }
    }

    #[test]
    fn day_is_plain_and_night_is_dim_and_blue() {
        let sky = Sky::default();
        assert_eq!(sky.at(sun(50.0, 180.0)), Shade::default());
        let night = sky.at(sun(-30.0, 0.0));
        let [r, _, b] = night.ambient.0;
        assert!(r < 0.4 && b > r && night.vivid < 0.5, "{night:?}");
    }

    #[test]
    fn dawn_and_dusk_differ_at_the_same_height() {
        let sky = Sky::default();
        let dawn = sky.at(sun(-2.0, 80.0));
        let dusk = sky.at(sun(-2.0, 280.0));
        // Dusk is the darker and more violet of the two.
        assert!(
            dusk.ambient.0[1] < dawn.ambient.0[1] - 0.1,
            "{dawn:?} {dusk:?}"
        );
        let sunset = sky.at(sun(2.0, 280.0));
        assert!(
            sunset.ambient.0[0] > sunset.ambient.0[2] + 0.2,
            "{sunset:?}"
        );
    }

    #[test]
    fn a_single_key_is_the_whole_day() {
        let one = Key {
            elevation: 0.0,
            when: When::Both,
            ambient: Colour::rgb(0.5, 0.5, 0.5),
            vivid: 0.5,
        };
        let sky = Sky(vec![one]);
        assert!(sky.check().is_ok());
        for elevation in [-60.0, 0.0, 60.0] {
            assert_eq!(sky.at(sun(elevation, 120.0)).vivid, 0.5);
        }
    }

    #[test]
    fn each_half_of_the_day_needs_a_key() {
        let rising_only = Sky(Sky::default()
            .0
            .into_iter()
            .filter(|k| k.when == When::Rising)
            .collect());
        assert!(rising_only.check().is_err());
        assert!(Sky::default().check().is_ok());
    }
}
