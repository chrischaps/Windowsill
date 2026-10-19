//! Picking the things up and setting them down somewhere else along the sill.
//!
//! Each thing stands somewhere along the taskbar's edge. A hand can lift one,
//! carry it along and put it down again. While it is held it rises a few pixels
//! off the edge and follows the hand on a stiff spring, so it has a little
//! weight. It tips with the way it is carried, and what is loose inside it
//! trails behind: the flame and the leaves lean back against the air, and the
//! jar's water piles up against the side it is being pulled from. Set down, it
//! rocks once or twice and is still. Whatever it was put down on slides aside
//! to make room, slowly.
//!
//! The cat is one of the things too, with two differences. A quick tap that
//! carries it nowhere wakes it rather than lifting it. And when it walks
//! somewhere by itself, it says where it is each frame and the sill follows.
//!
//! Pure logic in logical pixels across the sill, so it can be tested without a
//! desktop. Where things rest is kept as a share of the sill's width, from 0 at
//! the left end to 1 at the right, as the config gives it.

use std::f32::consts::TAU;
use std::path::PathBuf;

use serde::Deserialize;

use crate::config;

/// The plant, the jar, the candle and the cat, in the order their places are
/// given.
pub const PLANT: usize = 0;
pub const JAR: usize = 1;
pub const CANDLE: usize = 2;
pub const CAT: usize = 3;

/// How many things there are.
pub const COUNT: usize = 4;

/// Half of each thing's foot, in logical pixels: the pot's rim, the jar, the
/// candle, and the cat curled up. Things never stand closer than their feet
/// allow.
pub const FOOT: [f32; COUNT] = [13.0, 11.0, 7.0, 37.0];

/// How tall each thing is where a hand can take hold of it: the pot without
/// its leaves, the jar to its lid, and the candle's wax when it is fresh. The
/// cat says how tall it is itself, since it gets up and lies down.
const BODY: [f32; COUNT] = [24.0, 33.0, 24.0, 0.0];

/// How much room around a thing still counts as taking hold of it.
const REACH: f32 = 3.0;

/// The space left between two feet when one is made room for.
pub const GAP: f32 = 3.0;

/// A press let go of sooner than this, having moved no further than TAP_SLOP
/// logical pixels, is a tap rather than a carry.
const TAP_TIME: f32 = 0.3;
const TAP_SLOP: f32 = 4.0;

/// How far from the ends of the sill a foot may go.
const END: f32 = 2.0;

/// How high a held thing rises off the edge, in logical pixels.
const LIFT: f32 = 5.0;

/// How quickly a thing making room slides over, in radians a second, on a
/// critically damped spring.
const SLIDE: f32 = 7.0;

/// How much a held thing's speed is smoothed, in seconds, so the hand's
/// frame-by-frame jitter doesn't shake it.
const SMOOTH: f32 = 0.05;

/// How long the lift takes to rise and to settle, in seconds.
const RISE: f32 = 0.08;
const SETTLE: f32 = 0.11;

/// How a thing tips as it is carried: about once every PERIOD seconds left to
/// itself, losing most of the swing each time, by GAIN degrees for each logical
/// pixel per second per second it is pushed, and never more than MOST.
const TIP_PERIOD: f32 = 0.55;
const TIP_DAMPING: f32 = 0.3;
const TIP_GAIN: f32 = 0.0016;
const TIP_MOST: f32 = 8.0;

/// The same for what is loose inside it. The flame and the leaves answer how
/// fast it goes, by SWING_SPEED degrees for each logical pixel a second; the
/// water answers how hard it is pushed, by SWING_PUSH.
const SWING_PERIOD: f32 = 0.85;
const SWING_DAMPING: f32 = 0.22;
const SWING_SPEED: f32 = 0.03;
const SWING_PUSH: f32 = 0.0022;
const SWING_MOST: f32 = 14.0;

/// The longest step the springs are moved in one go.
const STEP: f32 = 1.0 / 240.0;

/// A frame longer than this means the sill was aside, and everything is simply
/// where it was left; no one was watching.
const AWAY: f32 = 1.0;

