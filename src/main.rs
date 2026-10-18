//! Windowsill: a thin, living strip that rests on top of the Windows taskbar.
//!
//! For now the sill is an empty pane of glass with nothing drawn on it yet.
//! It rests on top of a bottom taskbar on the primary monitor and steps aside in
//! any other layout. It follows the taskbar through display, DPI and monitor
//! changes. Clicks fall through it, it never takes focus, and it stays out of
//! the taskbar and Alt+Tab. For now it draws only a test band that fades to
//! fully see-through, the proof that the glass works (#4).

mod glass;
mod placement;
mod settle;
#[cfg(windows)]
mod win32;

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use glass::Glass;
use placement::{Rect, SILL_HEIGHT, Unplaced};
use settle::Settle;

#[cfg(windows)]
use win32::taskbar;

/// Off Windows there is no taskbar to rest on, so the sill always steps aside.
#[cfg(not(windows))]
fn taskbar() -> Option<placement::Taskbar> {
    None
}

/// The idle frame rate. Nothing on the sill ever needs more.
const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 15);

/// The desktop may have changed shape. The payload says what was heard.
#[derive(Clone, Copy, Debug)]
pub struct LayoutChanged(pub &'static str);

/// What the sill is doing, so a change is logged once rather than on every look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resting {
    Unknown,
    On(Rect),
    Aside(Unplaced),
}

struct App {
    #[cfg_attr(not(windows), allow(dead_code))]
    proxy: EventLoopProxy<LayoutChanged>,
    window: Option<Arc<Window>>,
    glass: Option<Glass>,
    resting: Resting,
    settle: Settle,
    next_frame: Instant,
}

impl App {
    fn new(proxy: EventLoopProxy<LayoutChanged>) -> Self {
        Self {
            proxy,
            window: None,
            glass: None,
            resting: Resting::Unknown,
            settle: Settle::default(),
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
                    // With `with_active(false)` winit shows it with SW_SHOWNOACTIVATE.
                    window.set_visible(true);
                }
                Resting::On(sill)
            }
            Err(why) => {
                if matches!(self.resting, Resting::On(_)) {
                    window.set_visible(false);
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

    /// Looks again, and quits if the window can no longer be looked after.
    fn look(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.rest(event_loop) {
            tracing::error!("could not place the sill: {err:#}");
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<LayoutChanged> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let path = glass::Path::from_env();
        let opened = self.open_sill(event_loop, path).and_then(|window| {
            let window = Arc::new(window);
            let glass = Glass::new(window.clone(), path)?;
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

    fn user_event(&mut self, event_loop: &ActiveEventLoop, LayoutChanged(what): LayoutChanged) {
        tracing::debug!("{what}: looking at the taskbar again");
        self.settle.poke(Instant::now());
        self.look(event_loop);
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
                if let Some(glass) = &mut self.glass {
                    glass.draw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.settle.due(Instant::now()) {
            self.look(event_loop);
        }
        // While the sill shows, draw at the idle rate. While it is aside, wake
        // only for events and the settling looks.
        let mut wake = self.settle.next();
        if let (Resting::On(_), Some(window)) = (self.resting, &self.window) {
            let now = Instant::now();
            if now >= self.next_frame {
                window.request_redraw();
                // Never try to catch up on missed frames; just keep the pace.
                self.next_frame = (self.next_frame + FRAME).max(now);
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

    let event_loop = EventLoop::<LayoutChanged>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}
