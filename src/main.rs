//! Windowsill: a thin, living strip that rests on top of the Windows taskbar.
//!
//! It rests on top of a bottom taskbar on the primary monitor and steps aside in
//! any other layout. It follows the taskbar through display, DPI and monitor
//! changes. Clicks fall through it, it never takes focus, and it stays out of
//! the taskbar and Alt+Tab. It draws a wooden ledge with a plant, a jar and a
//! candle on it in the light of the real sun, under glass that fades upward to
//! clear, and eases in over a few seconds when it comes to rest.
//! Its colours and frame rate come from `windowsill.toml`, re-read on every save.

mod config;
mod glass;
mod light;
mod objects;
mod palette;
mod placement;
mod presence;
mod settle;
mod sill;
mod sky;
mod sun;
#[cfg(windows)]
mod win32;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use config::{Config, Edited};
use glass::Glass;
use palette::Blend;
use placement::{Rect, SILL_HEIGHT, Unplaced};
use presence::Presence;
use settle::Settle;
use sill::{Sill, SillState};

#[cfg(windows)]
use win32::taskbar;

/// Off Windows there is no taskbar to rest on, so the sill always steps aside.
#[cfg(not(windows))]
fn taskbar() -> Option<placement::Taskbar> {
    None
}

/// How long to wait after a save for the editor to finish writing the file.
const QUIET: Duration = Duration::from_millis(150);

/// How often to look at the sun again. It moves a quarter of a degree a minute,
/// and the ease smooths each step, so this is plenty.
const SUN_EVERY: Duration = Duration::from_secs(20);

/// Where the light on the sill should be heading now, keeping whatever the
/// sun doesn't decide from `state`. With no place set it is plain daylight.
fn sunlight(config: &Config, state: SillState) -> SillState {
    match config.place().zip(config.room()) {
        Some((place, room)) => {
            let sun = sun::position(place, SystemTime::now());
            let shade = config.sky.at(sun);
            SillState {
                ambient: shade.ambient.0,
                vivid: shade.vivid,
                ..light::in_sunlight(state, room, sun)
            }
        }
        None => {
            let plain = SillState::default();
            SillState {
                beam_angle: plain.beam_angle,
                colour_temperature: plain.colour_temperature,
                shadow_side: plain.shadow_side,
                sunshine: plain.sunshine,
                window_side: plain.window_side,
                ambient: plain.ambient,
                vivid: plain.vivid,
                ..state
            }
        }
    }
}

/// Says where the sun is now, or that the sill doesn't know where it is.
fn note_the_sun(config: &Config) {
    let Some(place) = config.place() else {
        tracing::info!("no latitude and longitude set; the sill shows plain daylight");
        return;
    };
    let now = SystemTime::now();
    let sun = sun::position(place, now);
    let day = match sun::day(place, now) {
        sun::Day::Rises { rise, set } => {
            format!(
                "sunrise {}, sunset {}",
                from_now(rise, now),
                from_now(set, now)
            )
        }
        sun::Day::NeverSets => "it does not set today".into(),
        sun::Day::NeverRises => "it does not rise today".into(),
    };
    tracing::info!(
        "the sun is {:.1}° up at {:.0}°; {day}",
        sun.elevation,
        sun.azimuth
    );
}

/// `at` as a span of time before or after `now`, e.g. "in 3h 05m".
fn from_now(at: SystemTime, now: SystemTime) -> String {
    let (span, ago) = match at.duration_since(now) {
        Ok(ahead) => (ahead, false),
        Err(behind) => (behind.duration(), true),
    };
    let minutes = span.as_secs() / 60;
    let span = format!("{}h {:02}m", minutes / 60, minutes % 60);
    if ago {
        format!("{span} ago")
    } else {
        format!("in {span}")
    }
}

/// Why the event loop was woken from outside.
#[derive(Clone, Copy, Debug)]
pub enum Wake {
    /// The desktop may have changed shape. The payload says what was heard.
    Layout(&'static str),
    /// A file the sill reads was saved.
    Edited(Edited),
}

/// Saved files waiting to be read again once the writing stops.
#[derive(Clone, Copy)]
struct Pending {
    at: Instant,
    config: bool,
    shader: bool,
}

/// What the sill is doing, so a change is logged once rather than on every look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resting {
    Unknown,
    On(Rect),
    Aside(Unplaced),
}

struct App {
    #[cfg_attr(not(windows), allow(dead_code))]
    proxy: EventLoopProxy<Wake>,
    config_path: PathBuf,
    shader_path: Option<PathBuf>,
    config: Config,
    appearance: Blend,
    pending: Option<Pending>,
    window: Option<Arc<Window>>,
    glass: Option<Glass>,
    resting: Resting,
    settle: Settle,
    presence: Presence,
    sill: Sill,
    /// When the sill was last eased, so each frame eases by the time it took.
    eased: Instant,
    /// When the sun was last looked at.
    sun_seen: Instant,
    next_frame: Instant,
}