/// Below these, a thing counts as still: a twentieth of a logical pixel, and
/// a tenth of a degree.
const STILL: f32 = 0.05;
const LEVEL: f32 = 0.1;

/// What one thing is doing.
#[derive(Clone, Copy, Debug, Default)]
struct Thing {
    /// Where it rests, as a share of the sill's width.
    at: f32,
    /// Where it is, in logical pixels across, and how fast it is going.
    x: f32,
    speed: f32,
    /// How high it is off the edge.
    lift: f32,
    /// How far it tips, in degrees to the right, and how fast.
    tip: f32,
    tip_speed: f32,
    /// How far what is inside it leans, in degrees to the right, and how fast.
    swing: f32,
    swing_speed: f32,
}

/// The thing in the hand.
#[derive(Clone, Copy, Debug)]
struct Held {
    which: usize,
    /// Where the hand is, in logical pixels across.
    hand: f32,
    /// How far right of the thing's middle the hand took hold of it.
    grip: f32,
    /// Where the hand took hold, how long it has held on, in seconds, and the
    /// furthest it has moved from there, in logical pixels.
    start: f32,
    time: f32,
    travel: f32,
}

impl Held {
    /// Whether letting go now would be a tap rather than setting it down.
    fn tap(&self) -> bool {
        self.time < TAP_TIME && self.travel <= TAP_SLOP
    }
}

/// What letting go did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dropped {
    /// A quick tap that carried nothing anywhere. The thing is where it was.
    Tapped(usize),
    /// Set down: where everything now rests, to be kept.
    Set([f32; COUNT]),
}

/// How everything stands right now, for the shader.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Where each stands, in logical pixels across.
    pub x: [f32; COUNT],
    /// How high each is off the edge, in logical pixels.
    pub lift: [f32; COUNT],
    /// How far each tips, in degrees to the right.
    pub tip: [f32; COUNT],
    /// How far what is inside each leans, in degrees to the right.
    pub swing: [f32; COUNT],
    /// Which is drawn in front: the last one picked up.
    pub front: usize,
}

impl Pose {
    /// Everything standing still at `at`, shares of a sill `width` logical px wide.
    pub fn resting(at: [f32; COUNT], width: f32) -> Self {
        Self {
            x: at.map(|at| at * width),
            lift: [0.0; COUNT],
            tip: [0.0; COUNT],
            swing: [0.0; COUNT],
            front: CANDLE,
        }
    }
}

pub struct Carry {
    things: [Thing; COUNT],
    held: Option<Held>,
    front: usize,
    /// The width the things were last placed against, in logical pixels.
    width: f32,
}

impl Carry {
    /// Things resting at `at`, shares of the sill's width, not yet placed
    /// against one. They appear where they rest on the first step.
    pub fn new(at: [f32; COUNT]) -> Self {
        Self {
            things: at.map(|at| Thing {
                at,
                ..Thing::default()
            }),
            held: None,
            front: CANDLE,
            width: 0.0,
        }
    }

    /// Where each thing rests, as shares of the sill's width.
    pub fn rests(&self) -> [f32; COUNT] {
        self.things.map(|thing| thing.at)
    }

    /// Gives a thing a new place to rest, as from the config; it slides there.
    pub fn rest(&mut self, which: usize, at: f32) {
        self.things[which].at = at.clamp(0.0, 1.0);
    }

    /// Which thing a hand at `x` across and `y` up from the edge, in logical
    /// pixels, would take hold of, the one in front first. `candle` is how much
    /// of the candle is left, from 0 to 1, and `cat` how tall the cat is
    /// right now, in logical pixels.
    pub fn under(&self, x: f32, y: f32, candle: f32, cat: f32) -> Option<usize> {
        let mut order = [PLANT, JAR, CANDLE, CAT];
        order.sort_by_key(|&i| i == self.front);
        order.into_iter().rev().find(|&i| {
            let thing = &self.things[i];
            let tall = match i {
                CANDLE => BODY[i] * candle + 4.0,
                CAT => cat,
                _ => BODY[i],
            };
            (x - thing.x).abs() <= FOOT[i] + REACH
                && y >= thing.lift - REACH
                && y <= thing.lift + tall + REACH
        })
    }

