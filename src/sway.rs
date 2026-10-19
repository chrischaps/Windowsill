//! How far the plant's leaves lean as the bass plays.
//!
//! The bass leans the leaves over, the more the louder it is, and each kick
//! drum nods them about that lean, so a song with a steady beat leans them
//! over and nods them on every kick. A softly damped spring decides where they are,
//! so each push swings them a little past and back, the way a plant in a
//! draft answers a gust. A break lets them come upright again, and a drop
//! leans them back over. However sudden the music, the leaves never jump: the
//! kicks and the bass only push, and the spring moves them.

use std::f32::consts::TAU;

/// How long the lean takes to swing back and forth once, left to itself, and
/// how much of the swing it loses each time: 1 would come to rest without
/// overshooting at all.
const PERIOD: f32 = 2.4;
const DAMPING: f32 = 0.45;

/// How far the bass leans the leaves, in degrees, at full strength. A typical
/// song's `bass_att` of about half leans them six, so the tips of the
/// longest leaves move a few pixels.
const BASS: f32 = 12.0;

/// The nod each kick gives on top: quicker than the lean, and damped so it
/// comes back after a single dip rather than bouncing.
const NOD_PERIOD: f32 = 0.6;
const NOD_DAMPING: f32 = 0.6;

/// How hard a kick nods them, in degrees per second per second at full
/// strength. A song kicking twice a second nods them two or three degrees.
const PUSH: f32 = 650.0;

/// The furthest the leaves ever lean, in degrees.
pub const MOST: f32 = 16.0;

/// The longest step the springs are moved in one go. A frame longer than this
/// is taken in several, so they behave the same at any frame rate.
const STEP: f32 = 1.0 / 60.0;

/// A frame longer than this means the sill was aside, and the leaves are simply
/// back where the music left them settling to: upright.
const AWAY: f32 = 1.0;

/// The leaves' lean to the bass and their nod to the kicks, each in degrees to
/// the right, and how fast each is changing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sway {
    lean: f32,
    speed: f32,
    nod: f32,
    nod_speed: f32,
}

