//! `--film`: draws the sill offscreen at an exact 30 fps and writes every frame
//! to a file, for the loop on chaps.dev (#31).
//!
//! It is the bench's frame with the rest of the sill's life put back: the cat
//! looks for the sun when the sky is looked at, walks there, and a tap at
//! `--wake` gets it up. The clock is the film's own, a fixed day at a fixed
//! place, so the same command films the same thing on any machine. The music
//! is the bench's synthesized song, for the first `--music` of the film.
//!
//! The file is raw BGRA with premultiplied alpha, as the glass hands it to the
//! compositor: the `KEPT` part of the strip, 1920 by 540, one frame after
//! another. ffmpeg lays it over a backdrop:
//!
//! ```text
//! ffmpeg -f rawvideo -pix_fmt bgra -s 1920x540 -r 30 -i day.raw ...
//! ```

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Context;

use crate::audio::analysis::Analyzer;
use crate::carry::{CAT, Carry};
use crate::config::Config;
use crate::glass::{self, Painter};
use crate::pace::Pace;
use crate::placement::SILL_HEIGHT;
use crate::sill::{Sill, SillState};
use crate::sun::Place;
use crate::{bench, cat, dust, night, ring, sunlight, sway};

/// The film's frame rate: the sill's own, while music plays.
const FPS: u32 = 30;

/// The strip, in logical pixels, and how many physical pixels each is: close
/// enough that the things fill a 1080p frame.
const WIDTH: f32 = 640.0;
const SCALE: f64 = 6.0;

/// The part of the strip that is kept, as shares of its width: the right half,
/// where the things stand by default, out to the end the cat can walk to.
const KEPT: [f32; 2] = [0.5, 1.0];

/// The day it was first filmed, 18 October 2026, at midnight UTC.
const DAY: u64 = 20_744 * 86_400;

/// Where, when none is given: Seattle, as in windowsill.example.toml.
const PLACE: Place = Place {
    latitude: 47.61,
    longitude: -122.33,
};

/// What to film.
pub struct Shot<'a> {
    pub file: &'a Path,
    pub length: Duration,
    /// How long a whole day takes; `None` holds the clock.
    pub day_in: Option<Duration>,
    /// The time of day it starts at, by the place's own sun.
    pub at: (u32, u32),
    pub place: Option<Place>,
    /// How long the music plays from the start.
    pub music: Duration,
    /// When a hand taps the cat awake.
    pub wake: Option<Duration>,
}

