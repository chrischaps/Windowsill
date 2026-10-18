//! Windowsill: a thin, living strip that rests on top of the Windows taskbar.
//!
//! For now the sill is an empty pane of glass with nothing drawn on it yet.
//! It rests on top of a bottom taskbar on the primary monitor and steps aside in
//! any other layout. Clicks fall through it, it never takes focus, and it stays
//! out of the taskbar and Alt+Tab. Drawing arrives with the rest of M1.

mod placement;
#[cfg(windows)]
mod win32;

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use placement::{Rect, SILL_HEIGHT, Unplaced};

#[cfg(windows)]
use win32::taskbar;

/// Off Windows there is no taskbar to rest on, so the sill always steps aside.
#[cfg(not(windows))]
fn taskbar() -> Option<placement::Taskbar> {
    None
}

#[derive(Default)]
struct App {
    window: Option<Window>,
}

impl App {
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

    fn open_sill(&self, event_loop: &ActiveEventLoop) -> anyhow::Result<Window> {
        let mut attributes = WindowAttributes::default()
            .with_title("Windowsill")
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_active(false)
            .with_visible(false);

        let placed = Self::place(event_loop);
        attributes = match placed {
            Ok(sill) => attributes
                .with_inner_size(PhysicalSize::new(sill.width() as u32, sill.height() as u32))
                .with_position(PhysicalPosition::new(sill.left, sill.top)),
            Err(_) => attributes.with_inner_size(LogicalSize::new(800.0, SILL_HEIGHT)),
        };

        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes.with_skip_taskbar(true);
        }

        let window = event_loop.create_window(attributes)?;
        // Through winit, so the click-through bits survive its own style rewrites.
        window.set_cursor_hittest(false)?;
        #[cfg(windows)]
        win32::make_glass(&window)?;

        // In a layout v1 doesn't support, the glass stays up but hidden, so it can
        // come back when the taskbar does (#3).
        if let Err(why) = placed {
            tracing::info!("stepping aside: {why}");
            return Ok(window);
        }

        // Shown last, once it is already glass. With `with_active(false)` winit
        // shows it with SW_SHOWNOACTIVATE, but that show rewrites the ex-style, so
        // the glass styles go back on straight after.
        window.set_visible(true);
        #[cfg(windows)]
        {
            win32::make_glass(&window)?;
            tracing::debug!(
                "sill ex-style {:#010x}",
                win32::ex_style(&window).unwrap_or(0)
            );
        }

        Ok(window)
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match self.open_sill(event_loop) {
            Ok(window) => {
                tracing::info!(
                    "the sill is up: {:?} at {:?}",
                    window.inner_size(),
                    window.outer_position().ok()
                );
                self.window = Some(window);
            }
            Err(err) => {
                tracing::error!("could not open the sill: {err:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested | WindowEvent::Destroyed = event {
            event_loop.exit();
        }
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

    let event_loop = EventLoop::new()?;
    // Nothing is animated yet, so the loop only wakes for window events.
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut App::default())?;
    Ok(())
}
