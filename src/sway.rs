//! How far the plant's leaves lean as the bass thumps.
//!
//! Each kick drum pushes the leaves, and a slow, heavily damped spring brings
//! them back, so a song with a steady beat leans them over a few degrees and
//! holds them there, nodding a little. A break lets them come upright again,
//! with the smallest overshoot, and a drop leans them back over. The kicks
//! push, and the spring decides where the leaves are, so however sudden the
//! music the leaves move only as a leaf in a draft does.

use std::f32::consts::TAU;

/// How long the leaves take to swing back and forth once, left to themselves.
const PERIOD: f32 = 5.0;

/// How much of the swing the spring takes out: 1 would come to rest without
/// overshooting at all.
const DAMPING: f32 = 0.7;

/// How hard a kick pushes, in degrees per second per second at full strength.
/// A song kicking twice a second leans the leaves about three degrees.
const PUSH: f32 = 24.0;

/// The furthest the leaves ever lean, in degrees.
pub const MOST: f32 = 5.0;

/// The longest step the spring is moved in one go. A frame longer than this is
/// taken in several, so the spring behaves the same at any frame rate.
const STEP: f32 = 1.0 / 60.0;

/// A frame longer than this means the sill was aside, and the leaves are simply
/// back where the music left them settling to: upright.
const AWAY: f32 = 1.0;

/// The leaves' lean, in degrees to the right, and how fast it is changing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sway {
    lean: f32,
    speed: f32,
}

impl Sway {
    /// Moves on by `dt` real seconds while the kick envelope is `kick`.
    pub fn advance(&mut self, dt: f32, kick: f32) {
        if dt > AWAY {
            *self = Self::default();
            return;
        }
        let w = TAU / PERIOD;
        let steps = (dt / STEP).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            let pull = PUSH * kick - w * w * self.lean - 2.0 * DAMPING * w * self.speed;
            // Semi-implicit Euler: the new speed moves the lean, which keeps a
            // spring this stiff from gaining energy step by step.
            self.speed += pull * h;
            self.lean = (self.lean + self.speed * h).clamp(-MOST, MOST);
        }
    }

    /// How far the leaves lean, in degrees to the right.
    pub fn lean(&self) -> f32 {
        self.lean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kick drum `bpm` times a minute, as the analyzer hears it at `t`
    /// seconds: a jump to 1 that decays over about a tenth of a second.
    fn kick(bpm: f32, t: f32) -> f32 {
        let since = t % (60.0 / bpm);
        (-since / 0.1).exp()
    }

    /// Plays `seconds` of a steady song at `fps`, and returns the sway and the
    /// fastest the leaves moved, in degrees a second.
    fn play(sway: &mut Sway, fps: u32, seconds: f32, bpm: f32) -> f32 {
        let mut fastest: f32 = 0.0;
        for frame in 0..(fps as f32 * seconds) as u32 {
            let t = frame as f32 / fps as f32;
            let before = sway.lean();
            sway.advance(1.0 / fps as f32, kick(bpm, t));
            fastest = fastest.max((sway.lean() - before).abs() * fps as f32);
        }
        fastest
    }

    #[test]
    fn still_in_silence() {
        let mut sway = Sway::default();
        for _ in 0..300 {
            sway.advance(1.0 / 15.0, 0.0);
        }
        assert_eq!(sway.lean(), 0.0);
    }

    #[test]
    fn a_steady_beat_leans_the_leaves_a_few_degrees() {
        let mut sway = Sway::default();
        play(&mut sway, 30, 20.0, 120.0);
        let lean = sway.lean();
        assert!((1.5..=MOST).contains(&lean), "{lean}");
    }

    #[test]
    fn never_quicker_than_a_leaf_in_a_draft() {
        let mut sway = Sway::default();
        let fastest = play(&mut sway, 30, 20.0, 128.0);
        assert!(fastest < 6.0, "{fastest} degrees a second");
    }

    #[test]
    fn upright_again_some_seconds_after_the_music_stops() {
        let mut sway = Sway::default();
        play(&mut sway, 15, 10.0, 120.0);
        let leaning = sway.lean();
        for _ in 0..15 {
            sway.advance(1.0 / 15.0, 0.0);
        }
        assert!(sway.lean() > leaning / 3.0, "still leaning a second later");
        for _ in 0..15 * 8 {
            sway.advance(1.0 / 15.0, 0.0);
        }
        assert!(sway.lean().abs() < 0.1, "{}", sway.lean());
    }

    #[test]
    fn the_same_at_15_and_30_fps() {
        let (mut slow, mut fast) = (Sway::default(), Sway::default());
        play(&mut slow, 15, 10.0, 120.0);
        play(&mut fast, 30, 10.0, 120.0);
        assert!(
            (slow.lean() - fast.lean()).abs() < 0.3,
            "{slow:?} vs {fast:?}"
        );
    }

    #[test]
    fn a_long_gap_just_arrives() {
        let mut sway = Sway::default();
        play(&mut sway, 15, 5.0, 120.0);
        sway.advance(30.0, 0.0);
        assert_eq!(sway.lean(), 0.0);
    }
}