pub fn run(shot: &Shot) -> anyhow::Result<()> {
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let (painter, adapter) = Painter::headless(glass::Path::from_env(), format, glass::SHADER)?;
    let size = [
        (f64::from(WIDTH) * SCALE).round() as u32,
        (SILL_HEIGHT * SCALE).round() as u32,
    ];
    let texture = painter.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("film"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let kept = KEPT.map(|share| (share * size[0] as f32).round() as u32);
    let kept = wgpu::Extent3d {
        width: kept[1] - kept[0],
        height: size[1],
        depth_or_array_layers: 1,
    };
    let origin = (KEPT[0] * size[0] as f32).round() as u32;
    // 1920 * 4 bytes a row is already a multiple of wgpu's 256.
    let row = kept.width * 4;
    let readback = painter.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("film readback"),
        size: u64::from(row * size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut out = BufWriter::new(
        File::create(shot.file).with_context(|| format!("could not write {:?}", shot.file))?,
    );

    let place = shot.place.unwrap_or(PLACE);
    // The place's own clock, by its longitude, so 12:00 is near noon anywhere.
    let offset = (place.longitude / 15.0).round() as i64 * 3600;
    let (hours, minutes) = shot.at;
    let start = SystemTime::UNIX_EPOCH
        + Duration::from_secs(
            (DAY as i64 + i64::from(hours * 3600 + minutes * 60) - offset).max(0) as u64,
        );
    let speed = shot
        .day_in
        .map_or(0.0, |span| 86_400.0 / span.as_secs_f64());
    let pace = if speed > 0.0 { speed as f32 } else { 1.0 };
    let frames = (shot.length.as_secs_f64() * f64::from(FPS)).round() as u32;
    println!(
        "film: {frames} frames at {}x{} on {} into {:?}",
        kept.width, kept.height, adapter.name, shot.file
    );

    let config = Config::default();
    let look = config.look();
    let dt = 1.0 / FPS as f32;
    let per_frame = (bench::SAMPLE_RATE / FPS) as usize;
    let mut sill = Sill::at(sunlight(&config, Some(place), SillState::default(), start));
    let mut carry = Carry::new([look.plant.at, look.jar.at, look.candle.at, look.cat.at]);
    carry.step(0.0, WIDTH);
    let mut analyzer = Analyzer::new(bench::SAMPLE_RATE);
    let mut heard_pace = Pace::default();
    let mut drift = dust::Drift::default();
    let mut sway = sway::Sway::default();
    let mut ring = ring::Ring::default();
    let mut cat = cat::Cat::default();
    let mut sound = vec![0.0; per_frame];
    let mut heard_at = 0;
    let mut seen = start;

    for frame in 0..frames {
        let seconds = f64::from(frame) / f64::from(FPS);
        let world = start + Duration::from_secs_f64(seconds * speed);
        if frame == 0 || world.duration_since(seen).unwrap_or_default() >= Duration::from_secs(20)
        {
            sill.target = sunlight(&config, Some(place), sill.target, world);
            seen = world;
            if let Some((spots, width)) = carry.spots() {
                cat.seek(spots[CAT], spots, width, cat::Light::of(&sill.target));
            }
        }
        if shot.wake.is_some_and(|at| {
            let at = at.as_secs_f64();
            seconds <= at && at < seconds + f64::from(dt)
        }) && let Some((spots, width)) = carry.spots()
        {
            cat.wake(spots[CAT], spots, width, cat::Light::of(&sill.target));
        }
        sill.step(dt * pace, &config.ease);
        let music = seconds < shot.music.as_secs_f64();
        for sample in &mut sound {
            *sample = if music {
                bench::song(heard_at as f32 / bench::SAMPLE_RATE as f32)
            } else {
                0.0
            };
            heard_at += 1;
        }
        analyzer.push(&sound);
        let heard = *analyzer.features();
        heard_pace.hear(Instant::now(), &heard);
        sill.target.dust = dust::stir(heard.bass_att);
        sill.hear(dt, &config.ease);
        drift.advance(dt, sill.shown().dust, heard.beat_phase);
        sway.advance(dt, heard.kick, heard.bass_att);
        ring.advance(dt, heard.kick);
        cat.advance(
            dt,
            dt * pace,
            cat::Feel {
                x: carry.pose().x[CAT],
                beat: heard.beat,
                beat_phase: heard.beat_phase,
                kick: heard.kick,
                ..cat::Feel::default()
            },
        );
        if let Some(x) = cat.walking() {
            carry.walk(CAT, x);
        }
        if cat.arrived() {
            carry.arrive(CAT);
        }
        carry.step(dt, WIDTH);
        painter.paint(
            &view,
            size,
            SCALE,
            1.0,
            &look,
            &sill.shown(),
            night::flicker(seconds),
            [drift.at(), sway.lean(), ring.light(), sway.speed()],
            &carry.pose(),
            cat.shape(),
        );

        let mut encoder = painter
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("film") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                origin: wgpu::Origin3d {
                    x: origin,
                    y: 0,
                    z: 0,
                },
                ..texture.as_image_copy()
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(size[1]),
                },
            },
            kept,
        );
        painter.queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        painter.wait();
        out.write_all(&slice.get_mapped_range().context("could not read the frame back")?)?;
        readback.unmap();
        if frame % (FPS * 10) == 0 {
            println!("  {seconds:5.1} s");
        }
    }
    out.flush()?;
    Ok(())
}
