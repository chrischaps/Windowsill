//! What the real sun does to the sill: how high the beam comes in, what colour
//! it is, and which way the shadows fall.
//!
//! The sill is in a window that faces the way you do when you look at the
//! screen. Unless told otherwise it faces the equator, the side the sun spends
//! its day on. From the north that is south, so the morning sun comes in from
//! the left and shadows fall to the right. In the afternoon they swap.
//! From the south it is the mirror image.

use crate::sill::SillState;
use crate::sun::Sun;

/// The light at its warmest, with the sun on the horizon, in kelvin.
const HORIZON_WARMTH: f32 = 1900.0;
/// The light at its whitest, with the sun high.
const HIGH_WHITE: f32 = 5800.0;
/// How many degrees of elevation the light takes to whiten most of the way:
/// the gold goes quickly once the sun is clear of the haze.
const WHITENING: f32 = 11.0;
/// How far below the horizon the beam is followed down. Lower than this
/// nothing reaches the sill, so the angle stops there instead of easing
/// through the whole night.
const BELOW: f32 = -8.0;

/// The light's colour temperature with the sun `elevation` degrees up. It is
/// gold at the horizon and whitens as the sun climbs.
pub fn colour_temperature(elevation: f32) -> f32 {
    let up = elevation.max(0.0);
    HIGH_WHITE - (HIGH_WHITE - HORIZON_WARMTH) * (-up / WHITENING).exp()
}

/// The way a window faces the equator from `latitude`, in degrees clockwise
/// from north.
pub fn toward_the_equator(latitude: f64) -> f64 {
    if latitude >= 0.0 { 180.0 } else { 0.0 }
}

/// Which way shadows fall for a sun at `azimuth`, seen through a window that
/// looks out toward `facing`: -1 to the left, 0 straight back, 1 to the right.
pub fn shadow_side(facing: f64, azimuth: f64) -> f32 {
    // Where the sun stands across the window, -1 at its left edge to 1 at its
    // right. The shadow falls away from it.
    let across = (azimuth - facing).to_radians().sin();
    -across as f32
}

/// `state` with its beam, colour and shadows set by the real sun, seen through
/// a window that looks out toward `facing`.
pub fn in_sunlight(state: SillState, facing: f64, sun: Sun) -> SillState {
    let elevation = sun.elevation as f32;
    SillState {
        beam_angle: elevation.max(BELOW),
        colour_temperature: colour_temperature(elevation),
        shadow_side: shadow_side(facing, sun.azimuth),
        ..state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Looking out of a window in the north, and in the south.
    const SEATTLE: f64 = 180.0;
    const SYDNEY: f64 = 0.0;

    fn at(elevation: f64, azimuth: f64) -> Sun {
        Sun { elevation, azimuth }
    }

    #[test]
    fn morning_and_afternoon_shadows_fall_on_opposite_sides() {
        let morning = in_sunlight(SillState::default(), SEATTLE, at(15.0, 110.0));
        let afternoon = in_sunlight(SillState::default(), SEATTLE, at(15.0, 250.0));
        assert!(morning.shadow_side > 0.5, "{morning:?}");
        assert!(afternoon.shadow_side < -0.5, "{afternoon:?}");
    }

    #[test]
    fn at_noon_shadows_fall_straight_back() {
        let noon = in_sunlight(SillState::default(), SEATTLE, at(60.0, 180.0));
        assert!(noon.shadow_side.abs() < 1e-6);
        let noon = in_sunlight(SillState::default(), SYDNEY, at(60.0, 0.0));
        assert!(noon.shadow_side.abs() < 1e-6);
    }

    #[test]
    fn the_south_is_the_mirror_of_the_north() {
        // In the south the morning sun is still in the east, but the window
        // faces north, so the east is on the right.
        let morning = in_sunlight(SillState::default(), SYDNEY, at(15.0, 70.0));
        assert!(morning.shadow_side < -0.5, "{morning:?}");
    }

    #[test]
    fn the_window_faces_the_equator_unless_told() {
        assert_eq!(toward_the_equator(47.61), SEATTLE);
        assert_eq!(toward_the_equator(-33.87), SYDNEY);
    }

    #[test]
    fn looking_north_from_the_north_mirrors_looking_south() {
        // Facing north, the east is on the right, so the morning sun comes
        // from the right and shadows fall to the left.
        let morning = in_sunlight(SillState::default(), 0.0, at(15.0, 110.0));
        assert!(morning.shadow_side < -0.5, "{morning:?}");
        // Facing east, the noon sun in the south is on the right.
        let noon = in_sunlight(SillState::default(), 90.0, at(40.0, 180.0));
        assert!(noon.shadow_side < -0.9, "{noon:?}");
    }

    #[test]
    fn low_light_is_gold_and_high_light_is_white() {
        let dawn = colour_temperature(1.0);
        let morning = colour_temperature(10.0);
        let noon = colour_temperature(60.0);
        assert!(dawn < 2300.0, "{dawn}");
        assert!(dawn < morning && morning < noon);
        assert!(noon > 5700.0, "{noon}");
        // It never runs past either end.
        assert_eq!(colour_temperature(-20.0), HORIZON_WARMTH);
        assert!(colour_temperature(90.0) <= HIGH_WHITE);
    }

    #[test]
    fn the_beam_is_followed_only_a_little_way_below_the_horizon() {
        let dusk = in_sunlight(SillState::default(), SEATTLE, at(-3.0, 290.0));
        let night = in_sunlight(SillState::default(), SEATTLE, at(-40.0, 0.0));
        assert_eq!(dusk.beam_angle, -3.0);
        assert_eq!(night.beam_angle, BELOW);
    }

    #[test]
    fn the_candle_and_the_moon_are_left_alone() {
        let before = SillState {
            candle: 0.7,
            moonlight: 0.3,
            ..SillState::default()
        };
        let after = in_sunlight(before, SEATTLE, at(30.0, 200.0));
        assert_eq!((after.candle, after.moonlight), (0.7, 0.3));
    }
}
