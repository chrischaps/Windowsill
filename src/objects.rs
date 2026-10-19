//! The things on the sill: a potted plant, a small glass jar, a candle and a
//! cat asleep beside them.
//!
//! The shader draws their shapes. What lives here is where each one stands and
//! what it is coloured, so the sill can be rearranged or rethemed from
//! `windowsill.toml` without touching the WGSL.

use serde::Deserialize;

use crate::palette::{Colour, Mix};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Plant {
    /// Where it stands, from 0 at the left end of the sill to 1 at the right.
    pub at: f32,
    /// The clay pot.
    pub pot: Colour,
    /// The leaves, in full light.
    pub leaf: Colour,
}

impl Default for Plant {
    fn default() -> Self {
        Self {
            at: 0.72,
            pot: Colour::rgb(0.66, 0.36, 0.24),
            leaf: Colour::rgb(0.31, 0.48, 0.24),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Jar {
    /// Where it stands, from 0 at the left end of the sill to 1 at the right.
    pub at: f32,
    /// The glass, which shows mostly at its edges.
    pub glass: Colour,
    /// The water it is a little more than half full of.
    pub water: Colour,
}

impl Default for Jar {
    fn default() -> Self {
        Self {
            at: 0.75,
            glass: Colour::rgb(0.82, 0.90, 0.88),
            water: Colour::rgb(0.47, 0.63, 0.66),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Candle {
    /// Where it stands, from 0 at the left end of the sill to 1 at the right.
    pub at: f32,
    pub wax: Colour,
    /// The flame, and the warmth it throws around itself.
    pub flame: Colour,
}

impl Default for Candle {
    fn default() -> Self {
        Self {
            at: 0.775,
            wax: Colour::rgb(0.93, 0.89, 0.80),
            flame: Colour::rgb(1.0, 0.70, 0.32),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Cat {
    /// Where it sleeps until it gets up and goes somewhere else, from 0 at the
    /// left end of the sill to 1 at the right.
    pub at: f32,
    /// Its coat.
    pub fur: Colour,
    /// The darker stripes along its back and round its tail.
    pub mark: Colour,
}

impl Default for Cat {
    fn default() -> Self {
        Self {
            at: 0.80,
            fur: Colour::rgb(0.561, 0.529, 0.502),
            mark: Colour::rgb(0.302, 0.282, 0.271),
        }
    }
}

impl Mix for Plant {
    fn mix(&self, other: &Self, t: f32) -> Self {
        Self {
            at: self.at.mix(&other.at, t),
            pot: self.pot.mix(&other.pot, t),
            leaf: self.leaf.mix(&other.leaf, t),
        }
    }
}

impl Mix for Jar {
    fn mix(&self, other: &Self, t: f32) -> Self {
        Self {
            at: self.at.mix(&other.at, t),
            glass: self.glass.mix(&other.glass, t),
            water: self.water.mix(&other.water, t),
        }
    }
}

impl Mix for Candle {
    fn mix(&self, other: &Self, t: f32) -> Self {
        Self {
            at: self.at.mix(&other.at, t),
            wax: self.wax.mix(&other.wax, t),
            flame: self.flame.mix(&other.flame, t),
        }
    }
}

impl Mix for Cat {
    fn mix(&self, other: &Self, t: f32) -> Self {
        Self {
            at: self.at.mix(&other.at, t),
            fur: self.fur.mix(&other.fur, t),
            mark: self.mark.mix(&other.mark, t),
        }
    }
}
