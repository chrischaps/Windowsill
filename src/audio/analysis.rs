//! What the music is doing, from the ring [`LoopbackCapture`] fills.
//!
//! Seoul's analyzer, trimmed to the four features the sill reads: `bass_att`,
//! `beat`, `beat_phase` and `kick`. [`Analyzer`] is a pure, deterministic DSP
//! core: it consumes mono samples and advances on a fixed hop of the *sample*
//! clock, so every time constant below is in seconds of audio rather than
//! "per tick". The analysis thread wraps it with I/O concerns — draining the
//! capture ring, feeding real-time silence when WASAPI loopback stops
//! delivering packets, and swapping in a new source when capture reconnects.
//!
//! [`LoopbackCapture`]: crate::audio::capture::LoopbackCapture

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use ringbuf::traits::Consumer as _;
use tracing::{debug, info};
use triple_buffer::{Input, Output, TripleBuffer};

use crate::audio::features::AudioFeatures;

const FFT_SIZE: usize = 2048;
/// Analysis hop in samples (~5.3 ms at 48 kHz).
const HOP: usize = 256;

/// Absolute floor below which everything reads as silence (dBFS-ish, where a
/// full-scale sine maps to 0 dB). Keeps the AGC from amplifying noise.
const FLOOR_DB: f32 = -50.0;

// The bass envelope.
const BAND_ATTACK: f32 = 0.012;
const BAND_RELEASE: f32 = 0.150;
const BAND_REF_RELEASE: f32 = 3.0;
const ATT_TAU: f32 = 1.0;

// Onset / beat detection.
const ONSET_HISTORY_S: f32 = 1.0;
const ONSET_K: f32 = 1.5;
const ONSET_MIN: f32 = 0.02;
const BEAT_REFRACTORY: f32 = 0.18;
const BEAT_TAU: f32 = 0.10;

// The kick: refractory s, envelope tau s, the fraction of the band's recent
// peak an onset must reach, and a floor. It uses linear-magnitude flux, since
// bass beating against a kick's tail is a large *log* change but a small
// linear one.
//
// Unlike Seoul, the flux is measured against the bass reference, so the floor
// is a fraction of how loud the bass has been rather than an absolute level.
// Loopback can arrive 30× below full scale, and Seoul's absolute floor heard
// no kicks at all there. Kicks reach about 0.3 of the reference at any volume;
// -70 dBFS hiss reaches about 0.0014.
const KICK_TIMING: OnsetTiming = OnsetTiming {
    refractory: 0.12,
    tau: 0.10,
    rel: 0.4,
    floor: 0.01,
};

// Tempo estimation.
const TEMPO_WINDOW_S: f32 = 10.0;
const BPM_MIN: f32 = 80.0;
const BPM_MAX: f32 = 160.0;
const BPM_DEFAULT: f32 = 120.0;

/// How often what was heard is noted in the debug log.
const NOTE_EVERY: Duration = Duration::from_secs(2);

/// A fresh ring of mono samples, sent each time the capture (re)opens.
pub struct AudioSource {
    pub consumer: ringbuf::HeapCons<f32>,
    pub sample_rate: u32,
}

/// Spawns the analysis thread. Returns the features reader for the sill and a
/// sender for (re)attaching audio sources.
pub fn spawn_analysis() -> (Output<AudioFeatures>, Sender<AudioSource>) {
    let (input, output) = TripleBuffer::new(&AudioFeatures::default()).split();
    let (tx, rx) = channel();
    thread::Builder::new()
        .name("windowsill-audio".into())
        .spawn(move || analysis_loop(rx, input))
        .expect("spawn audio thread");
    (output, tx)
}