    /// Takes hold of `which` with a hand at `x` across.
    pub fn grab(&mut self, which: usize, x: f32) {
        self.held = Some(Held {
            which,
            hand: x,
            grip: x - self.things[which].x,
            start: x,
            time: 0.0,
            travel: 0.0,
        });
        self.front = which;
    }

    /// The hand has moved to `x` across.
    pub fn reach(&mut self, x: f32) {
        if let Some(held) = &mut self.held {
            held.hand = x;
            held.travel = held.travel.max((x - held.start).abs());
        }
    }

    /// What is in the hand.
    pub fn held(&self) -> Option<usize> {
        self.held.map(|held| held.which)
    }

    /// What is being carried, rather than only pressed on for a moment.
    pub fn carried(&self) -> Option<usize> {
        self.held.filter(|held| !held.tap()).map(|held| held.which)
    }

    /// Lets go. A tap leaves everything where it was. Otherwise the thing
    /// rests where the hand left it, anything it was put down on is given a
    /// place clear of it, and the new places are returned to be kept. `None`
    /// if nothing was held.
    pub fn drop(&mut self) -> Option<Dropped> {
        let held = self.held.take()?;
        if held.tap() {
            return Some(Dropped::Tapped(held.which));
        }
        // Put down, it stops where it was put, whatever the hand was doing.
        self.things[held.which].speed = 0.0;
        let width = self.width.max(1.0);
        let mut xs = self.things.map(|thing| thing.at * width);
        xs[held.which] = self.hand_target(&held);
        make_room(&mut xs, held.which, width);
        for (thing, x) in self.things.iter_mut().zip(xs) {
            thing.at = x / width;
        }
        Some(Dropped::Set(self.rests()))
    }

    /// `which` walks itself to `x` logical pixels across, and rests wherever
    /// it has got to. Called before [`Carry::step`] on each frame it walks, so
    /// it is drawn exactly there and never counts as sliding.
    pub fn walk(&mut self, which: usize, x: f32) {
        if self.held.is_some_and(|held| held.which == which) || self.width <= 0.0 {
            return;
        }
        let x = clamp_foot(x, which, self.width);
        let thing = &mut self.things[which];
        thing.at = x / self.width;
        thing.x = x;
        thing.speed = 0.0;
    }

    /// `which` has walked as far as it is going. Anything that came to stand
    /// where it lay down meanwhile makes room, and where everything now rests
    /// is returned to be kept.
    pub fn arrive(&mut self, which: usize) -> [f32; COUNT] {
        let width = self.width.max(1.0);
        let mut xs = self.things.map(|thing| thing.at * width);
        make_room(&mut xs, which, width);
        for (thing, x) in self.things.iter_mut().zip(xs) {
            thing.at = x / width;
        }
        self.rests()
    }

    /// Where everything rests, in logical pixels across, and how wide the sill
    /// is. Nothing is known until the first step.
    pub fn spots(&self) -> Option<([f32; COUNT], f32)> {
        (self.width > 0.0).then(|| (self.things.map(|thing| thing.at * self.width), self.width))
    }

    /// Lets go without moving anything, as when the sill steps aside mid-carry:
    /// the thing goes back to where it rested.
    pub fn let_go(&mut self) {
        self.held = None;
    }

    /// Where the hand is taking the held thing, kept clear of the ends.
    fn hand_target(&self, held: &Held) -> f32 {
        clamp_foot(held.hand - held.grip, held.which, self.width)
    }

