//! `--bench`: draws the sill's frames into a texture of its own, as fast as it
//! can, and says how much CPU each one took.
//!
//! Each frame does all the work a real one does: it looks at the sun, eases the
//! light, hears a frame's worth of sound, moves the dust, the leaves and the
//! jar's ring, and draws the strip at the size it has on a 1440p screen at 150%.
//! It runs twice: idle at 15 fps, hearing silence (the analysis still works
//! through it, as it does in the real thing), and with music at 30 fps, hearing
//! a synthesized four-on-the-floor kick over a bass line.
//!
//! The time is the whole process's, on every thread and in the kernel, so the
//! driver's share counts too. With no GPU, as on CI, wgpu falls back to
//! Windows' software renderer and the shader runs on the CPU; that number is
//! much bigger, but still catches a shader or a loop that has grown.

use std::time::{Duration, Instant, SystemTime};

use anyhow::bail;

use crate::audio::analysis::Analyzer;
use crate::carry::Pose;
use crate::config::Config;
use crate::glass::{self, Painter};
use crate::pace::Pace;
use crate::placement::SILL_HEIGHT;
use crate::sill::{Sill, SillState};
use crate::sun::Place;
use crate::{cat, dust, night, ring, sunlight, sway};

/// The strip's size: a 2560-wide screen at 150%.
const SCALE: f64 = 1.5;
const WIDTH: u32 = 2560;

/// Frames drawn first and not counted, while the driver warms up.
const WARM_UP: u32 = 30;

/// Somewhere with a sun, so the light has work to do. Greenwich, by tradition.
const PLACE: Place = Place {
    latitude: 51.48,
    longitude: 0.0,
};

/// The sample rate the sound is synthesized at, as a typical output device's.
const SAMPLE_RATE: u32 = 48_000;

/// What one run measured.
struct Run {
    fps: u32,
    /// CPU time for each frame, on average.
    cpu: Duration,
    /// Wall time for each frame, on average.
    wall: Duration,
}

impl Run {
    /// The share of one core it would take at its frame rate.
    fn of_one_core(&self) -> f64 {
        self.cpu.as_secs_f64() * f64::from(self.fps) * 100.0
    }
}

/// Runs the bench for `frames` frames each, idle and with music, and fails if
/// either took more than `budget` of CPU a frame.
pub fn run(frames: u32, budget: Option<Duration>) -> anyhow::Result<()> {
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let (painter, adapter) = Painter::headless(glass::Path::from_env(), format, glass::SHADER)?;
    let height = (SILL_HEIGHT * SCALE).round() as u32;
    let texture = painter.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let config = Config::default();
    println!(
        "bench: {frames} frames each at {WIDTH}x{height} on {} ({:?}, {:?})",
        adapter.name, adapter.backend, adapter.device_type
    );

    let idle = measure(&painter, &view, [WIDTH, height], &config, frames, false);
    let music = measure(&painter, &view, [WIDTH, height], &config, frames, true);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    for (name, run) in [("idle ", &idle), ("music", &music)] {
        println!(
            "  {name} ({:2} fps): {:6.3} ms CPU a frame ({:6.3} ms wall) = {:5.2}% of one core, {:5.3}% of all {threads}",
            run.fps,
            run.cpu.as_secs_f64() * 1000.0,
            run.wall.as_secs_f64() * 1000.0,
            run.of_one_core(),
            run.of_one_core() / threads as f64,
        );
    }
    if adapter.device_type == wgpu::DeviceType::Cpu {
        println!(
            "  (drawn in software, so the shader's work is on the CPU too; a GPU costs far less)"
        );
    }
    if let Some(budget) = budget {
        let worst = idle.cpu.max(music.cpu);
        if worst > budget {
            bail!("a frame took {worst:.3?} of CPU, over the budget of {budget:.3?}");
        }
        println!("  within the budget of {budget:.3?} a frame");
    }
    Ok(())
}