fn analysis_loop(sources: Receiver<AudioSource>, mut out: Input<AudioFeatures>) {
    // Until a source arrives we run at a nominal rate on synthesized silence.
    let mut source: Option<ringbuf::HeapCons<f32>> = None;
    let mut analyzer = Analyzer::new(48_000);
    let mut buf: Vec<f32> = Vec::with_capacity(8192);
    let zeros = vec![0.0f32; 4096];

    let mut last_real = Instant::now();
    let mut silence_clock = Instant::now();
    let mut note = Note::new(&analyzer);

    loop {
        // Latest source wins; a sample-rate change rebuilds the analyzer.
        loop {
            match sources.try_recv() {
                Ok(src) => {
                    if src.sample_rate as f32 != analyzer.sample_rate {
                        analyzer = Analyzer::new(src.sample_rate);
                        note = Note::new(&analyzer);
                    }
                    source = Some(src.consumer);
                    info!(
                        sample_rate = src.sample_rate,
                        "listening to a new audio source"
                    );
                }
                Err(TryRecvError::Empty) => break,
                // The capture is gone, so the sill is closing.
                Err(TryRecvError::Disconnected) => return,
            }
        }

        buf.clear();
        if let Some(c) = source.as_mut() {
            buf.extend(c.pop_iter());
        }

        let now = Instant::now();
        let mut published = false;
        if !buf.is_empty() {
            published = analyzer.push(&buf);
            last_real = now;
            silence_clock = now;
        } else if now.duration_since(last_real) > Duration::from_millis(40) {
            // WASAPI loopback sends nothing while the system is silent. Feed
            // real-time zeros so every envelope decays exactly as it would on
            // genuine digital silence.
            let owed =
                (now.duration_since(silence_clock).as_secs_f32() * analyzer.sample_rate) as usize;
            if owed >= HOP {
                let n = owed.min(zeros.len());
                published = analyzer.push(&zeros[..n]);
                silence_clock = now;
            }
        }

        if published {
            out.write(*analyzer.features());
        }
        if note.at.elapsed() >= NOTE_EVERY {
            note.write(&analyzer);
        }
        thread::sleep(Duration::from_millis(3));
    }
}

/// What the debug log last said, so the next note can say what changed. The
/// envelopes last only ~100 ms, so a note counts the beats and kicks since
/// the last one rather than showing where they happen to stand.
struct Note {
    at: Instant,
    beats: u32,
    kicks: u32,
}

impl Note {
    fn new(analyzer: &Analyzer) -> Self {
        Self {
            at: Instant::now(),
            beats: analyzer.beat_count,
            kicks: analyzer.kick.hits,
        }
    }

    fn write(&mut self, analyzer: &Analyzer) {
        let f = analyzer.features();
        debug!(
            bass_att = format_args!("{:.2}", f.bass_att),
            beats = analyzer.beat_count - self.beats,
            kicks = analyzer.kick.hits - self.kicks,
            bpm = format_args!("{:.1}", analyzer.bpm),
            confidence = format_args!("{:.2}", analyzer.bpm_confidence),
            beat_phase = format_args!("{:.2}", f.beat_phase),
            "heard"
        );
        *self = Self::new(analyzer);
    }
}

pub struct Analyzer {
    sample_rate: f32,
    hop_dt: f32,

    fft: Arc<dyn RealToComplex<f32>>,
    fft_in: Vec<f32>,
    fft_out: Vec<Complex<f32>>,
    fft_scratch: Vec<Complex<f32>>,
    window: Vec<f32>,
    mags: Vec<f32>,
    log_mags: Vec<f32>,
    prev_log_mags: Vec<f32>,
    prev_mags: Vec<f32>,

    rolling: Vec<f32>,
    write_pos: usize,
    pending: usize,

    bass_range: (usize, usize),
    kick_range: (usize, usize),
    flux_hi_bin: usize,

    bass_ref: f32,
    bass: f32,
    bass_att: f32,

    onset_hist: VecDeque<f32>,
    onset_hist_len: usize,
    onset_prev: [f32; 2],
    since_beat: f32,
    beat: f32,
    beat_count: u32,
    kick: OnsetDetector,

    clock: f32,
    onset_times: VecDeque<f32>,
    bpm: f32,
    bpm_confidence: f32,
    phase: f32,

    features: AudioFeatures,
}

impl Analyzer {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate as f32;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let fft_in = fft.make_input_vec();
        let fft_out = fft.make_output_vec();
        let fft_scratch = fft.make_scratch_vec();
        let n_bins = FFT_SIZE / 2 + 1;