    /// On first being placed, a cat lying on top of something (from an
    /// arrangement kept when it was smaller, or a narrow screen) moves to the
    /// nearest place clear of everything; the others stay where a hand put
    /// them. It lies there from the start rather than walking.
    fn make_way_for_the_cat(&mut self) {
        let width = self.width;
        let xs = self.things.map(|thing| thing.x);
        let clear =
            |x: f32| (0..COUNT).all(|i| i == CAT || (xs[i] - x).abs() >= FOOT[i] + FOOT[CAT] + GAP);
        if clear(xs[CAT]) {
            return;
        }
        let beside = (0..COUNT)
            .filter(|&i| i != CAT)
            .flat_map(|i| [-1.0, 1.0].map(|side| xs[i] + side * (FOOT[i] + FOOT[CAT] + GAP)));
        let best = beside
            .map(|x| clamp_foot(x, CAT, width))
            .filter(|&x| clear(x))
            .min_by(|a, b| (a - xs[CAT]).abs().total_cmp(&(b - xs[CAT]).abs()));
        if let Some(x) = best {
            let cat = &mut self.things[CAT];
            cat.x = x;
            cat.at = x / width;
        }
    }

    /// Whether `which` is off the edge in the hand: from the start for most
    /// things, but the cat only once it is clearly being carried, so a tap
    /// doesn't bounce it.
    fn lifting(&self, which: usize) -> bool {
        self.held
            .is_some_and(|held| held.which == which && (which != CAT || !held.tap()))
    }

    /// Moves everything on by `dt` real seconds on a sill `width` logical px wide.
    pub fn step(&mut self, dt: f32, width: f32) {
        let placed = self.width > 0.0;
        self.width = width;
        if let Some(held) = &mut self.held {
            held.time += dt.max(0.0);
        }
        if placed && dt <= 0.0 {
            return;
        }
        if !placed || dt > AWAY {
            let held = self.held.map(|held| (held.which, self.hand_target(&held)));
            for (i, thing) in self.things.iter_mut().enumerate() {
                let x = match held {
                    Some((which, x)) if which == i => x,
                    _ => clamp_foot(thing.at * width, i, width),
                };
                *thing = Thing {
                    at: thing.at,
                    x,
                    lift: if held.is_some_and(|(which, _)| which == i) {
                        LIFT
                    } else {
                        0.0
                    },
                    ..Thing::default()
                };
            }
            if !placed {
                self.make_way_for_the_cat();
            }
            return;
        }
        let steps = (dt / STEP).ceil().max(1.0);
        let h = dt / steps;
        let held = self.held.map(|held| (held.which, self.hand_target(&held)));
        let lifting = [PLANT, JAR, CANDLE, CAT].map(|i| self.lifting(i));
        for (i, thing) in self.things.iter_mut().enumerate() {
            let (target, in_hand) = match held {
                Some((which, x)) if which == i => (x, true),
                _ => (clamp_foot(thing.at * width, i, width), false),
            };
            let (lift_to, tau) = if lifting[i] {
                (LIFT, RISE)
            } else {
                (0.0, SETTLE)
            };
            // In the hand it goes exactly where the hand does, spread evenly
            // over the frame, and its speed is how fast the hand moved, a
            // little smoothed.
            let (from, hand_speed) = (thing.x, (target - thing.x) / dt);
            let smooth = 1.0 - (-h / SMOOTH).exp();
            for k in 1..=steps as u32 {
                let push = if in_hand {
                    let was = thing.speed;
                    thing.x = from + (target - from) * k as f32 / steps;
                    thing.speed += (hand_speed - thing.speed) * smooth;
                    (thing.speed - was) / h
                } else {
                    // Semi-implicit Euler, as the leaves' sway does: the new
                    // speed moves the thing, which keeps the spring from gaining
                    // energy.
                    let push = SLIDE * SLIDE * (target - thing.x) - 2.0 * SLIDE * thing.speed;
                    thing.speed += push * h;
                    thing.x += thing.speed * h;
                    push
                };
                thing.lift = lift_to - (lift_to - thing.lift) * (-h / tau).exp();

                // Tipped by the push only while off the edge; standing, it rocks
                // back upright.
                let off = thing.lift / LIFT;
                let tip_to = (push * TIP_GAIN * off).clamp(-TIP_MOST, TIP_MOST);
                spring(
                    &mut thing.tip,
                    &mut thing.tip_speed,
                    tip_to,
                    TIP_PERIOD,
                    TIP_DAMPING,
                    h,
                );
                thing.tip = thing.tip.clamp(-TIP_MOST, TIP_MOST);

                let swing_to = if i == JAR {
                    -push * SWING_PUSH
                } else {
                    -thing.speed * SWING_SPEED
                };
                spring(
                    &mut thing.swing,
                    &mut thing.swing_speed,
                    swing_to.clamp(-SWING_MOST, SWING_MOST),
                    SWING_PERIOD,
                    SWING_DAMPING,
                    h,
                );
                thing.swing = thing.swing.clamp(-SWING_MOST, SWING_MOST);
            }
        }
    }

