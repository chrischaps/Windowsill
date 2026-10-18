//! Where the sill rests: on top of a bottom taskbar, spanning the primary monitor.
//!
//! Pure geometry, so it can be tested without a desktop. Everything is in
//! physical pixels in virtual-desktop coordinates, which is what both winit and
//! a per-monitor DPI-aware `SHAppBarMessage` speak.

use std::fmt;

/// The strip's height in logical pixels: room for the plant's leaves, the
/// candle's warmth and a thing lifted off the edge, and no more.
pub const SILL_HEIGHT: f64 = 90.0;

/// A rectangle in physical pixels, edges exclusive on the right and bottom like a Win32 `RECT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        (self.left..self.right).contains(&x) && (self.top..self.bottom).contains(&y)
    }
}

/// The screen edge the taskbar is docked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Top,
    Right,
    Bottom,
}

/// The taskbar as the shell reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Taskbar {
    pub rect: Rect,
    pub edge: Edge,
    /// Whether it slides out of sight until the pointer comes near it.
    pub auto_hide: bool,
}

/// Why the sill is not drawn. Each is a layout v1 doesn't support, so the sill
/// steps aside rather than sit somewhere wrong (design rule 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unplaced {
    /// winit can't tell which monitor is the primary one.
    NoPrimaryMonitor,
    /// The shell reported no taskbar (Explorer isn't running, or the call failed).
    NoTaskbar,
    /// The taskbar is docked to the side or the top.
    Edge(Edge),
    /// The taskbar lives on a monitor other than the primary one.
    NotOnPrimary,
    /// The taskbar auto-hides, so most of the time there is nothing to rest on.
    AutoHide,
}

impl fmt::Display for Unplaced {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unplaced::NoPrimaryMonitor => write!(f, "there is no primary monitor"),
            Unplaced::NoTaskbar => write!(f, "there is no taskbar to rest on"),
            Unplaced::Edge(edge) => {
                write!(
                    f,
                    "the taskbar is on the {edge:?} edge; only a bottom taskbar is supported"
                )
            }
            Unplaced::NotOnPrimary => write!(f, "the taskbar is not on the primary monitor"),
            Unplaced::AutoHide => write!(f, "the taskbar auto-hides"),
        }
    }
}

/// The sill's rectangle: as wide as the primary monitor, [`SILL_HEIGHT`]
/// logical pixels tall, with its bottom edge touching the top of the taskbar.
pub fn place(monitor: Rect, scale: f64, taskbar: Option<Taskbar>) -> Result<Rect, Unplaced> {
    let taskbar = taskbar.ok_or(Unplaced::NoTaskbar)?;
    if taskbar.auto_hide {
        return Err(Unplaced::AutoHide);
    }
    if taskbar.edge != Edge::Bottom {
        return Err(Unplaced::Edge(taskbar.edge));
    }
    let rect = taskbar.rect;
    let centre = (rect.left + rect.width() / 2, rect.top + rect.height() / 2);
    if !monitor.contains_point(centre.0, centre.1) {
        return Err(Unplaced::NotOnPrimary);
    }

    let height = (SILL_HEIGHT * scale).round() as i32;
    Ok(Rect {
        left: monitor.left,
        top: rect.top - height,
        right: monitor.right,
        bottom: rect.top,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    /// A 1920×1080-logical monitor at `scale`, with a 48-logical-pixel bottom taskbar.
    fn desktop(scale: f64) -> (Rect, Taskbar) {
        let width = (1920.0 * scale) as i32;
        let height = (1080.0 * scale) as i32;
        let bar = (48.0 * scale).round() as i32;
        let monitor = rect(0, 0, width, height);
        let taskbar = Taskbar {
            rect: rect(0, height - bar, width, height),
            edge: Edge::Bottom,
            auto_hide: false,
        };
        (monitor, taskbar)
    }

    #[test]
    fn rests_on_the_taskbar_at_every_common_scale() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (monitor, taskbar) = desktop(scale);
            let sill = place(monitor, scale, Some(taskbar)).unwrap();
            assert_eq!(
                sill.bottom, taskbar.rect.top,
                "touches the taskbar at {scale}"
            );
            assert_eq!(
                sill.height(),
                (SILL_HEIGHT * scale).round() as i32,
                "height at {scale}"
            );
            assert_eq!(
                (sill.left, sill.right),
                (monitor.left, monitor.right),
                "spans at {scale}"
            );
        }
    }

    #[test]
    fn follows_a_primary_monitor_that_is_not_at_the_origin() {
        let monitor = rect(-2560, 200, 0, 1640);
        let taskbar = Taskbar {
            rect: rect(-2560, 1592, 0, 1640),
            edge: Edge::Bottom,
            auto_hide: false,
        };
        assert_eq!(
            place(monitor, 1.0, Some(taskbar)),
            Ok(rect(-2560, 1502, 0, 1592))
        );
    }

    #[test]
    fn steps_aside_for_side_and_top_taskbars() {
        let (monitor, _) = desktop(1.0);
        let cases = [
            (Edge::Left, rect(0, 0, 62, 1080)),
            (Edge::Right, rect(1858, 0, 1920, 1080)),
            (Edge::Top, rect(0, 0, 1920, 48)),
        ];
        for (edge, bar) in cases {
            let taskbar = Taskbar {
                rect: bar,
                edge,
                auto_hide: false,
            };
            assert_eq!(
                place(monitor, 1.0, Some(taskbar)),
                Err(Unplaced::Edge(edge))
            );
        }
    }

    #[test]
    fn steps_aside_when_the_taskbar_is_on_another_monitor() {
        let (monitor, _) = desktop(1.0);
        let taskbar = Taskbar {
            rect: rect(1920, 1032, 3840, 1080),
            edge: Edge::Bottom,
            auto_hide: false,
        };
        assert_eq!(
            place(monitor, 1.0, Some(taskbar)),
            Err(Unplaced::NotOnPrimary)
        );
    }

    #[test]
    fn steps_aside_without_a_taskbar() {
        let (monitor, _) = desktop(1.0);
        assert_eq!(place(monitor, 1.0, None), Err(Unplaced::NoTaskbar));
    }

    #[test]
    fn steps_aside_when_the_taskbar_auto_hides() {
        let (monitor, taskbar) = desktop(1.0);
        let taskbar = Taskbar {
            auto_hide: true,
            ..taskbar
        };
        assert_eq!(place(monitor, 1.0, Some(taskbar)), Err(Unplaced::AutoHide));
    }
}
