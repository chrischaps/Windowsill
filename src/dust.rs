//! The dust in the beam: how stirred it is, and how far it has drifted.
//!
//! The motes themselves are drawn in `glass.wgsl`, each one in a cell of a grid
//! that rises slowly through the air. All the CPU keeps is how far through its
//! cycle that grid has come. The grid moves at the music's pace, quicker just
//! after each beat and slower between, and stands still in silence.
//!
//! Only the speed follows `beat_phase`, never the position. A tempo that
//! jumps, or a phase nudged onto a beat, changes how fast the dust moves, not
//! where it is, so nothing on the sill ever jumps with it.

use std::f32::consts::TAU;

/// Seconds of drift at full stir before the grid comes round to where it began.
/// `glass.wgsl` repeats its motes on the same cycle, so the wrap is seamless.
pub const CYCLE: f32 = 360.0;

/// How far after the beat the push is strongest, as a share of a beat.
const PUSH: f32 = 0.1;

/// How stirred the dust should be by bass this loud, from 0 to 1. `bass_att`
/// is already measured against the music's own recent peaks, so most songs sit
/// between a third and two thirds of it.
pub fn stir(bass_att: f32) -> f32 {
    (bass_att * 1.8).clamp(0.0, 1.0)
}

/// How fast the dust drifts, as a share of its full speed: in proportion to
/// how stirred it is, and swelling once a beat. It is smooth in the phase,
/// including across the wrap from one beat into the next.
pub fn speed(stir: f32, beat_phase: f32) -> f32 {
    let push = 0.5 + 0.5 * (TAU * (beat_phase - PUSH)).cos();
    stir * (0.25 + 0.75 * push)
}

/// How far through its cycle the dust has drifted, from 0 to 1.
#[derive(Clone, Copy, Debug, Default)]
pub struct Drift {
    at: f32,
}

impl Drift {
    /// Drifts on by `dt` real seconds, `stir` and `beat_phase` as they are now.
    pub fn advance(&mut self, dt: f32, stir: f32, beat_phase: f32) {
        self.at = (self.at + dt * speed(stir, beat_phase) / CYCLE).rem_euclid(1.0);
    }

    /// How far through the cycle it has come.
    pub fn at(&self) -> f32 {
        self.at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_in_silence() {
        let mut drift = Drift::default();
        for frame in 0..300 {
            drift.advance(1.0 / 15.0, 0.0, frame as f32 / 37.0 % 1.0);
        }
        assert_eq!(drift.at(), 0.0);
    }

    #[test]
    fn quickest_just_after_the_beat() {
        let after = speed(1.0, PUSH);
        let between = speed(1.0, PUSH + 0.5);
        assert!(after > 2.0 * between, "{after} vs {between}");
        assert!(between > 0.0, "it never quite stops while music plays");
    }

    #[test]
    fn smooth_across_the_wrap() {
        let before = speed(0.6, 0.9999);
        let after = speed(0.6, 0.0);
        assert!((before - after).abs() < 1e-3, "{before} vs {after}");
    }

    #[test]
    fn the_same_at_15_and_30_fps() {
        // Ten seconds of a steady 120 BPM song.
        let run = |fps: u32| {
            let mut drift = Drift::default();
            for frame in 0..fps * 10 {
                let t = frame as f32 / fps as f32;
                drift.advance(1.0 / fps as f32, 0.7, (t * 2.0).fract());
            }
            drift.at()
        };
        let (slow, fast) = (run(15), run(30));
        assert!(slow > 0.0);
        assert!((slow - fast).abs() < 0.02 * slow, "{slow} vs {fast}");
    }

    #[test]
    fn comes_round_without_a_jump() {
        let mut drift = Drift { at: 0.99999 };
        drift.advance(1.0, 1.0, 0.0);
        assert!(drift.at() < 0.01, "{}", drift.at());
    }

    #[test]
    fn louder_bass_stirs_more_up_to_a_point() {
        assert_eq!(stir(0.0), 0.0);
        assert!(stir(0.2) < stir(0.4));
        assert_eq!(stir(0.9), 1.0);
    }
}
