//! The one interaction: rest the pointer on the candle and hold it still, and
//! a tiny menu opens.
//!
//! The sill lets every click through, so it never hears the mouse. Instead it
//! looks at where the pointer is each time it draws, and only a pointer that
//! stays put over the candle for a whole second counts. One sweeping across
//! the taskbar, dragging something, or pausing for a moment on the way past
//! does not. Once the menu has been, the pointer must leave the candle before
//! it can open again.
//!
//! Pure logic, so it can be tested without a desktop. Everything is in
//! physical pixels in virtual-desktop coordinates.

use std::time::{Duration, Instant};

use crate::placement::Rect;

/// How long the pointer must rest on the candle.
const HOLD: Duration = Duration::from_secs(1);

/// How far around the candle's foot the pointer counts as on it, in logical
/// pixels: a little wider than the wax, and from the board up past the flame.
const HALF_WIDTH: f64 = 14.0;
const FROM: f64 = 18.0;
const TO: f64 = 70.0;

/// How far a resting hand drifts, in logical pixels, without counting as a move.
const SLACK: f64 = 4.0;

/// Where on the screen the candle can be held, given where the sill rests,
/// how far across it the candle stands, from 0 to 1, and the sill's scale.
pub fn candle(sill: Rect, at: f32, scale: f64) -> Rect {
    // Where the shader stands it: on a whole physical pixel.
    let x = sill.left + (f64::from(at) * f64::from(sill.width())).floor() as i32;
    let half = (HALF_WIDTH * scale).round() as i32;
    Rect {
        left: x - half,
        top: sill.bottom - (TO * scale).round() as i32,
        right: x + half,
        bottom: sill.bottom - (FROM * scale).round() as i32,
    }
}

/// How far the pointer may drift at this scale and still be resting.
pub fn slack(scale: f64) -> i32 {
    (SLACK * scale).round().max(1.0) as i32
}

/// What was chosen from the menu, once it has closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Quit,
    /// It was closed without choosing anything that needs the sill.
    Nothing,
}

/// The mouse as last seen.
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    pub x: i32,
    pub y: i32,
    /// Whether any button is down, as when dragging something past.
    pub pressed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum State {
    /// Not on the candle.
    #[default]
    Away,
    /// On the candle, resting near `x`, `y` since `since`.
    Resting { x: i32, y: i32, since: Instant },
    /// The menu is open.
    Open,
    /// The menu has been and gone, and the pointer hasn't left the candle yet.
    Spent,
}

#[derive(Default)]
pub struct Hold {
    state: State,
}

impl Hold {
    /// Takes in where the pointer is at `now`, and says whether the menu should
    /// open. `candle` is `None` while the sill can't be held: aside, fading or
    /// covered.
    pub fn sense(
        &mut self,
        now: Instant,
        pointer: Option<Pointer>,
        candle: Option<Rect>,
        slack: i32,
    ) -> bool {
        if self.state == State::Open {
            return false;
        }
        let on = match (pointer, candle) {
            (Some(pointer), Some(candle)) if candle.contains_point(pointer.x, pointer.y) => pointer,
            _ => {
                self.state = State::Away;
                return false;
            }
        };
        self.state = match self.state {
            State::Spent => State::Spent,
            _ if on.pressed => State::Away,
            State::Resting { x, y, since }
                if (on.x - x).abs() <= slack && (on.y - y).abs() <= slack =>
            {
                if now.duration_since(since) >= HOLD {
                    self.state = State::Open;
                    return true;
                }
                State::Resting { x, y, since }
            }
            _ => State::Resting {
                x: on.x,
                y: on.y,
                since: now,
            },
        };
        false
    }

    /// The menu has closed. It won't open again until the pointer leaves the candle.
    pub fn closed(&mut self) {
        self.state = State::Spent;
    }

    /// Whether the menu is open.
    pub fn open(&self) -> bool {
        self.state == State::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONE: Rect = Rect {
        left: 100,
        top: 50,
        right: 128,
        bottom: 102,
    };

    fn at(x: i32, y: i32) -> Option<Pointer> {
        Some(Pointer {
            x,
            y,
            pressed: false,
        })
    }

    fn after(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn the_candle_stands_where_the_shader_draws_it() {
        let sill = Rect {
            left: 0,
            top: 840,
            right: 1920,
            bottom: 960,
        };
        let zone = candle(sill, 0.5, 1.0);
        assert_eq!((zone.left, zone.right), (946, 974));
        assert_eq!((zone.top, zone.bottom), (890, 942));

        let doubled = candle(sill, 0.5, 2.0);
        assert_eq!((doubled.left, doubled.right), (932, 988));
        assert_eq!(doubled.top, 960 - 140);
    }

    #[test]
    fn a_second_held_still_opens_it_once() {
        let start = Instant::now();
        let mut hold = Hold::default();
        assert!(!hold.sense(start, at(110, 80), Some(ZONE), 4));
        assert!(!hold.sense(after(start, 500), at(111, 81), Some(ZONE), 4));
        assert!(hold.sense(after(start, 1000), at(112, 79), Some(ZONE), 4));
        assert!(hold.open());
        assert!(!hold.sense(after(start, 2000), at(112, 79), Some(ZONE), 4));
    }

    #[test]
    fn passing_by_never_opens_it() {
        let start = Instant::now();
        let mut hold = Hold::default();
        // Slowly across the candle, a few pixels each frame for two seconds.
        for frame in 0..30 {
            let now = after(start, frame * 66);
            assert!(!hold.sense(now, at(100 + frame as i32, 80), Some(ZONE), 4));
        }
    }

    #[test]
    fn a_pause_off_the_candle_starts_again() {
        let start = Instant::now();
        let mut hold = Hold::default();
        hold.sense(start, at(110, 80), Some(ZONE), 4);
        hold.sense(after(start, 600), at(300, 80), Some(ZONE), 4);
        assert!(!hold.sense(after(start, 700), at(110, 80), Some(ZONE), 4));
        assert!(!hold.sense(after(start, 1500), at(110, 80), Some(ZONE), 4));
        assert!(hold.sense(after(start, 1700), at(110, 80), Some(ZONE), 4));
    }

    #[test]
    fn dragging_something_past_never_opens_it() {
        let start = Instant::now();
        let mut hold = Hold::default();
        let dragging = Some(Pointer {
            x: 110,
            y: 80,
            pressed: true,
        });
        for frame in 0..30 {
            assert!(!hold.sense(after(start, frame * 66), dragging, Some(ZONE), 4));
        }
    }

    #[test]
    fn not_while_the_sill_cant_be_held() {
        let start = Instant::now();
        let mut hold = Hold::default();
        hold.sense(start, at(110, 80), Some(ZONE), 4);
        assert!(!hold.sense(after(start, 1200), at(110, 80), None, 4));
        assert!(!hold.sense(after(start, 1300), None, Some(ZONE), 4));
    }

    #[test]
    fn after_closing_the_pointer_must_leave_first() {
        let start = Instant::now();
        let mut hold = Hold::default();
        hold.sense(start, at(110, 80), Some(ZONE), 4);
        assert!(hold.sense(after(start, 1000), at(110, 80), Some(ZONE), 4));
        hold.closed();
        assert!(!hold.sense(after(start, 3000), at(110, 80), Some(ZONE), 4));
        assert!(!hold.sense(after(start, 3100), at(400, 80), Some(ZONE), 4));
        hold.sense(after(start, 3200), at(110, 80), Some(ZONE), 4);
        assert!(hold.sense(after(start, 4200), at(110, 80), Some(ZONE), 4));
    }
}
