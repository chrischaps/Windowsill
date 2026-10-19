//! The sill's colours, and the ease from one look to the next when the config
//! changes them.
//!
//! Colours are kept as the shader writes them: non-linear 0..1 values, the same
//! numbers a `"#rrggbb"` in the config file names.

use std::time::{Duration, Instant};

use serde::{Deserialize, Deserializer, de};

use crate::objects::{Candle, Cat, Jar, Plant};

/// How long a new look takes to replace the old one. Long enough that a saved
/// edit never snaps (design rule 1); short enough to tune by eye.
const BLEND: Duration = Duration::from_secs(2);

/// Something that can be eased part of the way toward another of itself.
pub trait Mix {
    fn mix(&self, other: &Self, t: f32) -> Self;
}

impl Mix for f32 {
    fn mix(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colour(pub [f32; 3]);

impl Colour {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self([r, g, b])
    }

    /// For the shader, which wants `vec4` alignment.
    pub fn padded(self) -> [f32; 4] {
        let [r, g, b] = self.0;
        [r, g, b, 1.0]
    }
}

impl Mix for Colour {
    fn mix(&self, other: &Self, t: f32) -> Self {
        Self(std::array::from_fn(|i| self.0[i].mix(&other.0[i], t)))
    }
}

impl std::str::FromStr for Colour {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let hex = text
            .strip_prefix('#')
            .filter(|hex| hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| format!("{text:?} is not a colour like \"#6b4529\""))?;
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f32 / 255.0;
        Ok(Self::rgb(channel(0), channel(2), channel(4)))
    }
}

impl<'de> Deserialize<'de> for Colour {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Palette {
    /// The jar's wooden lid.
    pub wood: Colour,
    /// The lid's top edge, where it catches the light.
    pub lip: Colour,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            wood: Colour::rgb(0.42, 0.27, 0.16),
            lip: Colour::rgb(0.58, 0.40, 0.26),
        }
    }
}

impl Mix for Palette {
    fn mix(&self, other: &Self, t: f32) -> Self {
        Self {
            wood: self.wood.mix(&other.wood, t),
            lip: self.lip.mix(&other.lip, t),
        }
    }
}

/// Everything the config says about how the sill looks: the lid's colours,
/// and where each thing on it stands and what colour it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    pub palette: Palette,
    pub plant: Plant,
    pub jar: Jar,
    pub candle: Candle,
    pub cat: Cat,
}

impl Mix for Look {
    fn mix(&self, other: &Self, t: f32) -> Self {
        Self {
            palette: self.palette.mix(&other.palette, t),
            plant: self.plant.mix(&other.plant, t),
            jar: self.jar.mix(&other.jar, t),
            candle: self.candle.mix(&other.candle, t),
            cat: self.cat.mix(&other.cat, t),
        }
    }
}

/// The look on show, easing toward the most recently asked-for one.
pub struct Blend {
    from: Look,
    to: Look,
    since: Instant,
}

impl Blend {
    pub fn new(look: Look) -> Self {
        Self {
            from: look,
            to: look,
            since: Instant::now() - BLEND,
        }
    }

    /// Starts easing toward `look` from whatever is on show at `now`, so a
    /// second edit mid-ease carries on smoothly from where the first had got to.
    pub fn toward(&mut self, look: Look, now: Instant) {
        if look != self.to {
            self.from = self.at(now);
            self.to = look;
            self.since = now;
        }
    }

    pub fn at(&self, now: Instant) -> Look {
        let t = (now.saturating_duration_since(self.since).as_secs_f32() / BLEND.as_secs_f32())
            .min(1.0);
        if t == 1.0 {
            // Exactly, rather than to within a rounding error.
            return self.to;
        }
        self.from.mix(&self.to, t * t * (3.0 - 2.0 * t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hex_colours() {
        assert_eq!("#ff0080".parse(), Ok(Colour::rgb(1.0, 0.0, 128.0 / 255.0)));
        assert_eq!("#FFFFFF".parse(), Ok(Colour::rgb(1.0, 1.0, 1.0)));
        for bad in ["ff0080", "#ff008", "#ff00800", "#gg0080", "#ff00é"] {
            assert!(bad.parse::<Colour>().is_err(), "accepted {bad:?}");
        }
    }

    fn red() -> Look {
        Look {
            palette: Palette {
                wood: Colour::rgb(1.0, 0.0, 0.0),
                ..Palette::default()
            },
            candle: Candle {
                at: 0.2,
                ..Candle::default()
            },
            ..Look::default()
        }
    }

    #[test]
    fn eases_to_a_new_look_without_snapping() {
        let start = Instant::now();
        let mut blend = Blend::new(Look::default());
        let red = red();
        blend.toward(red, start);
        assert_eq!(blend.at(start), Look::default());
        let mut last = blend.at(start).palette.wood.0[0];
        for frame in 1..=30 {
            let now = start + Duration::from_nanos(1_000_000_000 / 15 * frame);
            let r = blend.at(now).palette.wood.0[0];
            assert!(r >= last && r - last < 0.05, "frame {frame}: {last} -> {r}");
            last = r;
        }
        assert_eq!(blend.at(start + BLEND), red);
    }

    #[test]
    fn a_second_edit_carries_on_from_where_the_first_got_to() {
        let start = Instant::now();
        let mut blend = Blend::new(Look::default());
        let red = red();
        blend.toward(red, start);
        let halfway = start + BLEND / 2;
        let shown = blend.at(halfway);
        blend.toward(Look::default(), halfway);
        assert_eq!(blend.at(halfway), shown);
        assert_eq!(blend.at(halfway + BLEND), Look::default());
    }
}
