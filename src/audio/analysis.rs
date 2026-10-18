//! The thread that listens to the ring [`LoopbackCapture`] fills.
//!
//! For now it only drains the ring and notes how loud it is, so the capture
//! can be seen working. Seoul's analyzer, trimmed to the four features the
//! sill needs, takes its place next.
//!
//! [`LoopbackCapture`]: crate::audio::capture::LoopbackCapture

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use ringbuf::traits::Consumer;
use tracing::{debug, info};

/// A fresh ring of mono samples, sent each time the capture (re)opens.
pub struct AudioSource {
    pub consumer: ringbuf::HeapCons<f32>,
    pub sample_rate: u32,
}

/// How long the thread sleeps between draining the ring. The ring holds about
/// a third of a second at 48 kHz, so this never lets it overflow.
const NAP: Duration = Duration::from_millis(20);

/// How often the level is noted in the debug log.
const NOTE_EVERY: Duration = Duration::from_secs(2);

/// Spawns the listening thread. Returns a sender for (re)attaching sources.
pub fn spawn_analysis() -> Sender<AudioSource> {
    let (tx, rx) = channel();
    thread::Builder::new()
        .name("windowsill-audio".into())
        .spawn(move || listen(rx))
        .expect("spawn audio thread");
    tx
}

fn listen(sources: Receiver<AudioSource>) {
    let mut source: Option<ringbuf::HeapCons<f32>> = None;
    let mut level = Level::default();
    let mut noted = Instant::now();
    loop {
        // Latest source wins.
        loop {
            match sources.try_recv() {
                Ok(src) => {
                    info!(
                        sample_rate = src.sample_rate,
                        "listening to a new audio source"
                    );
                    source = Some(src.consumer);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                // The capture is gone, so the sill is closing.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            }
        }
        if let Some(c) = source.as_mut() {
            c.pop_iter().for_each(|s| level.hear(s));
        }
        if noted.elapsed() >= NOTE_EVERY {
            debug!(
                samples = level.samples,
                rms = format_args!("{:.4}", level.rms()),
                peak = format_args!("{:.4}", level.peak),
                "audio level"
            );
            level = Level::default();
            noted = Instant::now();
        }
        thread::sleep(NAP);
    }
}

/// How loud the samples heard since the last note were.
#[derive(Default)]
struct Level {
    samples: u64,
    squares: f64,
    peak: f32,
}

impl Level {
    fn hear(&mut self, sample: f32) {
        self.samples += 1;
        self.squares += f64::from(sample * sample);
        self.peak = self.peak.max(sample.abs());
    }

    fn rms(&self) -> f32 {
        if self.samples == 0 {
            0.0
        } else {
            (self.squares / self.samples as f64).sqrt() as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_has_no_level() {
        let level = Level::default();
        assert_eq!(level.rms(), 0.0);
        assert_eq!(level.peak, 0.0);
    }

    #[test]
    fn a_square_wave_is_as_loud_as_its_height() {
        let mut level = Level::default();
        for i in 0..100 {
            level.hear(if i % 2 == 0 { 0.5 } else { -0.5 });
        }
        assert!((level.rms() - 0.5).abs() < 1e-6);
        assert_eq!(level.peak, 0.5);
        assert_eq!(level.samples, 100);
    }
}