/// Draws `frames` frames, after a few uncounted ones, the way the sill does at
/// its idle rate, or at the music's while hearing music.
fn measure(
    painter: &Painter,
    view: &wgpu::TextureView,
    size: [u32; 2],
    config: &Config,
    frames: u32,
    music: bool,
) -> Run {
    let fps = if music {
        config.fps_music
    } else {
        config.fps_idle
    };
    let dt = 1.0 / fps as f32;
    let per_frame = (SAMPLE_RATE / fps) as usize;
    let look = config.look();
    let pose = Pose::resting(
        [look.plant.at, look.jar.at, look.candle.at, look.cat.at],
        size[0] as f32 / SCALE as f32,
    );
    // From early morning, so the run crosses the moving light of sunrise.
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(20_000 * 86_400 + 5 * 3600);
    let mut sill = Sill::at(sunlight(config, Some(PLACE), SillState::default(), start));
    let mut analyzer = Analyzer::new(SAMPLE_RATE);
    let mut pace = Pace::default();
    let mut drift = dust::Drift::default();
    let mut sway = sway::Sway::default();
    let mut ring = ring::Ring::default();
    let mut cat = cat::Cat::default();
    let mut sound = vec![0.0; per_frame];
    let mut heard_at = 0;

    let mut began = (Instant::now(), cpu_time());
    for frame in 0..WARM_UP + frames {
        if frame == WARM_UP {
            painter.wait();
            began = (Instant::now(), cpu_time());
        }
        let seconds = f64::from(frame) / f64::from(fps);
        // A minute of the world each frame, so the sun is looked at every
        // frame, as it is when sped up, and actually moves.
        let world = start + Duration::from_secs(u64::from(frame) * 60);
        sill.target = sunlight(config, Some(PLACE), sill.target, world);
        sill.step(dt, &config.ease);
        if music {
            for sample in &mut sound {
                *sample = song(heard_at as f32 / SAMPLE_RATE as f32);
                heard_at += 1;
            }
        }
        analyzer.push(&sound);
        let heard = *analyzer.features();
        pace.hear(Instant::now(), &heard);
        sill.target.dust = dust::stir(heard.bass_att);
        sill.hear(dt, &config.ease);
        drift.advance(dt, sill.shown().dust, heard.beat_phase);
        sway.advance(dt, heard.kick, heard.bass_att);
        ring.advance(dt, heard.kick);
        cat.advance(
            dt,
            dt,
            cat::Feel {
                x: pose.x[crate::carry::CAT],
                beat: heard.beat,
                beat_phase: heard.beat_phase,
                kick: heard.kick,
                ..cat::Feel::default()
            },
        );
        painter.paint(
            view,
            size,
            SCALE,
            1.0,
            &look,
            &sill.shown(),
            night::flicker(seconds),
            [drift.at(), sway.lean(), ring.light(), sway.speed()],
            &pose,
            cat.shape(),
        );
        // One frame in flight, as the swapchain allows.
        painter.wait();
    }
    let (wall, cpu) = (began.0.elapsed(), cpu_time().zip(began.1));
    let cpu = cpu.map_or(wall, |(end, start)| end.saturating_sub(start));
    Run {
        fps,
        cpu: cpu / frames,
        wall: wall / frames,
    }
}

/// A plain song: a kick on every beat at 120 bpm, over a bass line that walks
/// between two notes each bar. `t` is in seconds.
fn song(t: f32) -> f32 {
    use std::f32::consts::TAU;
    let beat = t % 0.5;
    // A thump that falls from 110 to 50 Hz as it fades.
    let kick = (-beat * 18.0).exp()
        * (TAU * (50.0 * beat + 60.0 * (1.0 - (-beat * 30.0).exp()) / 30.0)).sin();
    let note = if t % 2.0 < 1.0 { 55.0 } else { 73.4 };
    let bass = 0.25 * (TAU * note * t).sin();
    0.6 * kick + bass
}

#[cfg(windows)]
use crate::win32::cpu_time;

/// Off Windows the process's CPU time isn't asked for, so wall time stands in.
#[cfg(not(windows))]
fn cpu_time() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_song_stays_in_range() {
        for i in 0..SAMPLE_RATE * 4 {
            let s = song(i as f32 / SAMPLE_RATE as f32);
            assert!(s.abs() <= 1.0, "{s} at sample {i}");
        }
    }

    #[test]
    fn the_song_is_heard_as_music() {
        let mut analyzer = Analyzer::new(SAMPLE_RATE);
        let samples: Vec<f32> = (0..SAMPLE_RATE * 4)
            .map(|i| song(i as f32 / SAMPLE_RATE as f32))
            .collect();
        let mut pace = Pace::default();
        let now = Instant::now();
        for chunk in samples.chunks(1600) {
            analyzer.push(chunk);
            pace.hear(now, analyzer.features());
        }
        assert!(pace.playing(now), "{:?}", analyzer.features());
    }
}
