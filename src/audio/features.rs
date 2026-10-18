//! What the sill hears, as the analysis thread last published it.
//!
//! Seoul's `AudioFeatures`, trimmed to the four fields the sill reads. The
//! rest (mid and treble, snare and hat, the spectrum and waveform, the link
//! lanes and the render clock) has no use on a windowsill, so it is gone.

/// One snapshot of the music, all 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioFeatures {
    /// The bass level averaged over about a second (MilkDrop's `bass_att`).
    pub bass_att: f32,
    /// Onset envelope: jumps to 1 on a detected beat, decays over ~100 ms.
    pub beat: f32,
    /// 0→1 sawtooth locked to the estimated tempo; wraps on each beat.
    pub beat_phase: f32,
    /// Jumps to 1 on a low thump and decays over ~100 ms. Independent of
    /// `beat`, which is the tempo-tracking onset.
    pub kick: f32,
}