        let window = (0..FFT_SIZE)
            .map(|i| 0.5 * (1.0 - (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos()))
            .collect();

        let hz_per_bin = sr / FFT_SIZE as f32;
        let hz = |f: f32| ((f / hz_per_bin) as usize).clamp(1, n_bins - 1);
        let bass_range = (hz(40.0), hz(250.0));

        info!(
            fft_size = FFT_SIZE,
            hop = HOP,
            sample_rate,
            bass = ?bass_range,
            "analyzer ready"
        );

        let hop_dt = HOP as f32 / sr;
        let onset_hist_len = (ONSET_HISTORY_S / hop_dt) as usize;

        Self {
            sample_rate: sr,
            hop_dt,
            fft,
            fft_in,
            fft_out,
            fft_scratch,
            window,
            mags: vec![0.0; n_bins],
            log_mags: vec![0.0; n_bins],
            prev_log_mags: vec![0.0; n_bins],
            prev_mags: vec![0.0; n_bins],
            rolling: vec![0.0; FFT_SIZE],
            write_pos: 0,
            pending: 0,
            bass_range,
            kick_range: (hz(30.0), hz(180.0)),
            flux_hi_bin: hz(8000.0),
            bass_ref: db_to_amp(FLOOR_DB),
            bass: 0.0,
            bass_att: 0.0,
            onset_hist: VecDeque::with_capacity(onset_hist_len + 1),
            onset_hist_len,
            onset_prev: [0.0; 2],
            since_beat: f32::INFINITY,
            beat: 0.0,
            beat_count: 0,
            kick: OnsetDetector::new(onset_hist_len, KICK_TIMING),
            clock: 0.0,
            onset_times: VecDeque::new(),
            bpm: BPM_DEFAULT,
            bpm_confidence: 0.0,
            phase: 0.0,
            features: AudioFeatures::default(),
        }
    }

    pub fn features(&self) -> &AudioFeatures {
        &self.features
    }

    /// Feed mono samples. Returns true if at least one hop was analyzed
    /// (i.e. `features()` changed).
    pub fn push(&mut self, samples: &[f32]) -> bool {
        let mut ran = false;
        for &s in samples {
            self.rolling[self.write_pos] = s;
            self.write_pos = (self.write_pos + 1) % FFT_SIZE;
            self.pending += 1;
            if self.pending >= HOP {
                self.pending = 0;
                self.hop();
                ran = true;
            }
        }
        ran
    }

    fn hop(&mut self) {
        let dt = self.hop_dt;
        self.clock += dt;

        // Spectrum via windowed real FFT over the ring, oldest → newest.
        // Normalize so a full-scale sine peaks at ~1.0 (Hann coherent gain
        // 0.5 × N/2).
        for i in 0..FFT_SIZE {
            self.fft_in[i] = self.rolling[(self.write_pos + i) % FFT_SIZE] * self.window[i];
        }
        self.fft
            .process_with_scratch(&mut self.fft_in, &mut self.fft_out, &mut self.fft_scratch)
            .expect("FFT buffer sizes are fixed at construction");
        let norm = 4.0 / FFT_SIZE as f32;
        for (m, c) in self.mags.iter_mut().zip(self.fft_out.iter()) {
            *m = c.norm() * norm;
        }

        self.update_bass(dt);
        self.update_beat(dt);
        self.update_tempo(dt);

        self.features = AudioFeatures {
            bass_att: self.bass_att,
            beat: self.beat,
            beat_phase: self.phase,
            kick: self.kick.env,
        };
    }

    fn update_bass(&mut self, dt: f32) {
        let floor = db_to_amp(FLOOR_DB);
        let (lo, hi) = self.bass_range;
        // Band energy, not mean: a wide band isn't diluted by bin count.
        let raw = self.mags[lo..hi].iter().map(|m| m * m).sum::<f32>().sqrt();
        // AGC: the reference tracks recent peaks and relaxes toward the
        // floor, so the bass uses its full 0..1 range.
        let ref_decay = (-dt / BAND_REF_RELEASE).exp();
        self.bass_ref = raw.max(floor + (self.bass_ref - floor) * ref_decay);
        let target = if raw > floor {
            raw / self.bass_ref
        } else {
            0.0
        };
        self.bass = follow(self.bass, target, dt, BAND_ATTACK, BAND_RELEASE);
        self.bass_att = follow(self.bass_att, self.bass, dt, ATT_TAU, ATT_TAU);
    }