impl App {
    fn new(proxy: EventLoopProxy<Wake>) -> Self {
        let config_path = config::locate();
        tracing::info!("config: {}", config_path.display());
        let config = Config::load(&config_path).unwrap_or_else(|err| {
            tracing::error!("{err:#}; using the defaults until it is fixed");
            Config::default()
        });
        note_the_sun(&config);
        // Nothing is on show yet, so the sill starts in the light it should be in.
        let sill = Sill::at(sunlight(&config, SillState::default()));
        Self {
            proxy,
            shader_path: config::shader_source(),
            appearance: Blend::new(config.look()),
            config,
            config_path,
            pending: None,
            window: None,
            glass: None,
            resting: Resting::Unknown,
            settle: Settle::default(),
            presence: Presence::default(),
            sill,
            eased: Instant::now(),
            sun_seen: Instant::now(),
            next_frame: Instant::now(),
        }
    }

    /// Where the sill should be right now, or why it shouldn't be anywhere.
    fn place(event_loop: &ActiveEventLoop) -> Result<Rect, Unplaced> {
        let monitor = event_loop
            .primary_monitor()
            .ok_or(Unplaced::NoPrimaryMonitor)?;
        let origin = monitor.position();
        let size = monitor.size();
        let bounds = Rect {
            left: origin.x,
            top: origin.y,
            right: origin.x + size.width as i32,
            bottom: origin.y + size.height as i32,
        };
        let taskbar = taskbar();
        tracing::debug!(
            "primary monitor {bounds:?} at {}x, taskbar {taskbar:?}",
            monitor.scale_factor()
        );
        placement::place(bounds, monitor.scale_factor(), taskbar)
    }

    /// Opens the glass, hidden and not yet placed. [`App::rest`] moves and shows it.
    fn open_sill(&self, event_loop: &ActiveEventLoop, path: glass::Path) -> anyhow::Result<Window> {
        let mut attributes = WindowAttributes::default()
            .with_title("Windowsill")
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_active(false)
            .with_visible(false)
            .with_inner_size(LogicalSize::new(800.0, SILL_HEIGHT));

        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes
                .with_skip_taskbar(true)
                .with_no_redirection_bitmap(path.wants_no_redirection_bitmap());
        }

