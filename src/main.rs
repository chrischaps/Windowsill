//! Windowsill: a thin, living strip that rests on top of the Windows taskbar.
//!
//! For now the sill is an empty pane of glass with nothing drawn on it yet.
//! Clicks fall through it, it never takes focus, and it stays out of the
//! taskbar and Alt+Tab. Drawing arrives with the rest of M1.

#[cfg(windows)]
mod win32;

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

/// The strip's height in logical pixels.
const SILL_HEIGHT: f64 = 120.0;

/// Rough height of a Windows 11 taskbar, in logical pixels. A stand-in until the
/// strip is placed from the real taskbar position (#2).
const TASKBAR_GUESS: f64 = 48.0;

#[derive(Default)]
struct App {
    window: Option<Window>,
}

impl App {
    fn open_sill(&self, event_loop: &ActiveEventLoop) -> anyhow::Result<Window> {
        let mut attributes = WindowAttributes::default()
            .with_title("Windowsill")
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_active(false)
            .with_visible(false);

        if let Some(monitor) = event_loop.primary_monitor() {
            let scale = monitor.scale_factor();
            let origin = monitor.position();
            let size = monitor.size();
            let height = (SILL_HEIGHT * scale).round() as i32;
            let above_taskbar = (TASKBAR_GUESS * scale).round() as i32;
            attributes = attributes
                .with_inner_size(PhysicalSize::new(size.width, height as u32))
                .with_position(PhysicalPosition::new(
                    origin.x,
                    origin.y + size.height as i32 - above_taskbar - height,
                ));
        } else {
            attributes = attributes.with_inner_size(LogicalSize::new(800.0, SILL_HEIGHT));
        }

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

        // Shown last, once it is already glass. With `with_active(false)` winit
        // shows it with SW_SHOWNOACTIVATE, but that show rewrites the ex-style, so
        // the glass styles go back on straight after.
        window.set_visible(true);
        #[cfg(windows)]
        {
            win32::make_glass(&window)?;
            tracing::debug!("sill ex-style {:#010x}", win32::ex_style(&window).unwrap_or(0));
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