    fn update_beat(&mut self, dt: f32) {
        // Log-compressed magnitudes make flux roughly gain-invariant.
        for (l, &m) in self.log_mags.iter_mut().zip(self.mags.iter()) {
            *l = (1.0 + 1000.0 * m).ln();
        }
        let flux = |range: (usize, usize), cur: &[f32], prev: &[f32]| -> f32 {
            let (lo, hi) = range;
            let sum: f32 = cur[lo..hi]
                .iter()
                .zip(&prev[lo..hi])
                .map(|(c, p)| (c - p).max(0.0))
                .sum();
            sum / (hi - lo).max(1) as f32
        };
        let kick = flux(self.kick_range, &self.log_mags, &self.prev_log_mags);
        let broad = flux((1, self.flux_hi_bin), &self.log_mags, &self.prev_log_mags);
        // `bass_ref` never falls below the floor, so this never divides by zero.
        let kick_lin = flux(self.kick_range, &self.mags, &self.prev_mags) / self.bass_ref;
        self.prev_mags.copy_from_slice(&self.mags);
        self.kick.step(kick_lin, dt);
        std::mem::swap(&mut self.log_mags, &mut self.prev_log_mags);
        let onset = kick + 0.5 * broad;

        // Adaptive threshold from the last ~1 s of onset strength.
        let n = self.onset_hist.len().max(1) as f32;
        let mean = self.onset_hist.iter().sum::<f32>() / n;
        let var = self
            .onset_hist
            .iter()
            .map(|o| (o - mean).powi(2))
            .sum::<f32>()
            / n;
        let threshold = mean + ONSET_K * var.sqrt() + ONSET_MIN;

        // Peak-pick one hop late: the previous value must be a local maximum.
        let [prev2, prev1] = self.onset_prev;
        self.since_beat += dt;
        let is_peak = prev1 > prev2 && prev1 >= onset && prev1 > threshold;
        if is_peak && self.since_beat > BEAT_REFRACTORY {
            self.since_beat = 0.0;
            self.beat = 1.0;
            self.beat_count += 1;
            self.on_beat();
        } else {
            self.beat *= (-dt / BEAT_TAU).exp();
        }

        self.onset_prev = [prev1, onset];
        self.onset_hist.push_back(onset);
        if self.onset_hist.len() > self.onset_hist_len {
            self.onset_hist.pop_front();
        }
    }

    fn on_beat(&mut self) {
        let now = self.clock;
        self.onset_times.push_back(now);
        while self
            .onset_times
            .front()
            .is_some_and(|&t| now - t > TEMPO_WINDOW_S)
        {
            self.onset_times.pop_front();
        }

        // Nudge the phase oscillator toward 0 when a beat lands near it.
        let err = if self.phase > 0.5 {
            self.phase - 1.0
        } else {
            self.phase
        };
        if err.abs() < 0.3 {
            self.phase = (self.phase - err * 0.3).rem_euclid(1.0);
        }

        if let Some((bpm, conf)) = estimate_tempo(&self.onset_times) {
            self.bpm_confidence = conf;
            if conf > 0.2 {
                // Snap on big confident jumps (new song), glide otherwise.
                let k = if (bpm - self.bpm).abs() > 8.0 && conf > 0.4 {
                    1.0
                } else {
                    0.25
                };
                self.bpm += (bpm - self.bpm) * k;
            }
        }
    }

    fn update_tempo(&mut self, dt: f32) {
        self.phase = (self.phase + dt * self.bpm / 60.0).rem_euclid(1.0);
        // Confidence fades if beats stop arriving.
        if self.since_beat > 2.0 {
            self.bpm_confidence *= (-dt / 2.0).exp();
        }
    }
}

/// When one band's onset counts, for [`OnsetDetector`].
#[derive(Clone, Copy)]
struct OnsetTiming {
    refractory: f32,
    tau: f32,
    rel: f32,
    floor: f32,
}

/// Adaptive-threshold peak picker for one band's onset strength, with a
/// decaying 0..1 envelope. Same scheme as the main beat detector, minus
/// the tempo tracking.
struct OnsetDetector {
    hist: VecDeque<f32>,
    hist_len: usize,
    prev: [f32; 2],
    since: f32,
    timing: OnsetTiming,
    env: f32,
    /// Onsets reported so far.
    hits: u32,
}

impl OnsetDetector {
    fn new(hist_len: usize, timing: OnsetTiming) -> Self {
        Self {
            hist: VecDeque::with_capacity(hist_len + 1),
            hist_len,
            prev: [0.0; 2],
            since: f32::INFINITY,
            timing,
            env: 0.0,
            hits: 0,
        }
    }

    /// Feed this hop's onset strength.
    fn step(&mut self, onset: f32, dt: f32) {
        let n = self.hist.len().max(1) as f32;
        let mean = self.hist.iter().sum::<f32>() / n;
        let var = self.hist.iter().map(|o| (o - mean).powi(2)).sum::<f32>() / n;
        let peak = self.hist.iter().fold(0.0f32, |m, &o| m.max(o));
        let threshold =
            (mean + ONSET_K * var.sqrt() + self.timing.floor).max(peak * self.timing.rel);

        let [prev2, prev1] = self.prev;
        self.since += dt;
        self.env *= (-dt / self.timing.tau).exp();
        if prev1 > prev2
            && prev1 >= onset
            && prev1 > threshold
            && self.since > self.timing.refractory
        {
            self.since = 0.0;
            self.env = 1.0;
            self.hits += 1;
        }

        self.prev = [prev1, onset];
        self.hist.push_back(onset);
        if self.hist.len() > self.hist_len {
            self.hist.pop_front();
        }
    }
}