        let window = event_loop.create_window(attributes)?;
        // Through winit, so the click-through bits survive its own style rewrites.
        window.set_cursor_hittest(false)?;
        #[cfg(windows)]
        {
            win32::make_glass(&window)?;
            win32::watch_layout(&window, self.proxy.clone())?;
        }
        Ok(window)
    }

    /// Looks at the desktop and puts the sill where it belongs: resting on the
    /// taskbar, or hidden when there is nowhere it can rest. It is cheap enough to
    /// call whenever anything might have changed, and it only touches the window
    /// when the answer differs from what the window already is.
    fn rest(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let now = match Self::place(event_loop) {
            Ok(sill) => {
                let size = PhysicalSize::new(sill.width() as u32, sill.height() as u32);
                let position = PhysicalPosition::new(sill.left, sill.top);
                // Compared with the window itself rather than the last placement,
                // because winit resizes the window on its own when the DPI changes.
                if window.outer_position().ok() != Some(position) {
                    window.set_outer_position(position);
                }
                if window.inner_size() != size {
                    let _ = window.request_inner_size(size);
                }
                if !matches!(self.resting, Resting::On(_)) {
                    // The swapchain still holds the last frame from before it
                    // stepped aside; clear it so the sill eases in from nothing.
                    if let Some(glass) = &mut self.glass {
                        glass.draw(
                            window.scale_factor(),
                            0.0,
                            &self.appearance.at(Instant::now()),
                            &self.sill.shown(),
                        );
                    }
                    // With `with_active(false)` winit shows it with SW_SHOWNOACTIVATE.
                    window.set_visible(true);
                    self.presence.arrive(Instant::now());
                }
                Resting::On(sill)
            }
            Err(why) => {
                if matches!(self.resting, Resting::On(_)) {
                    window.set_visible(false);
                    self.presence.leave();
                }
                Resting::Aside(why)
            }
        };

        // winit rewrites the ex-style on every show, hide, move and resize, so the
        // glass styles go back on after each look.
        #[cfg(windows)]
        win32::make_glass(window)?;

        if now != self.resting {
            match now {
                Resting::On(sill) => tracing::info!(
                    "the sill rests at {}x{} +{}+{}",
                    sill.width(),
                    sill.height(),
                    sill.left,
                    sill.top
                ),
                Resting::Aside(why) => tracing::info!("stepping aside: {why}"),
                Resting::Unknown => {}
            }
            #[cfg(windows)]
            tracing::debug!(
                "sill ex-style {:#010x}",
                win32::ex_style(window).unwrap_or(0)
            );
            self.resting = now;
        }
        Ok(())
    }

    /// Time between frames while the room is quiet.
    fn frame(&self) -> Duration {
        Duration::from_secs(1) / self.config.fps_idle
    }

    /// The shader to draw with: its source on disk if that is there, otherwise
    /// the one built in.
    fn shader(&self) -> String {
        self.shader_path
            .as_ref()
            .and_then(|path| {
                std::fs::read_to_string(path)
                    .inspect_err(|err| tracing::warn!("could not read {}: {err}", path.display()))
                    .ok()
            })
            .unwrap_or_else(|| glass::SHADER.to_owned())
    }

    /// Reads again whatever was saved, keeping the last good version of anything
    /// that no longer makes sense.
    fn reload(&mut self, pending: Pending) {
        if pending.config {
            match Config::load(&self.config_path) {
                Ok(config) if config == self.config => {}
                Ok(config) => {
                    tracing::info!("config reloaded");
                    self.appearance.toward(config.look(), Instant::now());
                    if config.place() != self.config.place() {
                        note_the_sun(&config);
                    }
                    self.sill.target = sunlight(&config, self.sill.target);
                    self.config = config;
                }
                Err(err) => tracing::error!("{err:#}; keeping the last good config"),
            }
        }
        if pending.shader {
            let shader = self.shader();
            if let Some(glass) = &mut self.glass {
                match glass.reload(&shader) {
                    Ok(()) => tracing::info!("shader reloaded"),
                    Err(err) => {
                        tracing::error!("the shader would not build; keeping the last one: {err:#}")
                    }
                }
            }
        }
    }

    /// Looks again, and quits if the window can no longer be looked after.
    fn look(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.rest(event_loop) {
            tracing::error!("could not place the sill: {err:#}");
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let path = glass::Path::from_env();
        let opened = self.open_sill(event_loop, path).and_then(|window| {
            let window = Arc::new(window);
            let glass = match Glass::new(window.clone(), path, &self.shader()) {
                Err(err) if self.shader_path.is_some() => {
                    tracing::error!("{err:#}; drawing with the built-in shader");
                    Glass::new(window.clone(), path, glass::SHADER)
                }
                built => built,
            }?;
            Ok((window, glass))
        });
        match opened {
            Ok((window, glass)) => {
                self.window = Some(window);
                self.glass = Some(glass);
            }
            Err(err) => {
                tracing::error!("could not open the sill: {err:#}");
                event_loop.exit();
                return;
            }
        }
        self.look(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, wake: Wake) {
        match wake {
            Wake::Layout(what) => {
                tracing::debug!("{what}: looking at the taskbar again");
                self.settle.poke(Instant::now());
                self.look(event_loop);
            }
            Wake::Edited(edited) => {
                tracing::debug!("{edited:?} saved");
                // Each save pushes the read back, so a burst of writes is read once.
                let at = Instant::now() + QUIET;
                let mut pending = self.pending.unwrap_or(Pending {
                    at,
                    config: false,
                    shader: false,
                });
                pending.at = at;
                match edited {
                    Edited::Config => pending.config = true,
                    Edited::Shader => pending.shader = true,
                }
                self.pending = Some(pending);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
            WindowEvent::Resized(size) => {
                tracing::debug!("sill resized to {size:?}");
                if let Some(glass) = &mut self.glass {
                    glass.resize(size);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.sun_seen) >= SUN_EVERY {
                    self.sill.target = sunlight(&self.config, self.sill.target);
                    self.sun_seen = now;
                }
                self.sill.step(
                    now.duration_since(self.eased).as_secs_f32(),
                    &self.config.ease,
                );
                self.eased = now;
                if let (Some(glass), Some(window)) = (&mut self.glass, &self.window) {
                    glass.draw(
                        window.scale_factor(),
                        self.presence.at(now),
                        &self.appearance.at(now),
                        &self.sill.shown(),
                    );
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.settle.due(Instant::now()) {
            self.look(event_loop);
        }
        if let Some(pending) = self.pending
            && Instant::now() >= pending.at
        {
            self.pending = None;
            self.reload(pending);
        }
        // While the sill shows, draw at the idle rate. While it is aside, wake
        // only for events and the settling looks.
        let mut wake = self.settle.next();
        if let Some(pending) = self.pending {
            wake = Some(wake.map_or(pending.at, |at| at.min(pending.at)));
        }
        if let (Resting::On(_), Some(window)) = (self.resting, &self.window) {
            let now = Instant::now();
            if now >= self.next_frame {
                window.request_redraw();
                // Never try to catch up on missed frames; just keep the pace.
                self.next_frame = (self.next_frame + self.frame()).max(now);
            }
            wake = Some(wake.map_or(self.next_frame, |at| at.min(self.next_frame)));
        }
        event_loop.set_control_flow(match wake {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "windowsill=info".into()),
        )
        .init();

    tracing::info!("Windowsill {}", env!("CARGO_PKG_VERSION"));

    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(event_loop.create_proxy());
    // Without it the sill still runs; it just won't notice edits.
    let _watch = config::Watch::new(
        &app.config_path,
        app.shader_path.as_deref(),
        app.proxy.clone(),
    )
    .inspect_err(|err| tracing::warn!("not watching for edits: {err:#}"))
    .ok();
    event_loop.run_app(&mut app)?;
    Ok(())
}