impl Sway {
    /// Moves on by `dt` real seconds while the kick envelope is `kick` and the
    /// bass, averaged over about a second, is `bass`.
    pub fn advance(&mut self, dt: f32, kick: f32, bass: f32) {
        if dt > AWAY {
            *self = Self::default();
            return;
        }
        let (w, n) = (TAU / PERIOD, TAU / NOD_PERIOD);
        let rest = BASS * bass.clamp(0.0, 1.0);
        let steps = (dt / STEP).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            // Semi-implicit Euler: the new speed moves the lean, which keeps a
            // spring this stiff from gaining energy step by step.
            self.speed += (w * w * (rest - self.lean) - 2.0 * DAMPING * w * self.speed) * h;
            self.lean += self.speed * h;
            self.nod_speed +=
                (PUSH * kick - n * n * self.nod - 2.0 * NOD_DAMPING * n * self.nod_speed) * h;
            self.nod += self.nod_speed * h;
        }
    }

    /// How far the leaves lean, in degrees to the right.
    pub fn lean(&self) -> f32 {
        (self.lean + self.nod).clamp(-MOST, MOST)
    }

    /// How fast they are swinging, in degrees a second to the right. The long
    /// leaves trail the short ones by it, so the plant ripples rather than
    /// turning all of a piece.
    pub fn speed(&self) -> f32 {
        self.speed + self.nod_speed
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

    /// Plays `seconds` of a steady song with `bass` under it at `fps`, and
    /// returns the fastest the leaves moved, in degrees a second.
    fn play(sway: &mut Sway, fps: u32, seconds: f32, bpm: f32, bass: f32) -> f32 {
        let mut fastest: f32 = 0.0;
        for frame in 0..(fps as f32 * seconds) as u32 {
            let t = frame as f32 / fps as f32;
            let before = sway.lean();
            sway.advance(1.0 / fps as f32, kick(bpm, t), bass);
            fastest = fastest.max((sway.lean() - before).abs() * fps as f32);
        }
        fastest
    }

    #[test]
    fn still_in_silence() {
        let mut sway = Sway::default();
        for _ in 0..300 {
            sway.advance(1.0 / 15.0, 0.0, 0.0);
        }
        assert_eq!(sway.lean(), 0.0);
    }

    #[test]
    fn a_steady_beat_leans_the_leaves_and_nods_them() {
        let mut sway = Sway::default();
        play(&mut sway, 30, 20.0, 120.0, 0.5);
        // Over the last few beats, a good lean that rises and falls with them.
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for frame in 0..60 {
            let t = 20.0 + frame as f32 / 30.0;
            sway.advance(1.0 / 30.0, kick(120.0, t), 0.5);
            low = low.min(sway.lean());
            high = high.max(sway.lean());
        }
        assert!(low >= 4.0 && high <= MOST * 0.8, "{low}..{high}");
        assert!(high - low >= 2.0, "nods only {}", high - low);
    }

    #[test]
    fn bass_alone_leans_them() {
        let mut sway = Sway::default();
        play(&mut sway, 30, 10.0, 120.0, 0.0);
        let kicks_only = sway.lean();
        let mut sway = Sway::default();
        for _ in 0..300 {
            sway.advance(1.0 / 30.0, 0.0, 0.5);
        }
        assert!((sway.lean() - BASS * 0.5).abs() < 0.2, "{}", sway.lean());
        assert!((-0.5..3.0).contains(&kicks_only), "{kicks_only}");
    }

    #[test]
    fn never_quicker_than_a_leaf_in_a_draft() {
        // A loud, fast song arriving all at once, its bass rising over a
        // second or so as `bass_att` does.
        let mut sway = Sway::default();
        let mut fastest: f32 = 0.0;
        for frame in 0..600 {
            let t = frame as f32 / 30.0;
            let before = sway.lean();
            sway.advance(1.0 / 30.0, kick(128.0, t), 1.0 - (-t).exp());
            fastest = fastest.max((sway.lean() - before).abs() * 30.0);
        }
        assert!(fastest < 30.0, "{fastest} degrees a second");
    }

    #[test]
    fn upright_again_some_seconds_after_the_music_stops() {
        let mut sway = Sway::default();
        play(&mut sway, 30, 10.0, 120.0, 0.5);
        // The bass fades over about a second, as `bass_att` does, and the
        // leaves follow it back without hurrying.
        let mut fastest: f32 = 0.0;
        for frame in 0..30 * 8 {
            let before = sway.lean();
            sway.advance(1.0 / 30.0, 0.0, 0.5 * (-(frame as f32) / 30.0).exp());
            fastest = fastest.max((sway.lean() - before).abs() * 30.0);
        }
        assert!(fastest < 30.0, "{fastest} degrees a second");
        assert!(sway.lean().abs() < 0.1, "{}", sway.lean());
    }

    #[test]
    fn the_same_at_15_and_30_fps() {
        // On average over the last two seconds, since the nods themselves
        // fall a little differently when the kicks are sampled half as often.
        let average = |fps: u32| {
            let mut sway = Sway::default();
            play(&mut sway, fps, 10.0, 120.0, 0.5);
            let mut sum = 0.0;
            for frame in 0..2 * fps {
                let t = 10.0 + frame as f32 / fps as f32;
                sway.advance(1.0 / fps as f32, kick(120.0, t), 0.5);
                sum += sway.lean();
            }
            sum / (2 * fps) as f32
        };
        let (slow, fast) = (average(15), average(30));
        assert!((slow - fast).abs() < 0.5, "{slow} vs {fast}");
    }

    #[test]
    fn a_long_gap_just_arrives() {
        let mut sway = Sway::default();
        play(&mut sway, 15, 5.0, 120.0, 0.5);
        sway.advance(30.0, 0.0, 0.0);
        assert_eq!(sway.lean(), 0.0);
    }
}