/// Inter-onset-interval histogram tempo estimate, folded into BPM_MIN..BPM_MAX.
/// Returns (bpm, confidence 0..1) once enough onsets are available.
fn estimate_tempo(onsets: &VecDeque<f32>) -> Option<(f32, f32)> {
    if onsets.len() < 6 {
        return None;
    }
    const BINS: usize = (BPM_MAX - BPM_MIN) as usize;
    let mut hist = [0.0f32; BINS];
    let times: Vec<f32> = onsets.iter().copied().collect();
    for i in 0..times.len() {
        for j in (i + 1)..times.len() {
            let d = times[j] - times[i];
            if d > 2.0 {
                break;
            }
            if d < 0.25 {
                continue;
            }
            let mut bpm = 60.0 / d;
            while bpm < BPM_MIN {
                bpm *= 2.0;
            }
            while bpm >= BPM_MAX {
                bpm /= 2.0;
            }
            // Adjacent onsets carry the most tempo information.
            let w = 1.0 / (j - i) as f32;
            for (k, h) in hist.iter_mut().enumerate() {
                let c = BPM_MIN + k as f32 + 0.5;
                let z = (c - bpm) / 1.5;
                *h += w * (-0.5 * z * z).exp();
            }
        }
    }
    let total: f32 = hist.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let (peak, _) = hist.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    // Parabolic refinement around the peak bin.
    let y0 = hist[peak.saturating_sub(1)];
    let y1 = hist[peak];
    let y2 = hist[(peak + 1).min(BINS - 1)];
    let denom = y0 - 2.0 * y1 + y2;
    let offset = if denom.abs() > 1e-6 {
        0.5 * (y0 - y2) / denom
    } else {
        0.0
    };
    let bpm = BPM_MIN + peak as f32 + 0.5 + offset.clamp(-0.5, 0.5);

    let lo = peak.saturating_sub(3);
    let hi = (peak + 4).min(BINS);
    let near: f32 = hist[lo..hi].iter().sum();
    Some((bpm, (near / total).clamp(0.0, 1.0)))
}

/// One-pole follower with separate rise/fall time constants (seconds).
#[inline]
fn follow(cur: f32, target: f32, dt: f32, rise: f32, fall: f32) -> f32 {
    let tau = if target > cur { rise } else { fall };
    cur + (target - cur) * (1.0 - (-dt / tau).exp())
}

