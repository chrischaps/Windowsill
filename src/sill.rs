//! The sill's state: everything the renderer draws, and the ease that moves it.
//!
//! Inputs never touch what is on show. They set a *target*, and every frame
//! each value closes a share of the gap to its target, the share set by its
//! time constant and how long the frame was. That is design rule 1 in code:
//! a sudden change of target is followed, never jumped to.
//!
//! The ease is exponential, `1 - e^(-dt/τ)` of the gap per frame, so the same
//! span of time moves a value the same distance at 15 fps or at 30.

use serde::Deserialize;

/// The quickest time constant the config may ask for. With it, a sudden change
/// is still less than two thirds done after two seconds.
pub const QUICKEST: f32 = 2.0;

/// What the sill shows. Each field is either on show or a target, depending on
/// which half of a [`Sill`] holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SillState {
    /// How high the light comes in, in degrees above the horizon.
    pub beam_angle: f32,
    /// The light's colour temperature, in kelvin.
    pub colour_temperature: f32,
    /// Which way shadows fall: -1 to the left, 0 straight down, 1 to the right.
    pub shadow_side: f32,
    /// The candle's flame, from 0 (unlit) to 1.
    pub candle: f32,
    /// How much of the light is the moon's, from 0 to 1.
    pub moonlight: f32,
}

impl Default for SillState {
    /// Plain daylight, which is what the sill shows when it knows nothing.
    fn default() -> Self {
        Self {
            beam_angle: 45.0,
            colour_temperature: 5500.0,
            shadow_side: 0.0,
            candle: 0.0,
            moonlight: 0.0,
        }
    }
}

/// How slowly each part of the sill follows its target, in seconds: the time to
/// close about two thirds of a sudden gap.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Ease {
    /// The beam, its colour, the shadows and the moonlight.
    pub light: f32,
    /// The candle catching and going out.
    pub candle: f32,
}

impl Default for Ease {
    fn default() -> Self {
        Self {
            light: 4.0,
            candle: 3.0,
        }
    }
}

impl Ease {
    /// Each time constant with its name, for checking the config.
    pub fn each(&self) -> [(&'static str, f32); 2] {
        [("light", self.light), ("candle", self.candle)]
    }
}

/// The sill on show, and where each part of it is heading.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sill {
    shown: SillState,
    pub target: SillState,
}

impl Sill {
    /// A sill already showing `state`, with nothing to ease toward.
    pub fn at(state: SillState) -> Self {
        Self {
            shown: state,
            target: state,
        }
    }

    /// What to draw.
    pub fn shown(&self) -> SillState {
        self.shown
    }

    /// Moves everything on show toward its target by `dt` seconds' worth. A long
    /// `dt`, as after the sill has been aside, simply arrives; no one was watching.
    pub fn step(&mut self, dt: f32, ease: &Ease) {
        let (s, t) = (&mut self.shown, &self.target);
        for (value, target, tau) in [
            (&mut s.beam_angle, t.beam_angle, ease.light),
            (&mut s.colour_temperature, t.colour_temperature, ease.light),
            (&mut s.shadow_side, t.shadow_side, ease.light),
            (&mut s.moonlight, t.moonlight, ease.light),
            (&mut s.candle, t.candle, ease.candle),
        ] {
            *value += (target - *value) * (1.0 - (-dt / tau).exp());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sill asked, all at once, to go from plain daylight to a low red sunset.
    fn sunset() -> Sill {
        Sill {
            target: SillState {
                beam_angle: 5.0,
                colour_temperature: 2000.0,
                shadow_side: 1.0,
                candle: 1.0,
                moonlight: 1.0,
            },
            ..Sill::default()
        }
    }

    /// Runs `seconds` at `fps` and returns what is on show.
    fn run(mut sill: Sill, fps: u32, seconds: u32, ease: &Ease) -> SillState {
        for _ in 0..fps * seconds {
            sill.step(1.0 / fps as f32, ease);
        }
        sill.shown()
    }

    #[test]
    fn a_sudden_change_takes_at_least_two_seconds() {
        let quickest = Ease {
            light: QUICKEST,
            candle: QUICKEST,
        };
        let mut sill = sunset();
        let mut last = sill.shown().colour_temperature;
        for frame in 1..=30 {
            sill.step(1.0 / 15.0, &quickest);
            let k = sill.shown().colour_temperature;
            // Never more than a small nudge in one frame.
            assert!(
                k < last && last - k < 0.04 * 3500.0,
                "frame {frame}: {last} -> {k}"
            );
            last = k;
        }
        // Two seconds in, more than a third of the way is still to go.
        let shown = sill.shown();
        assert!(shown.colour_temperature - 2000.0 > 3500.0 / 3.0);
        assert!(1.0 - shown.candle > 1.0 / 3.0);
        assert!(shown.beam_angle - 5.0 > 40.0 / 3.0);
    }

    #[test]
    fn the_same_at_15_and_30_fps() {
        let ease = Ease::default();
        for seconds in [1, 2, 5, 20] {
            let slow = run(sunset(), 15, seconds, &ease);
            let fast = run(sunset(), 30, seconds, &ease);
            assert!(
                (slow.colour_temperature - fast.colour_temperature).abs() < 0.5
                    && (slow.beam_angle - fast.beam_angle).abs() < 1e-3
                    && (slow.candle - fast.candle).abs() < 1e-4
                    && (slow.shadow_side - fast.shadow_side).abs() < 1e-4,
                "after {seconds} s: {slow:?} vs {fast:?}"
            );
        }
    }

    #[test]
    fn it_arrives_in_the_end() {
        let shown = run(sunset(), 15, 60, &Ease::default());
        assert!((shown.colour_temperature - 2000.0).abs() < 1.0);
        assert!((shown.candle - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_long_gap_just_arrives() {
        let mut sill = sunset();
        sill.step(3600.0, &Ease::default());
        assert_eq!(sill.shown(), sill.target);
    }

    #[test]
    fn a_new_target_mid_ease_carries_on_from_where_it_got_to() {
        let ease = Ease::default();
        let mut sill = sunset();
        sill.step(1.0, &ease);
        let midway = sill.shown();
        sill.target = SillState::default();
        sill.step(1.0 / 30.0, &ease);
        assert!((sill.shown().beam_angle - midway.beam_angle).abs() < 0.5);
    }
}