    /// Whether anything is held or still moving, so the sill should draw
    /// smoothly rather than at its idle rate.
    pub fn moving(&self) -> bool {
        self.held.is_some()
            || self.things.iter().enumerate().any(|(i, thing)| {
                (thing.x - clamp_foot(thing.at * self.width, i, self.width)).abs() > STILL
                    || thing.speed.abs() > STILL
                    || thing.lift > STILL
                    || thing.tip.abs() > LEVEL
                    || thing.tip_speed.abs() > LEVEL
                    || thing.swing.abs() > LEVEL
                    || thing.swing_speed.abs() > LEVEL
            })
    }

    /// How everything stands.
    pub fn pose(&self) -> Pose {
        Pose {
            x: self.things.map(|thing| thing.x),
            lift: self.things.map(|thing| thing.lift),
            tip: self.things.map(|thing| thing.tip),
            swing: self.things.map(|thing| thing.swing),
            front: self.front,
        }
    }
}

/// Moves a damped spring of the given period on by `h` seconds toward `to`.
fn spring(value: &mut f32, speed: &mut f32, to: f32, period: f32, damping: f32, h: f32) {
    let w = TAU / period;
    *speed += (w * w * (to - *value) - 2.0 * damping * w * *speed) * h;
    *value += *speed * h;
}

/// `x` kept far enough from the ends for `which`'s foot.
fn clamp_foot(x: f32, which: usize, width: f32) -> f32 {
    let margin = FOOT[which] + END;
    x.clamp(margin, (width - margin).max(margin))
}

/// Moves the things in `xs` apart until no two feet overlap, keeping `put`
/// where it was put down if there is any room to, and all of them on the sill.
fn make_room(xs: &mut [f32; COUNT], put: usize, width: f32) {
    let mut order = [PLANT, JAR, CANDLE, CAT];
    // Whichever side of a thing's middle the new one lands, it goes that side.
    order.sort_by(|&a, &b| xs[a].total_cmp(&xs[b]));
    for fixed in [Some(put), None] {
        for _ in 0..12 {
            let mut moved = false;
            for pair in order.windows(2) {
                let (l, r) = (pair[0], pair[1]);
                let overlap = FOOT[l] + FOOT[r] + GAP - (xs[r] - xs[l]);
                if overlap <= 1e-3 {
                    continue;
                }
                moved = true;
                match fixed {
                    Some(f) if f == l => xs[r] += overlap,
                    Some(f) if f == r => xs[l] -= overlap,
                    _ => {
                        xs[l] -= overlap * 0.5;
                        xs[r] += overlap * 0.5;
                    }
                }
            }
            for (i, x) in xs.iter_mut().enumerate() {
                *x = clamp_foot(*x, i, width);
            }
            if !moved {
                return;
            }
        }
    }
}

/// Where things were last set down, from `%APPDATA%\Windowsill`, if they have
/// ever been moved and the file makes sense. Anything the file doesn't
/// mention, like a cat that came after it was written, rests where `set` says.
pub fn remembered(set: [f32; COUNT]) -> Option<[f32; COUNT]> {
    parse(&std::fs::read_to_string(path()?).ok()?, set)
}

/// Keeps where things now rest, so they are there tomorrow.
pub fn keep(at: [f32; COUNT]) {
    let Some(path) = path() else { return };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, render(at)));
    if let Err(err) = written {
        tracing::warn!(
            "could not keep where things stand in {}: {err}",
            path.display()
        );
    }
}

const FILE_NAME: &str = "arranged.toml";