#[inline]
fn db_to_amp(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 44_100;

    /// Run `seconds` of a generated signal through a fresh analyzer, calling
    /// `each` with the time and features after every hop.
    fn run(
        seconds: f32,
        mut signal: impl FnMut(f32) -> f32,
        mut each: impl FnMut(f32, &AudioFeatures),
    ) -> Analyzer {
        let mut a = Analyzer::new(SR);
        let n = (seconds * SR as f32) as usize;
        let mut chunk = Vec::with_capacity(HOP);
        for i in 0..n {
            chunk.push(signal(i as f32 / SR as f32));
            if chunk.len() == HOP {
                a.push(&chunk);
                chunk.clear();
                each((i + 1) as f32 / SR as f32, a.features());
            }
        }
        a
    }

    /// Deterministic four-on-the-floor at `bpm`: pitched-down kick + soft pad.
    fn kick_at(bpm: f32, t: f32) -> f32 {
        let beat_t = t % (60.0 / bpm);
        let kick = (-beat_t * 18.0).exp()
            * (std::f32::consts::TAU * (50.0 + 90.0 * (-beat_t * 30.0).exp()) * beat_t).sin();
        let pad = 0.05 * (std::f32::consts::TAU * 330.0 * t).sin();
        0.8 * kick + pad
    }

    fn kick_track(t: f32) -> f32 {
        kick_at(120.0, t)
    }

    #[test]
    fn silence_stays_still() {
        let a = run(3.0, |_| 0.0, |_, _| {});
        assert_eq!(a.beat_count, 0);
        assert_eq!(a.kick.hits, 0);
        let f = a.features();
        assert!(f.bass_att < 1e-3 && f.beat < 1e-3 && f.kick < 1e-3);
    }

    #[test]
    fn near_silent_noise_is_not_amplified() {
        // -70 dBFS hiss: well below the floor, must not stir anything.
        let mut seed = 1u32;
        let a = run(
            3.0,
            |_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * 3e-4
            },
            |_, _| {},
        );
        assert!(a.features().bass_att < 0.05, "{:?}", a.features());
        assert_eq!(a.beat_count, 0);
        assert_eq!(a.kick.hits, 0, "kicks on hiss");
    }

    #[test]
    fn detects_120_bpm_kick() {
        let a = run(12.0, kick_track, |_, _| {});
        // 24 kicks in 12 s; allow a miss at the start while history fills.
        assert!((21..=25).contains(&a.beat_count), "beats {}", a.beat_count);
        assert!((21..=25).contains(&a.kick.hits), "kicks {}", a.kick.hits);
        assert!((a.bpm - 120.0).abs() < 2.0, "bpm {}", a.bpm);
        assert!(a.bpm_confidence > 0.3, "confidence {}", a.bpm_confidence);
    }

    #[test]
    fn hears_kicks_played_quietly() {
        // Loopback often arrives far below full scale; the kick is still a kick.
        for gain in [0.1f32, 0.03, 0.01] {
            let a = run(12.0, |t| gain * kick_track(t), |_, _| {});
            assert!(
                (21..=25).contains(&a.kick.hits),
                "gain {gain}: kicks {}",
                a.kick.hits
            );
        }
    }

    #[test]
    fn tracks_tempos_other_than_the_default() {
        // 120 is the initial estimate, so prove the estimator actually moves.
        for target in [100.0f32, 140.0] {
            let a = run(14.0, |t| kick_at(target, t), |_, _| {});
            assert!(
                (a.bpm - target).abs() < 2.5,
                "target {target}, got {}",
                a.bpm
            );
        }
    }

    #[test]
    fn beat_and_kick_decay_between_kicks() {
        let mut lingered = 0.0f32;
        run(4.0, kick_track, |t, f| {
            // 350–450 ms after each kick, the envelopes must have mostly decayed.
            if t > 1.0 && (0.35..0.45).contains(&(t % 0.5)) {
                lingered = lingered.max(f.beat).max(f.kick);
            }
        });
        assert!(lingered < 0.1, "an envelope lingered at {lingered}");
    }

    #[test]
    fn beat_phase_wraps_on_the_kick() {
        // Once the tempo is found, each kick lands near the start of a phase.
        let mut worst = 0.0f32;
        let mut checked = 0;
        run(12.0, kick_track, |t, f| {
            if t > 6.0 && f.kick == 1.0 {
                let off = f.beat_phase.min(1.0 - f.beat_phase);
                worst = worst.max(off);
                checked += 1;
            }
        });
        assert!(checked >= 10, "only {checked} kicks checked");
        assert!(worst < 0.15, "a kick landed {worst} of a beat off");
    }

    #[test]
    fn beat_phase_rises_steadily() {
        // A sawtooth: it only ever climbs, except when it wraps.
        let mut prev = 0.0f32;
        let mut wraps = 0;
        run(4.0, kick_track, |_, f| {
            if f.beat_phase < prev - 0.5 {
                wraps += 1;
            }
            assert!(
                f.beat_phase >= prev - 0.1 || f.beat_phase < prev - 0.5,
                "phase fell from {prev} to {}",
                f.beat_phase
            );
            prev = f.beat_phase;
        });
        // About two beats a second at 120 BPM.
        assert!((7..=9).contains(&wraps), "wrapped {wraps} times in 4 s");
    }

    #[test]
    fn bass_att_swells_slowly_and_settles() {
        let mut after_a_tenth = None;
        let mut a = run(3.0, kick_track, |t, f| {
            if after_a_tenth.is_none() && t >= 0.1 {
                after_a_tenth = Some(f.bass_att);
            }
        });
        // It is an average over about a second, so the first kick barely moves it.
        let early = after_a_tenth.unwrap();
        let swelled = a.features().bass_att;
        assert!(early < 0.15, "bass_att jumped to {early}");
        assert!(swelled > 0.3, "bass_att only reached {swelled}");
        // Then silence: it falls back toward zero over a few seconds.
        a.push(&vec![0.0; 5 * SR as usize]);
        assert!(a.features().bass_att < 0.02, "{:?}", a.features());
    }
}
