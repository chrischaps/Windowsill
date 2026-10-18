//! The faint ring of light the jar gives back to an accent in the music.
//!
//! A strong kick sets the glass ringing, as a struck glass hums. The ring
//! swells over a second or so and dies away over several more, and the glass
//! takes no notice of another kick until the last ring is all but gone, so a
//! song never makes it flash. The kick only sets the ring going; how bright it
//! is comes from two slow eases one after the other, so it never jumps.

/// How strong a kick has to be heard, from 0 to 1, to set the glass ringing.
const STRUCK: f32 = 0.5;

/// The least time between one ring and the next, in seconds.
pub const GAP: f32 = 6.0;

/// How long the ringing takes to die away, in seconds.
const HUM: f32 = 1.8;

/// How slowly the light follows the ringing, in seconds.
const GLOW: f32 = 1.2;

/// How bright the light gets from one ring, before it is scaled to 1. With
/// these two eases it peaks about a second and a half after the kick.
const PEAK: f32 = 0.445;

/// A frame longer than this means the sill was aside, and any ring is over.
const AWAY: f32 = 1.0;

/// How the jar is ringing.
#[derive(Clone, Copy, Debug)]
pub struct Ring {
    /// How hard the glass is ringing, from 0 to 1.
    hum: f32,
    /// How much of the ring's light is on show, from 0 to 1.
    glow: f32,
    /// Seconds since the glass last rang.
    since: f32,
}

impl Default for Ring {
    fn default() -> Self {
        Self {
            hum: 0.0,
            glow: 0.0,
            since: GAP,
        }
    }
}

impl Ring {
    /// Moves on by `dt` real seconds while the kick envelope is `kick`.
    pub fn advance(&mut self, dt: f32, kick: f32) {
        if dt > AWAY {
            *self = Self::default();
            return;
        }
        self.since += dt;
        if kick >= STRUCK && self.since >= GAP {
            self.hum = 1.0;
            self.since = 0.0;
        }
        // The light eases toward the hum as it was at the start of the frame,
        // then the hum dies down, so the frame a kick lands on starts the rise.
        let hum = self.hum;
        self.hum *= (-dt / HUM).exp();
        self.glow = hum - (hum - self.glow) * (-dt / GLOW).exp();
    }

    /// How bright the ring's light is, from 0 to about 1.
    pub fn light(&self) -> f32 {
        (self.glow / PEAK).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `seconds` at `fps` with kicks at full strength `per_second` times
    /// a second, and returns each frame's light.
    fn play(ring: &mut Ring, fps: u32, seconds: f32, per_second: f32) -> Vec<f32> {
        (0..(fps as f32 * seconds) as u32)
            .map(|frame| {
                let t = frame as f32 / fps as f32;
                let kick = (-(t % (1.0 / per_second)) / 0.1).exp();
                ring.advance(1.0 / fps as f32, kick);
                ring.light()
            })
            .collect()
    }

    #[test]
    fn dark_in_silence() {
        let mut ring = Ring::default();
        for _ in 0..150 {
            ring.advance(1.0 / 15.0, 0.0);
        }
        assert_eq!(ring.light(), 0.0);
    }

    #[test]
    fn swells_and_fades_without_a_jump() {
        let mut ring = Ring::default();
        ring.advance(1.0 / 15.0, 1.0);
        let mut last = ring.light();
        assert!(last < 0.15, "the first frame is only a start: {last}");
        let mut peak = (0.0, 0);
        for frame in 1..15 * 8 {
            ring.advance(1.0 / 15.0, 0.0);
            let light = ring.light();
            assert!(
                (light - last).abs() < 0.15,
                "frame {frame}: {last} -> {light}"
            );
            if light > peak.0 {
                peak = (light, frame);
            }
            last = light;
        }
        assert!(peak.0 > 0.95, "{peak:?}");
        assert!(
            (15..=30).contains(&peak.1),
            "peaks a second or two in: {peak:?}"
        );
        assert!(last < 0.1, "all but gone eight seconds on: {last}");
    }

    #[test]
    fn a_busy_song_never_makes_it_strobe() {
        let mut ring = Ring::default();
        let lights = play(&mut ring, 30, 30.0, 4.0);
        // Count the times it starts to rise again.
        let rises = lights
            .windows(3)
            .filter(|w| w[0] >= w[1] && w[2] > w[1])
            .count();
        assert!(rises <= (30.0 / GAP) as usize + 1, "{rises} rings in 30 s");
        assert!(lights.iter().all(|&l| l <= 1.0));
    }

    #[test]
    fn the_same_at_15_and_30_fps() {
        let (mut slow, mut fast) = (Ring::default(), Ring::default());
        let slow = play(&mut slow, 15, 4.0, 0.2);
        let fast = play(&mut fast, 30, 4.0, 0.2);
        for (i, s) in slow.iter().enumerate() {
            let f = fast[2 * i + 1];
            assert!((s - f).abs() < 0.08, "at {i}: {s} vs {f}");
        }
    }

    #[test]
    fn a_long_gap_ends_the_ring() {
        let mut ring = Ring::default();
        ring.advance(1.0 / 15.0, 1.0);
        ring.advance(1.0 / 15.0, 0.0);
        ring.advance(30.0, 0.0);
        assert_eq!(ring.light(), 0.0);
    }
}