fn path() -> Option<PathBuf> {
    config::app_dir().map(|dir| dir.join(FILE_NAME))
}

fn parse(text: &str, set: [f32; COUNT]) -> Option<[f32; COUNT]> {
    #[derive(Deserialize)]
    struct Kept {
        plant: f32,
        jar: f32,
        candle: f32,
        cat: Option<f32>,
    }
    let kept: Kept = toml::from_str(text).ok()?;
    let at = [
        kept.plant,
        kept.jar,
        kept.candle,
        kept.cat.unwrap_or(set[CAT]),
    ];
    at.iter().all(|at| (0.0..=1.0).contains(at)).then_some(at)
}

fn render([plant, jar, candle, cat]: [f32; COUNT]) -> String {
    format!(
        "# Where the things on the sill were last set down, from 0 at its left\n\
         # end to 1 at its right. Changing `at` in windowsill.toml moves them\n\
         # again; deleting this file puts them back where it says.\n\
         plant = {plant:.4}\n\
         jar = {jar:.4}\n\
         candle = {candle:.4}\n\
         cat = {cat:.4}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: f32 = 1920.0;

    /// Where the config puts things before a hand moves them.
    const SET: [f32; COUNT] = [0.72, 0.75, 0.775, 0.81];

    /// Where things rest after being set down; a tap fails the test.
    fn set(dropped: Option<Dropped>) -> [f32; COUNT] {
        match dropped {
            Some(Dropped::Set(at)) => at,
            other => panic!("not set down: {other:?}"),
        }
    }

    /// The default arrangement, placed and settled.
    fn carry() -> Carry {
        let mut carry = Carry::new(SET);
        carry.step(1.0 / 60.0, WIDTH);
        carry
    }

    /// Runs `seconds` at 60 fps, the hand moving by `dx` each frame.
    fn run(carry: &mut Carry, seconds: f32, mut hand: f32, dx: f32) -> f32 {
        for _ in 0..(seconds * 60.0) as u32 {
            hand += dx;
            carry.reach(hand);
            carry.step(1.0 / 60.0, WIDTH);
        }
        hand
    }

    #[test]
    fn things_start_where_they_rest() {
        let carry = carry();
        let pose = carry.pose();
        assert_eq!(pose.x, SET.map(|at| at * WIDTH));
        assert_eq!(pose.lift, [0.0; COUNT]);
        assert!(!carry.moving());
    }

    #[test]
    fn a_hand_finds_what_it_is_over() {
        let carry = carry();
        let jar = 0.75 * WIDTH;
        assert_eq!(carry.under(jar, 10.0, 1.0, 14.0), Some(JAR));
        assert_eq!(carry.under(jar + 30.0, 10.0, 1.0, 14.0), None);
        // Above the jar's lid is the desktop, not the jar.
        assert_eq!(carry.under(jar, 45.0, 1.0, 14.0), None);
        // The candle burns down, and so does where it can be held.
        let candle = 0.775 * WIDTH;
        assert_eq!(carry.under(candle, 26.0, 1.0, 14.0), Some(CANDLE));
        assert_eq!(carry.under(candle, 26.0, 0.5, 14.0), None);
    }

    #[test]
    fn what_is_held_follows_the_hand_and_lifts() {
        let mut carry = carry();
        let start = 0.75 * WIDTH;
        carry.grab(JAR, start + 4.0);
        let hand = run(&mut carry, 1.0, start + 4.0, -5.0);
        let pose = carry.pose();
        // It keeps the grip it was taken by, give or take a frame of lag.
        assert!((pose.x[JAR] - (hand - 4.0)).abs() < 2.0, "{pose:?}");
        assert!((pose.lift[JAR] - LIFT).abs() < 0.1);
        assert_eq!(pose.front, JAR);
        assert!(carry.moving());
    }

    #[test]
    fn carried_things_tip_and_their_insides_trail() {
        let mut carry = carry();
        let start = 0.72 * WIDTH;
        carry.grab(PLANT, start);
        let mut most = [0.0f32; 2];
        let mut hand = start;
        for _ in 0..30 {
            hand = run(&mut carry, 1.0 / 60.0, hand, 12.0);
            let pose = carry.pose();
            most[0] = most[0].max(pose.tip[PLANT].abs());
            // Carried right, the leaves lean back to the left.
            assert!(pose.swing[PLANT] <= 0.05, "{pose:?}");
            most[1] = most[1].max(pose.swing[PLANT].abs());
        }
        assert!(most[0] > 0.5 && most[0] <= TIP_MOST, "{most:?}");
        assert!(most[1] > 1.0 && most[1] <= SWING_MOST, "{most:?}");
    }

    #[test]
    fn set_down_it_comes_to_rest() {
        let mut carry = carry();
        let start = 0.2 * WIDTH;
        carry.grab(CANDLE, 0.775 * WIDTH);
        let hand = run(
            &mut carry,
            0.5,
            0.775 * WIDTH,
            (start - 0.775 * WIDTH) / 30.0,
        );
        let rests = set(carry.drop());
        assert!((rests[CANDLE] * WIDTH - hand).abs() < 0.5, "{rests:?}");
        run(&mut carry, 4.0, hand, 0.0);
        assert!(!carry.moving(), "{:?}", carry.pose());
        assert!(carry.pose().lift[CANDLE] < STILL);
    }

    #[test]
    fn put_down_on_something_it_slides_aside() {
        let mut carry = carry();
        let jar = 0.75 * WIDTH;
        // The candle, set down just right of the jar's middle.
        carry.grab(CANDLE, 0.775 * WIDTH);
        carry.reach(jar + 2.0);
        carry.step(1.0, WIDTH);
        let rests = set(carry.drop()).map(|at| at * WIDTH);
        assert!((rests[CANDLE] - (jar + 2.0)).abs() < 1e-3, "{rests:?}");
        // The jar makes room to the left, and so does the plant beyond it.
        assert!(
            rests[JAR] <= rests[CANDLE] - FOOT[JAR] - FOOT[CANDLE] - GAP + 1e-3,
            "{rests:?}"
        );
        assert!(
            rests[PLANT] <= rests[JAR] - FOOT[PLANT] - FOOT[JAR] - GAP + 1e-3,
            "{rests:?}"
        );
        // It slides over rather than jumping.
        let before = carry.pose().x[JAR];
        carry.step(1.0 / 60.0, WIDTH);
        assert!((carry.pose().x[JAR] - before).abs() < 3.0);
    }

    #[test]
    fn nothing_leaves_the_sill() {
        let mut carry = carry();
        carry.grab(PLANT, 0.72 * WIDTH);
        carry.reach(-500.0);
        carry.step(2.0, WIDTH);
        assert!(carry.pose().x[PLANT] >= FOOT[PLANT]);
        carry.drop();
        // Everything piled at the left end still fits, side by side.
        carry.grab(JAR, 0.75 * WIDTH);
        carry.reach(0.0);
        carry.step(2.0, WIDTH);
        let rests = set(carry.drop()).map(|at| at * WIDTH);
        let mut order = [PLANT, JAR, CANDLE, CAT];
        order.sort_by(|&a, &b| rests[a].total_cmp(&rests[b]));
        assert!(rests[order[0]] >= FOOT[order[0]]);
        for pair in order.windows(2) {
            let (l, r) = (pair[0], pair[1]);
            assert!(
                rests[r] - rests[l] >= FOOT[l] + FOOT[r] + GAP - 1e-2,
                "{rests:?}"
            );
        }
    }

    #[test]
    fn after_being_aside_things_are_simply_where_they_were_left() {
        let mut carry = carry();
        carry.rest(JAR, 0.1);
        carry.step(30.0, WIDTH);
        assert_eq!(carry.pose().x[JAR], 0.1 * WIDTH);
        assert!(!carry.moving());
    }

    #[test]
    fn the_same_at_30_and_60_fps() {
        let mut slow = carry();
        let mut fast = carry();
        slow.rest(CANDLE, 0.5);
        fast.rest(CANDLE, 0.5);
        for _ in 0..30 {
            slow.step(1.0 / 30.0, WIDTH);
            fast.step(1.0 / 60.0, WIDTH);
            fast.step(1.0 / 60.0, WIDTH);
        }
        let (a, b) = (slow.pose(), fast.pose());
        assert!((a.x[CANDLE] - b.x[CANDLE]).abs() < 0.5, "{a:?} vs {b:?}");
        assert!((a.swing[CANDLE] - b.swing[CANDLE]).abs() < 0.1);
    }

    #[test]
    fn the_kept_file_reads_back() {
        let at = [0.1, 0.5, 0.9, 0.95];
        assert_eq!(parse(&render(at), SET), Some(at));
        assert_eq!(parse("plant = 2.0\njar = 0.5\ncandle = 0.5\n", SET), None);
        assert_eq!(parse("plant = 0.5\n", SET), None);
    }

    #[test]
    fn a_file_from_before_the_cat_still_reads() {
        assert_eq!(
            parse("plant = 0.1\njar = 0.2\ncandle = 0.3\n", SET),
            Some([0.1, 0.2, 0.3, SET[CAT]])
        );
    }

    #[test]
    fn a_quick_press_is_a_tap() {
        let mut carry = carry();
        let cat = SET[CAT] * WIDTH;
        carry.grab(CAT, cat);
        run(&mut carry, 0.1, cat, 0.5);
        // The cat isn't lifted for a tap, and nothing moves.
        assert!(carry.pose().lift[CAT] < STILL);
        assert_eq!(carry.carried(), None);
        assert_eq!(carry.drop(), Some(Dropped::Tapped(CAT)));
        assert_eq!(carry.rests(), SET);
    }

    #[test]
    fn a_press_held_or_moved_is_a_carry() {
        let mut carry = carry();
        let cat = SET[CAT] * WIDTH;
        carry.grab(CAT, cat);
        run(&mut carry, 0.5, cat, 0.0);
        assert_eq!(carry.carried(), Some(CAT));
        assert!(carry.pose().lift[CAT] > LIFT * 0.9);
        set(carry.drop());

        let mut carry = self::carry();
        carry.grab(CAT, cat);
        run(&mut carry, 0.1, cat, -2.0);
        set(carry.drop());
    }

    #[test]
    fn a_walking_cat_is_drawn_where_it_is_and_kept_when_it_lies_down() {
        let mut carry = carry();
        let start = SET[CAT] * WIDTH;
        for k in 1..=60 {
            carry.walk(CAT, start + k as f32 * 0.2);
            carry.step(1.0 / 30.0, WIDTH);
            assert!((carry.pose().x[CAT] - (start + k as f32 * 0.2)).abs() < 1e-2);
            assert!(!carry.moving());
        }
        let rests = carry.arrive(CAT);
        assert!(
            (rests[CAT] * WIDTH - (start + 12.0)).abs() < 1e-2,
            "{rests:?}"
        );
    }

    #[test]
    fn a_cat_found_on_top_of_something_starts_beside_it() {
        let mut at = SET;
        at[CAT] = SET[JAR] + 2.0 / WIDTH;
        let mut carry = Carry::new(at);
        carry.step(1.0 / 60.0, WIDTH);
        let x = carry.pose().x;
        for i in [PLANT, JAR, CANDLE] {
            assert!(
                (x[i] - x[CAT]).abs() >= FOOT[i] + FOOT[CAT] + GAP - 1e-3,
                "{x:?}"
            );
            assert_eq!(x[i], SET[i] * WIDTH, "the others stay put");
        }
        assert!(!carry.moving());
    }

    #[test]
    fn a_cat_lying_down_on_something_moves_it_aside() {
        let mut carry = carry();
        let candle = SET[CANDLE] * WIDTH;
        carry.walk(CAT, candle + 3.0);
        let rests = carry.arrive(CAT).map(|at| at * WIDTH);
        assert!(
            rests[CANDLE] <= rests[CAT] - FOOT[CANDLE] - FOOT[CAT] - GAP + 1e-3,
            "{rests:?}"
        );
    }
}
