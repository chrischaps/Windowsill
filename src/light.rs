//! What the real sun does to the sill: how high the beam comes in, what colour
//! it is, and which way the shadows fall.
//!
//! The sill sits in the light of a room. You face one way to look at the
//! screen, and the room's window looks out another. The sun only shines in
//! while it is in front of that window; the rest of the day the light is the
//! sky's, softer and from the window's side. So the sill never argues with
//! the light falling on the desk around it.
//!
//! Told nothing about the room, the sill is its own window: you face the
//! equator, the side the sun spends its day on, and the sun comes in from
//! whichever side it is on. From the north that is south, so the morning sun
//! comes in from the left and shadows fall to the right. In the afternoon they
//! swap. From the south it is the mirror image.

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

/// Which ways things face in the room, in degrees clockwise from north.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    /// The way you face when you look at the screen.
    pub facing: f64,
    /// The way the room's window looks out. Unknown means the sill is its own
    /// window, straight ahead, and lets the sun in from either side.
    pub window: Option<f64>,
}

impl Room {
    /// Where something at `bearing` is across your view: -1 on your left, 0
    /// straight ahead or behind, 1 on your right.
    fn across(&self, bearing: f64) -> f32 {
        (bearing - self.facing).to_radians().sin() as f32
    }

    /// Which way shadows fall for a sun at `azimuth`: -1 to the left, 0
    /// straight back, 1 to the right. They fall away from it.
    pub fn shadow_side(&self, azimuth: f64) -> f32 {
        -self.across(azimuth)
    }

    /// Which side the window's light comes from: -1 left, 0 ahead, 1 right.
    pub fn window_side(&self) -> f32 {
        self.window.map_or(0.0, |window| self.across(window))
    }

    /// How much of a sun at `azimuth` the window lets in, from 0 to 1. Only a
    /// sun in front of the window shines through it, and one that has just
    /// come round its edge only a little.
    pub fn sunshine(&self, azimuth: f64) -> f32 {
        let Some(window) = self.window else {
            return 1.0;
        };
        let ahead = (azimuth - window).to_radians().cos() as f32;
        let t = ((ahead + 0.05) / 0.35).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

/// `state` with its beam, colour and shadows set by the real sun, as it falls
/// in `room`.
pub fn in_sunlight(state: SillState, room: Room, sun: Sun) -> SillState {
    let elevation = sun.elevation as f32;
    SillState {
        beam_angle: elevation.max(BELOW),
        colour_temperature: colour_temperature(elevation),
        shadow_side: room.shadow_side(sun.azimuth),
        sunshine: room.sunshine(sun.azimuth),
        window_side: room.window_side(),
        ..state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sill as its own window in the north, and in the south.
    const SEATTLE: Room = Room {
        facing: 180.0,
        window: None,
    };
    const SYDNEY: Room = Room {
        facing: 0.0,
        window: None,
    };
    /// Facing north at a desk, with a window on the right looking east.
    const DESK: Room = Room {
        facing: 0.0,
        window: Some(90.0),
    };

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
        assert_eq!(toward_the_equator(47.61), SEATTLE.facing);
        assert_eq!(toward_the_equator(-33.87), SYDNEY.facing);
    }

    #[test]
    fn looking_north_from_the_north_mirrors_looking_south() {
        // Facing north, the east is on the right, so the morning sun comes
        // from the right and shadows fall to the left.
        let room = Room {
            facing: 0.0,
            window: None,
        };
        let morning = in_sunlight(SillState::default(), room, at(15.0, 110.0));
        assert!(morning.shadow_side < -0.5, "{morning:?}");
    }

    #[test]
    fn an_east_window_lets_the_morning_sun_in_and_not_the_afternoon() {
        let morning = in_sunlight(SillState::default(), DESK, at(15.0, 110.0));
        let afternoon = in_sunlight(SillState::default(), DESK, at(30.0, 220.0));
        assert_eq!(morning.sunshine, 1.0);
        assert_eq!(afternoon.sunshine, 0.0);
        // The window's light comes from the right all day, and so does the
        // morning sun, so shadows fall to the left.
        assert_eq!(morning.window_side, 1.0);
        assert_eq!(afternoon.window_side, 1.0);
        assert!(morning.shadow_side < -0.9, "{morning:?}");
    }

    #[test]
    fn the_sun_fades_in_as_it_comes_round_the_window_edge() {
        let mut last = 0.0;
        for azimuth in (150..=200).rev().step_by(5) {
            let sunshine = DESK.sunshine(azimuth as f64);
            assert!(sunshine >= last, "{azimuth}: {sunshine} after {last}");
            last = sunshine;
        }
        assert!(DESK.sunshine(185.0) == 0.0 && DESK.sunshine(150.0) == 1.0);
    }

    #[test]
    fn with_no_window_the_sill_lets_the_sun_in_from_anywhere() {
        let dawn = in_sunlight(SillState::default(), SEATTLE, at(1.0, 60.0));
        assert_eq!((dawn.sunshine, dawn.window_side), (1.0, 0.0));
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
