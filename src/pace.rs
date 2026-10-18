//! How often to draw: at the music rate while anything is playing, and back at
//! the idle rate a little after the room goes quiet. The music's answers are
//! all eased in real time, so the drop never shows; it only spares the machine.

use std::time::{Duration, Instant};

use crate::audio::features::AudioFeatures;

/// How loud the music must be, on any of the features the sill answers, to
/// count as playing. `bass_att` falls from a typical 0.5 to this in about two
/// and a half seconds of silence.
const PLAYING: f32 = 0.05;

/// How long after the last sound it keeps the music rate, so a quiet bar or the
/// gap between songs doesn't flicker it back and forth.
const LINGER: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct Pace {
    /// When something was last heard playing.
    heard: Option<Instant>,
    /// Whether the last frame was drawn at the music rate, so a change is
    /// noted once.
    playing: bool,
}

impl Pace {
    /// Takes in what the music is doing at `now`.
    pub fn hear(&mut self, now: Instant, music: &AudioFeatures) {
        if music.bass_att > PLAYING || music.beat > PLAYING || music.kick > PLAYING {
            self.heard = Some(now);
        }
        let playing = self.playing(now);
        if playing != self.playing {
            self.playing = playing;
            tracing::debug!(
                "{}",
                if playing {
                    "music: drawing at the music rate"
                } else {
                    "quiet: back to the idle rate"
                }
            );
        }
    }

    /// Whether music is playing at `now`, or was a moment ago.
    pub fn playing(&self, now: Instant) -> bool {
        self.heard
            .is_some_and(|heard| now.saturating_duration_since(heard) < LINGER)
    }

    /// Time between frames at `now`, given the two rates.
    pub fn frame(&self, now: Instant, fps_idle: u32, fps_music: u32) -> Duration {
        let fps = if self.playing(now) {
            fps_music
        } else {
            fps_idle
        };
        Duration::from_secs(1) / fps
    }
}

/// How many kicks to gather before the log says how quickly they were drawn.
const NOTE_EVERY: u32 = 20;

/// How long kicks take from reaching the analysis thread to being drawn, noted
/// in the debug log now and then. The sill's half of the latency check.
#[derive(Default)]
pub struct Lag {
    kicks: u32,
    total: Duration,
    worst: Duration,
}

impl Lag {
    /// A kick heard at `heard` has just been drawn at `drawn`.
    pub fn drawn(&mut self, heard: Instant, drawn: Instant) {
        let lag = drawn.saturating_duration_since(heard);
        self.kicks += 1;
        self.total += lag;
        self.worst = self.worst.max(lag);
        if self.kicks == NOTE_EVERY {
            tracing::debug!(
                "kicks drawn {:.1} ms after they were heard, at worst {:.1} ms",
                (self.total / self.kicks).as_secs_f64() * 1e3,
                self.worst.as_secs_f64() * 1e3
            );
            *self = Self::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at(pace: &Pace, now: Instant) -> Duration {
        pace.frame(now, 15, 30)
    }

    #[test]
    fn idle_until_something_plays() {
        let mut pace = Pace::default();
        let now = Instant::now();
        assert_eq!(frame_at(&pace, now), Duration::from_secs(1) / 15);
        pace.hear(now, &AudioFeatures::default());
        assert_eq!(frame_at(&pace, now), Duration::from_secs(1) / 15);
    }

    #[test]
    fn a_kick_alone_counts_as_playing() {
        let mut pace = Pace::default();
        let now = Instant::now();
        let kick = AudioFeatures {
            kick: 1.0,
            ..AudioFeatures::default()
        };
        pace.hear(now, &kick);
        assert_eq!(frame_at(&pace, now), Duration::from_secs(1) / 30);
    }

    #[test]
    fn drops_back_a_little_after_the_music_stops() {
        let mut pace = Pace::default();
        let start = Instant::now();
        let music = AudioFeatures {
            bass_att: 0.4,
            ..AudioFeatures::default()
        };
        pace.hear(start, &music);
        // Silence: still at the music rate for the linger, then idle.
        let quiet = AudioFeatures::default();
        let soon = start + Duration::from_secs(1);
        pace.hear(soon, &quiet);
        assert_eq!(frame_at(&pace, soon), Duration::from_secs(1) / 30);
        let later = start + LINGER + Duration::from_millis(1);
        pace.hear(later, &quiet);
        assert_eq!(frame_at(&pace, later), Duration::from_secs(1) / 15);
    }

    #[test]
    fn the_fading_bass_holds_the_rate_only_a_few_seconds() {
        // bass_att decays with a 1 s time constant once the music stops.
        let mut pace = Pace::default();
        let start = Instant::now();
        let mut idle_after = None;
        for frame in 0..15 * 10 {
            let t = frame as f32 / 15.0;
            let now = start + Duration::from_secs_f32(t);
            let music = AudioFeatures {
                bass_att: 0.5 * (-t).exp(),
                ..AudioFeatures::default()
            };
            pace.hear(now, &music);
            if !pace.playing(now) {
                idle_after = Some(t);
                break;
            }
        }
        let idle_after = idle_after.expect("never went idle");
        assert!((3.0..6.0).contains(&idle_after), "{idle_after} s");
    }
}
