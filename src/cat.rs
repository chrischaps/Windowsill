//! The cat: asleep on the sill nearly all the time, the way a cat on a real
//! windowsill is.
//!
//! It lies curled, breathing slowly. A few times a day it gets up by itself
//! and pads over to lie on whichever side of its neighbours the real sun is
//! coming from, out of their shade, and after dark it goes to lie by the
//! candle while the candle burns. When the sun's side isn't known, or it is
//! straight behind, it stays where it is. It never invents a reason to move.
//!
//! A hand can carry it like the others, and it hangs a little loose while it
//! is held. A tap wakes it: it gets up, stretches, walks a little way and lies
//! down again. A pointer resting on it is petting it, and it half opens its
//! eyes, slow-blinks and breathes deeper, which is the nearest a picture gets
//! to a purr. While music plays the tip of its tail keeps time, and now and
//! then a strong kick flicks an ear.
//!
//! Everything it does takes seconds. Only its legs move faster than that,
//! and only as fast as its feet cover the ground, so it never slides.
//!
//! Pure logic in logical pixels across the sill and seconds, so it can be
//! tested without a desktop. It decides where it is going; [`crate::carry`]
//! keeps where it rests.

use std::f32::consts::{PI, TAU};

use crate::carry::{CANDLE, CAT, COUNT, FOOT, GAP};
use crate::sill::SillState;

/// How long getting up takes, and lying down again, in seconds.
const RISING: f32 = 2.5;
const SETTLING: f32 = 3.0;

/// How it gets up and lies down: one end leads and the other follows, each on
/// a critically damped spring, so it starts gently and arrives without a
/// bump. Getting up its front leads, as a cat pushes up onto its forelegs and
/// then lifts its hindquarters; lying down its hindquarters lead. LEAD and
/// FOLLOW are the springs' natural frequencies, in radians a second.
const LEAD: f32 = 3.0;
const FOLLOW: f32 = 3.6;

/// How quickly a stretch fades if it is cut short.
const UP_TAU: f32 = 0.6;

/// How long a stretch takes, from first reaching forward to standing again.
const STRETCHING: f32 = 3.6;

/// How fast it walks, in logical pixels a second, and how far it goes with
/// each full stride. It picks up speed over about GATHER seconds, keeps a
/// steady pace, and slows to a stop over about BRAKE seconds at the end.
const PACE: f32 = 20.0;
const STRIDE: f32 = 32.0;
const GATHER: f32 = 0.35;
const BRAKE: f32 = 1.1;

/// How far a tap sends it, at most, in logical pixels, and the least room
/// worth walking into.
const WANDER: f32 = 160.0;
const ROOM: f32 = 45.0;

/// The furthest it goes by itself for the sun or the candle, and the least
/// difference worth getting up for, in logical pixels.
const FARTHEST: f32 = 320.0;
const NEAREST: f32 = 55.0;

/// How long it stays put after it last moved by itself, and after a hand last
/// set it down, in seconds of the sky's time. The hand's choice wins for a
/// good while.
const RESTED: f32 = 90.0 * 60.0;
const PUT: f32 = 3.0 * 60.0 * 60.0;

/// How much sun, from 0 to 1, and how far round to one side, from 0 to 1, it
/// takes for there to be a sunny side worth lying on.
const SUNNY: f32 = 0.3;
const SIDEWAYS: f32 = 0.25;

/// How it turns round: about a vertical axis, toward the room, its head
/// leading on a spring of HEAD radians a second and its body following the
/// head on one of BODY. Both are critically damped, so it starts the turn
/// gently and finishes it without swinging back.
const HEAD: f32 = 3.4;
const BODY: f32 = 2.8;

/// How long one breath takes asleep, and petted.
const BREATH: f32 = 4.2;
const BREATH_PETTED: f32 = 5.6;

/// How quickly petting is noticed and forgotten, in seconds.
const PET_IN: f32 = 0.9;
const PET_OUT: f32 = 1.4;

/// How quickly it goes loose in a hand and gathers itself again.
const HELD_TAU: f32 = 0.35;

/// How quickly its eyes open and close, and how long a slow blink takes.
const EYES_TAU: f32 = 0.5;
const BLINK: f32 = 0.9;

/// How far the tail tip swings with the beat, in degrees, and how slowly the
/// swing grows and dies away with the music.
const TAIL: f32 = 10.0;
const TAIL_TAU: f32 = 1.5;

/// The ear flick: how strong a kick sets it off, the least time between one
/// and the next, and the spring that flicks it and brings it back.
const STRUCK: f32 = 0.5;
const EAR_GAP: f32 = 6.0;
const EAR_PERIOD: f32 = 0.6;
const EAR_DAMPING: f32 = 0.6;
const EAR_KICK: f32 = 260.0;

/// Now and then in its sleep an ear twitches by itself, about this often, in
/// seconds, and this hard, in degrees a second.
const TWITCH_EVERY: f32 = 23.0;
const TWITCH: f32 = 110.0;

/// The longest step the springs are moved in one go.
const STEP: f32 = 1.0 / 60.0;

/// A frame longer than this means the sill was aside. Whatever the cat was
/// doing it has finished, and it is asleep where it was going.
const AWAY: f32 = 1.0;

/// What it is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Doing {
    Asleep,
    /// Getting up, to walk to `to`, stretching first if it was woken.
    Rising {
        to: f32,
        stretch: bool,
    },
    Stretching {
        to: f32,
    },
    /// Walking from `from` to `to`.
    Walking {
        from: f32,
        to: f32,
    },
    /// Lying down again.
    Settling,
}

/// What the cat can feel this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Feel {
    /// Where it is, in logical pixels across.
    pub x: f32,
    /// Whether a hand is carrying it.
    pub held: bool,
    /// Whether a pointer is resting on it.
    pub petted: bool,
    /// The music: how sure the beat is, how far through it, and the kick.
    pub beat: f32,
    pub beat_phase: f32,
    pub kick: f32,
}

/// The light it chooses where to lie by.
#[derive(Clone, Copy, Debug, Default)]
pub struct Light {
    /// How much sun reaches the sill, from 0 to 1.
    pub sun: f32,
    /// Which side the sun is on, -1 left to 1 right.
    pub side: f32,
    /// The candle's flame, from 0 to 1.
    pub candle: f32,
}

impl Light {
    /// The light the sill is heading into. Plain daylight, when nothing is
    /// known, has the sun straight behind, so there is no sunny side.
    pub fn of(state: &SillState) -> Self {
        let t = ((state.beam_angle + 5.0) / 7.0).clamp(0.0, 1.0);
        Self {
            sun: t * t * (3.0 - 2.0 * t) * state.sunshine,
            side: -state.shadow_side,
            candle: state.candle,
        }
    }

    /// Which side the sun is on, -1 left to 1 right, if it is out; otherwise 0.
    fn sunny_side(&self) -> f32 {
        if self.sun >= SUNNY { self.side } else { 0.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cat {
    doing: Doing,
    /// Seconds since it began doing it.
    since: f32,
    /// How far up its front and its hindquarters are, from 0 curled to 1
    /// standing, and how fast each is moving.
    front: f32,
    front_speed: f32,
    rear: f32,
    rear_speed: f32,
    /// How far into a stretch, from 0 to 1.
    stretch: f32,
    /// How many strides it has walked; only the part after the point shows.
    stride: f32,
    /// How far along its walk it is, in logical pixels, how fast it is going,
    /// and the pace it is gathering toward.
    walked: f32,
    speed: f32,
    gathering: f32,
    /// Which way it is turned, in radians from 0 facing right to pi facing
    /// left, its head and its body, how fast each is turning, and which way
    /// it means to face, -1 left or 1 right.
    head: f32,
    head_speed: f32,
    body: f32,
    body_speed: f32,
    face: f32,
    /// How far through a breath, in radians, and how deep they are.
    breath: f32,
    depth: f32,
    /// How petted it feels, and how loose it hangs in a hand, each 0 to 1.
    pet: f32,
    held: f32,
    /// How open its eyes are, from 0 to 1, and how long it has been awake.
    eyes: f32,
    awake: f32,
    /// How far the beat swings its tail tip, from 0 to 1.
    tail: f32,
    tail_phase: f32,
    /// How far its ear is flicked, in degrees, how fast, and seconds since the
    /// last flick and the last twitch in its sleep.
    ear: f32,
    ear_speed: f32,
    since_ear: f32,
    since_twitch: f32,
    /// Seconds it has slept, for the slow drift of its tail.
    asleep: f32,
    /// Seconds of the sky's time before it gets up by itself again.
    wait: f32,
    /// Where it has walked to this frame, while it walks.
    at: Option<f32>,
    /// Whether it lay down after a walk this frame.
    arrived: bool,
    /// How many times it has been woken, so each walk goes a different way.
    woken: u32,
}

impl Default for Cat {
    fn default() -> Self {
        Self {
            doing: Doing::Asleep,
            since: 0.0,
            front: 0.0,
            front_speed: 0.0,
            rear: 0.0,
            rear_speed: 0.0,
            stretch: 0.0,
            stride: 0.0,
            walked: 0.0,
            speed: 0.0,
            gathering: 0.0,
            head: 0.0,
            head_speed: 0.0,
            body: 0.0,
            body_speed: 0.0,
            face: 1.0,
            breath: 0.0,
            depth: 1.0,
            pet: 0.0,
            held: 0.0,
            eyes: 0.0,
            awake: 0.0,
            tail: 0.0,
            tail_phase: 0.0,
            ear: 0.0,
            ear_speed: 0.0,
            since_ear: EAR_GAP,
            since_twitch: 0.0,
            asleep: 0.0,
            wait: 0.0,
            at: None,
            arrived: false,
            woken: 0,
        }
    }
}

impl Cat {
    /// Woken by a tap at `x`, with the things resting at `spots` on a sill
    /// `width` logical px wide in `light`. It gets up, stretches and goes a
    /// little way, toward the sun if there is room that way. Only if it was
    /// asleep.
    pub fn wake(&mut self, x: f32, spots: [f32; COUNT], width: f32, light: Light) {
        if self.doing != Doing::Asleep {
            return;
        }
        let side = light.sunny_side();
        self.woken += 1;
        let (left, right) = room(x, spots, width);
        let toward = if side.abs() >= SIDEWAYS && (if side > 0.0 { right } else { left }) >= ROOM {
            side.signum()
        } else if right >= left {
            1.0
        } else {
            -1.0
        };
        let room = if toward > 0.0 { right } else { left };
        // Somewhere between half and all of the way it would go.
        let far = WANDER * (0.5 + 0.5 * (self.woken as f32 * 0.618).fract());
        let to = if room >= ROOM {
            x + toward * far.min(room)
        } else {
            x
        };
        self.start(Doing::Rising { to, stretch: true }, x);
    }

    /// The sky was looked at again: if it is time, and there is somewhere
    /// warmer to lie than `x`, it gets up and goes there.
    pub fn seek(&mut self, x: f32, spots: [f32; COUNT], width: f32, light: Light) {
        if self.doing != Doing::Asleep || self.wait > 0.0 {
            return;
        }
        let Some(to) = warmest(x, spots, width, light) else {
            return;
        };
        let far = (to - x).abs();
        if (NEAREST..=FARTHEST).contains(&far) {
            self.wait = RESTED;
            self.start(Doing::Rising { to, stretch: false }, x);
        }
    }

    /// Moves on by `dt` real seconds, `sky` of the sky's.
    pub fn advance(&mut self, dt: f32, sky: f32, feel: Feel) {
        self.at = None;
        self.arrived = false;
        self.wait = (self.wait - sky.max(0.0)).max(0.0);
        if dt > AWAY {
            if let Doing::Rising { to, .. } | Doing::Stretching { to } | Doing::Walking { to, .. } =
                self.doing
            {
                self.at = Some(to);
                self.arrived = true;
            }
            *self = Self {
                doing: Doing::Asleep,
                front: 0.0,
                front_speed: 0.0,
                rear: 0.0,
                rear_speed: 0.0,
                speed: 0.0,
                gathering: 0.0,
                stretch: 0.0,
                held: 0.0,
                pet: 0.0,
                eyes: 0.0,
                tail: 0.0,
                ear: 0.0,
                ear_speed: 0.0,
                head: facing(self.face),
                head_speed: 0.0,
                body: facing(self.face),
                body_speed: 0.0,
                ..*self
            };
            return;
        }
        if dt <= 0.0 {
            return;
        }

        // In a hand, whatever it was doing stops, and once it is put down it
        // lies down again where it was put, and stays a good while.
        if feel.held {
            if self.doing != Doing::Settling {
                self.start(Doing::Settling, feel.x);
            }
            self.since = 0.0;
            self.wait = PUT;
        }
        self.since += dt;
        let t = self.since;
        let (up_to, next) = match self.doing {
            Doing::Asleep => (0.0, None),
            Doing::Rising { to, stretch } => (
                1.0,
                (t >= RISING).then(|| {
                    if stretch {
                        Doing::Stretching { to }
                    } else {
                        walk(feel.x, to)
                    }
                }),
            ),
            Doing::Stretching { to } => (1.0, (t >= STRETCHING).then(|| walk(feel.x, to))),
            Doing::Walking { from, to } => {
                let there = self.pad(dt, (to - from).abs());
                self.at = Some(from + (to - from).signum() * self.walked);
                if there {
                    self.arrived = true;
                }
                (1.0, there.then_some(Doing::Settling))
            }
            Doing::Settling => (0.0, (t >= SETTLING).then_some(Doing::Asleep)),
        };
        if let Some(next) = next {
            self.doing = next;
            self.since = 0.0;
        }
        let held_to = if feel.held { 1.0 } else { 0.0 };
        self.held = toward(self.held, held_to, HELD_TAU, dt);
        // Held, it hangs half uncurled whatever it was doing.
        let up_to = if feel.held { 0.4 } else { up_to };
        self.rise(dt, up_to);
        self.stretch = match self.doing {
            Doing::Stretching { .. } => (PI * (self.since / STRETCHING).min(1.0)).sin().powi(2),
            _ => toward(self.stretch, 0.0, UP_TAU, dt),
        };
        self.stride = self.stride.fract();

        // It turns round only once it is up on its feet.
        if self.up() > 0.6 {
            self.turn(dt);
        }

        let awake = self.doing != Doing::Asleep || feel.held;
        self.awake = if awake { self.awake + dt } else { 0.0 };
        self.pet = toward(
            self.pet,
            if feel.petted && !feel.held { 1.0 } else { 0.0 },
            if feel.petted { PET_IN } else { PET_OUT },
            dt,
        );
        // Asleep its eyes are shut; petted, they open to slits and close again
        // in a slow blink now and then; up and about, they are open.
        let blink = |t: f32, every: f32| {
            let into = t % every;
            1.0 - (PI * (into / BLINK).min(1.0)).sin().powi(2)
        };
        let eyes_to = if awake {
            blink(self.awake, 4.3)
        } else {
            0.35 * self.pet * blink(self.since_ear + self.breath, 6.0)
        };
        self.eyes = toward(self.eyes, eyes_to, EYES_TAU, dt);

        self.asleep = if awake { 0.0 } else { self.asleep + dt };
        let period = BREATH + (BREATH_PETTED - BREATH) * self.pet;
        self.breath = (self.breath + TAU * dt / period) % TAU;
        self.depth = 1.0 + 0.7 * self.pet;

        // The music only reaches it while it sleeps.
        let beat = if self.doing == Doing::Asleep {
            feel.beat.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.tail = toward(self.tail, beat, TAIL_TAU, dt);
        self.tail_phase = feel.beat_phase;
        self.since_ear += dt;
        if feel.kick >= STRUCK && self.since_ear >= EAR_GAP && self.doing == Doing::Asleep {
            self.ear_speed += EAR_KICK;
            self.since_ear = 0.0;
        }
        // Unevenly spaced, so it never looks like clockwork.
        self.since_twitch += dt;
        let next = TWITCH_EVERY * (1.0 + 0.4 * (self.asleep * 0.013).sin());
        if self.doing == Doing::Asleep && self.since_twitch >= next {
            self.ear_speed += TWITCH;
            self.since_twitch = 0.0;
        }
        let steps = (dt / STEP).ceil().max(1.0);
        let h = dt / steps;
        let w = TAU / EAR_PERIOD;
        for _ in 0..steps as u32 {
            self.ear_speed += (-w * w * self.ear - 2.0 * EAR_DAMPING * w * self.ear_speed) * h;
            self.ear += self.ear_speed * h;
        }
    }

    /// Turns its head toward the way it means to face, and its body after
    /// its head.
    fn turn(&mut self, dt: f32) {
        let to = facing(self.face);
        let steps = (dt / STEP).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            self.head_speed += (HEAD * HEAD * (to - self.head) - 2.0 * HEAD * self.head_speed) * h;
            self.head += self.head_speed * h;
            self.body_speed +=
                (BODY * BODY * (self.head - self.body) - 2.0 * BODY * self.body_speed) * h;
            self.body += self.body_speed * h;
        }
        // Never further round than facing straight one way or the other.
        self.head = self.head.clamp(0.0, PI);
        self.body = self.body.clamp(0.0, PI);
    }

    /// How far up it is on the whole, from 0 curled to 1 standing.
    fn up(&self) -> f32 {
        0.5 * (self.front + self.rear)
    }

    /// Moves its front and hindquarters on by `dt` toward `to`: the end that
    /// leads chases `to`, and the other chases the end that leads.
    fn rise(&mut self, dt: f32, to: f32) {
        let front_leads = to >= self.up();
        let steps = (dt / STEP).ceil().max(1.0);
        let h = dt / steps;
        let chase = |value: &mut f32, speed: &mut f32, to: f32, w: f32| {
            *speed += (w * w * (to - *value) - 2.0 * w * *speed) * h;
            *value += *speed * h;
        };
        for _ in 0..steps as u32 {
            if front_leads {
                chase(&mut self.front, &mut self.front_speed, to, LEAD);
                let lead = self.front;
                chase(&mut self.rear, &mut self.rear_speed, lead, FOLLOW);
            } else {
                chase(&mut self.rear, &mut self.rear_speed, to, LEAD);
                let lead = self.rear;
                chase(&mut self.front, &mut self.front_speed, lead, FOLLOW);
            }
        }
    }

    /// Walks on by `dt` along a walk `far` logical pixels long, and says
    /// whether it is there. Its pace gathers toward PACE in two smooth lags,
    /// so it never lurches off, and it slows ahead of the end, as hard as it
    /// can without stopping short, so it comes to a stop rather than braking.
    fn pad(&mut self, dt: f32, far: f32) -> bool {
        let steps = (dt / STEP).ceil().max(1.0);
        let h = dt / steps;
        let k = 1.0 - (-h / GATHER).exp();
        for _ in 0..steps as u32 {
            let left = (far - self.walked).max(0.0);
            // The pace takes a moment to follow what it wants, so it looks
            // ahead by the ground it covers meanwhile.
            let ahead = (left - self.speed * 2.0 * GATHER).max(0.0);
            let want = PACE.min((2.0 * PACE / BRAKE * ahead).sqrt());
            self.gathering += (want - self.gathering) * k;
            self.speed += (self.gathering - self.speed) * k;
            let step = (self.speed * h).min(left);
            self.walked += step;
            self.stride += step / STRIDE;
        }
        let there = far - self.walked < 0.05 || (far - self.walked < 0.5 && self.speed < 0.5);
        if there {
            self.walked = far;
            self.speed = 0.0;
            self.gathering = 0.0;
        }
        there
    }

    /// Starts doing something new, from `x`.
    fn start(&mut self, doing: Doing, x: f32) {
        if let Doing::Rising { to, .. } = doing
            && (to - x).abs() > 1.0
        {
            self.face = (to - x).signum();
        }
        if let Doing::Walking { .. } = doing {
            self.walked = 0.0;
            self.speed = 0.0;
            self.gathering = 0.0;
        }
        self.doing = doing;
        self.since = 0.0;
    }

    /// Where it has walked to this frame, while it is walking.
    pub fn walking(&self) -> Option<f32> {
        self.at
    }

    /// Whether it lay down after a walk this frame, so where it rests should
    /// be kept.
    pub fn arrived(&self) -> bool {
        self.arrived
    }

    /// Whether it is up and doing something, so it should be drawn smoothly.
    pub fn busy(&self) -> bool {
        self.doing != Doing::Asleep || self.up() > 0.01 || self.stretch > 0.01
    }

    /// How tall it is right now, where a hand can take hold of it, in
    /// logical pixels.
    pub fn height(&self) -> f32 {
        40.0 + 20.0 * self.up()
    }

    /// How it is lying or standing, for the shader: how far up its front is,
    /// how far into a stretch, how far through a stride and how its breath
    /// swells; its tail and ear in degrees, how open its eyes are and how far
    /// its body is turned (the cosine: 1 facing right, -1 left); how petted
    /// and how held it is, how hard it is striding, and how far up its
    /// hindquarters are; and how far its head is turned, the same way.
    pub fn shape(&self) -> [[f32; 4]; 4] {
        // Asleep, the tail tip drifts a little on its own, slowly.
        let drift = (1.0 - self.up())
            * (3.0 * (self.asleep * 0.55).sin() + 2.0 * (self.asleep * 0.23).sin());
        let tail = self.tail * TAIL * (TAU * self.tail_phase).sin() + drift;
        // A breath is drawn in a little quicker than it is let go.
        let breath = (self.breath + 0.35 * self.breath.sin()).sin() * self.depth;
        [
            [self.front, self.stretch, self.stride, breath],
            [tail, self.ear, self.eyes, self.body.cos()],
            [self.pet, self.held, self.speed / PACE, self.rear],
            [self.head.cos(), 0.0, 0.0, 0.0],
        ]
    }
}

/// How far round it is turned to face `face`, -1 left or 1 right: 0 for
/// right, pi for left.
fn facing(face: f32) -> f32 {
    if face >= 0.0 { 0.0 } else { PI }
}

/// A walk from `from` to `to`, or straight to lying down if there's no way to go.
fn walk(from: f32, to: f32) -> Doing {
    let far = (to - from).abs();
    if far < 1.0 {
        return Doing::Settling;
    }
    Doing::Walking { from, to }
}

/// `value` eased toward `to` over `dt` seconds with time constant `tau`.
fn toward(value: f32, to: f32, tau: f32, dt: f32) -> f32 {
    to - (to - value) * (-dt / tau).exp()
}

/// How far the cat at `x` could walk left and right before it came up against
/// a neighbour or the end of the sill, in logical pixels.
fn room(x: f32, spots: [f32; COUNT], width: f32) -> (f32, f32) {
    let end = FOOT[CAT] + 2.0;
    let (mut lo, mut hi) = (end, (width - end).max(end));
    for (i, &at) in spots.iter().enumerate() {
        if i == CAT {
            continue;
        }
        let clear = FOOT[i] + FOOT[CAT] + GAP;
        if at <= x {
            lo = lo.max(at + clear);
        } else {
            hi = hi.min(at - clear);
        }
    }
    ((x - lo).max(0.0), (hi - x).max(0.0))
}

/// The warmest place to lie, starting from `x`: by the candle while it burns,
/// on the sun's side of whatever it lies beside while the sun is out and round
/// to one side, and otherwise nowhere in particular.
pub fn warmest(x: f32, spots: [f32; COUNT], width: f32, light: Light) -> Option<f32> {
    if light.candle >= 0.5 {
        let near = if x >= spots[CANDLE] { 1.0 } else { -1.0 };
        return [near, -near]
            .into_iter()
            .find_map(|side| beside(spots, width, CANDLE, side, false));
    }
    if light.sunny_side().abs() >= SIDEWAYS {
        let nearest = (0..COUNT)
            .filter(|&i| i != CAT)
            .min_by(|&a, &b| (spots[a] - x).abs().total_cmp(&(spots[b] - x).abs()))?;
        return beside(spots, width, nearest, light.side.signum(), true);
    }
    None
}

/// Where the cat would lie just to `side` of thing `next to`, clear of it. If
/// something else is already there and `on` is set, it goes beside that
/// instead, and so on down the row; otherwise there is no such place.
fn beside(spots: [f32; COUNT], width: f32, next: usize, side: f32, on: bool) -> Option<f32> {
    let end = FOOT[CAT] + 2.0;
    let mut by = next;
    for _ in 0..COUNT {
        let x = spots[by] + side * (FOOT[by] + FOOT[CAT] + GAP + 2.0);
        let blocked = (0..COUNT)
            .find(|&i| i != CAT && i != by && (spots[i] - x).abs() < FOOT[i] + FOOT[CAT] + GAP);
        match blocked {
            None => return (end..=width - end).contains(&x).then_some(x),
            Some(i) if on => by = i,
            Some(_) => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carry::PLANT;

    const WIDTH: f32 = 1920.0;

    /// Plant, jar and candle where the config puts them, and the cat to the
    /// right of the candle.
    fn spots() -> [f32; COUNT] {
        [0.72, 0.75, 0.775, 0.81].map(|at| at * WIDTH)
    }

    /// Runs the cat `seconds` at `fps`, walking it in the sill's place, and
    /// returns where it ends up and every phase it went through with how long
    /// each lasted.
    fn run(cat: &mut Cat, x: &mut f32, fps: f32, seconds: f32) -> Vec<(Doing, f32)> {
        let mut phases: Vec<(Doing, f32)> = Vec::new();
        for _ in 0..(seconds * fps) as u32 {
            cat.advance(
                1.0 / fps,
                1.0 / fps,
                Feel {
                    x: *x,
                    ..Feel::default()
                },
            );
            if let Some(at) = cat.walking() {
                *x = at;
            }
            match phases.last_mut() {
                Some((doing, time))
                    if std::mem::discriminant(doing) == std::mem::discriminant(&cat.doing) =>
                {
                    *time += 1.0 / fps;
                }
                _ => phases.push((cat.doing, 1.0 / fps)),
            }
        }
        phases
    }

    #[test]
    fn asleep_it_only_breathes() {
        let mut cat = Cat::default();
        let mut x = spots()[CAT];
        run(&mut cat, &mut x, 15.0, 30.0);
        assert_eq!(cat.doing, Doing::Asleep);
        assert_eq!(x, spots()[CAT]);
        assert!(!cat.busy());
        assert_eq!(cat.shape()[1][2], 0.0, "its eyes are shut");
    }

    #[test]
    fn a_tap_wakes_it_and_it_lies_down_again_a_little_way_off() {
        let mut cat = Cat::default();
        let mut x = spots()[CAT];
        cat.wake(x, spots(), WIDTH, Light::default());
        let phases = run(&mut cat, &mut x, 30.0, 30.0);
        let names: Vec<_> = phases
            .iter()
            .map(|(doing, _)| match doing {
                Doing::Asleep => "asleep",
                Doing::Rising { .. } => "rising",
                Doing::Stretching { .. } => "stretching",
                Doing::Walking { .. } => "walking",
                Doing::Settling => "settling",
            })
            .collect();
        assert_eq!(
            names,
            ["rising", "stretching", "walking", "settling", "asleep"]
        );
        // Nothing it does takes less than two seconds.
        for (doing, time) in &phases {
            assert!(*time >= 2.0, "{doing:?} took {time}");
        }
        // There's more room to the right, past the end of the row.
        let moved = x - spots()[CAT];
        assert!((ROOM..=WANDER).contains(&moved), "{moved}");
    }

    #[test]
    fn it_walks_no_faster_than_a_cat_pads() {
        let mut cat = Cat::default();
        let mut x = spots()[CAT];
        cat.wake(x, spots(), WIDTH, Light::default());
        let mut fastest: f32 = 0.0;
        for _ in 0..30 * 30 {
            let was = x;
            run(&mut cat, &mut x, 30.0, 1.0 / 30.0);
            fastest = fastest.max((x - was).abs() * 30.0);
        }
        assert!(fastest <= PACE * 1.6, "{fastest} px a second");
    }

    #[test]
    fn hemmed_in_it_stretches_and_lies_back_down_where_it_was() {
        let mut cat = Cat::default();
        let mut spots = spots();
        spots[CAT] = 0.5 * WIDTH;
        spots[PLANT] = spots[CAT] - (FOOT[PLANT] + FOOT[CAT] + GAP);
        spots[CANDLE] = spots[CAT] + FOOT[CANDLE] + FOOT[CAT] + GAP + 1.0;
        let mut x = spots[CAT];
        cat.wake(x, spots, WIDTH, Light::default());
        run(&mut cat, &mut x, 30.0, 20.0);
        assert_eq!(x, spots[CAT]);
        assert_eq!(cat.doing, Doing::Asleep);
    }

    #[test]
    fn it_goes_round_to_the_sunny_side() {
        let spots = spots();
        let x = spots[CAT];
        let morning = Light {
            sun: 1.0,
            side: -1.0,
            candle: 0.0,
        };
        let to = warmest(x, spots, WIDTH, morning).unwrap();
        assert!(to < spots[PLANT] - FOOT[PLANT], "{to}");
        // Already on the afternoon side, it stays.
        let afternoon = Light {
            side: 1.0,
            ..morning
        };
        let to = warmest(x, spots, WIDTH, afternoon).unwrap();
        assert!((to - x).abs() < NEAREST, "{to}");
    }

    #[test]
    fn with_the_sun_unknown_or_behind_it_stays() {
        let (spots, x) = (spots(), spots()[CAT]);
        let plain = Light {
            sun: 1.0,
            side: 0.0,
            candle: 0.0,
        };
        assert_eq!(warmest(x, spots, WIDTH, plain), None);
        let night = Light {
            sun: 0.0,
            side: -1.0,
            candle: 0.0,
        };
        assert_eq!(warmest(x, spots, WIDTH, night), None);
    }

    #[test]
    fn by_night_it_lies_by_the_candle() {
        let mut spots = spots();
        spots[CAT] = 0.3 * WIDTH;
        let light = Light {
            sun: 0.0,
            side: 0.0,
            candle: 1.0,
        };
        let to = warmest(spots[CAT], spots, WIDTH, light).unwrap();
        // The candle's left is the jar, so it goes to the candle's right.
        assert!(
            to > spots[CANDLE] && to - spots[CANDLE] < FOOT[CANDLE] + FOOT[CAT] + GAP + 3.0,
            "{to}"
        );
    }

    #[test]
    fn it_gets_up_for_the_sun_but_not_too_often_and_not_after_a_hand() {
        let spots = spots();
        let morning = Light {
            sun: 1.0,
            side: -1.0,
            candle: 0.0,
        };
        let mut cat = Cat::default();
        let mut x = spots[CAT];
        cat.seek(x, spots, WIDTH, morning);
        assert!(matches!(cat.doing, Doing::Rising { stretch: false, .. }));
        run(&mut cat, &mut x, 30.0, 30.0);
        assert_eq!(cat.doing, Doing::Asleep);
        assert!(x < spots[PLANT], "{x}");
        // Straight back the other way an hour later? No.
        let afternoon = Light {
            side: 1.0,
            ..morning
        };
        let moved = {
            let mut spots = spots;
            spots[CAT] = x;
            spots
        };
        cat.advance(
            0.5,
            60.0 * 60.0,
            Feel {
                x,
                ..Feel::default()
            },
        );
        cat.seek(x, moved, WIDTH, afternoon);
        assert_eq!(cat.doing, Doing::Asleep);

        // Set down by a hand, it stays three hours whatever the sun does.
        let mut cat = Cat::default();
        let x = spots[CAT];
        cat.advance(
            0.1,
            0.1,
            Feel {
                x,
                held: true,
                ..Feel::default()
            },
        );
        for _ in 0..10 {
            cat.advance(
                0.5,
                0.5 * 60.0 * 60.0 * 0.5,
                Feel {
                    x,
                    ..Feel::default()
                },
            );
        }
        cat.seek(x, spots, WIDTH, morning);
        assert_eq!(cat.doing, Doing::Asleep);
    }

    #[test]
    fn petted_it_opens_its_eyes_a_little_and_breathes_deeper() {
        let mut cat = Cat::default();
        for _ in 0..60 {
            cat.advance(
                1.0 / 15.0,
                1.0 / 15.0,
                Feel {
                    petted: true,
                    ..Feel::default()
                },
            );
        }
        let [a, b, c, _] = cat.shape();
        assert!(c[0] > 0.9, "{c:?}");
        assert!(b[2] <= 0.36, "only to slits: {b:?}");
        assert!(a[3].abs() <= 1.7);
        assert_eq!(cat.doing, Doing::Asleep, "petting doesn't get it up");
    }

    #[test]
    fn asleep_its_tail_keeps_time_and_a_kick_flicks_an_ear() {
        let mut cat = Cat::default();
        let mut most: [f32; 2] = [0.0; 2];
        let mut flicks = 0;
        for frame in 0..30 * 20 {
            let t = frame as f32 / 30.0;
            let beat_phase = (t * 2.0).fract();
            let kick = (-(beat_phase / 2.0) / 0.1).exp();
            let before = cat.since_ear;
            cat.advance(
                1.0 / 30.0,
                1.0 / 30.0,
                Feel {
                    beat: 0.9,
                    beat_phase,
                    kick,
                    ..Feel::default()
                },
            );
            if cat.since_ear < before {
                flicks += 1;
            }
            let [_, b, _, _] = cat.shape();
            most[0] = most[0].max(b[0].abs());
            most[1] = most[1].max(b[1].abs());
        }
        assert!(most[0] > TAIL * 0.6 && most[0] <= TAIL + 5.0, "{most:?}");
        assert!(most[1] > 5.0 && most[1] < 40.0, "{most:?}");
        // Twice a second for twenty seconds, but at most once every six.
        assert!((3..=4).contains(&flicks), "{flicks}");
    }

    #[test]
    fn it_gets_up_front_first_and_lies_down_hindquarters_first() {
        let mut cat = Cat::default();
        let mut x = spots()[CAT];
        cat.wake(x, spots(), WIDTH, Light::default());
        run(&mut cat, &mut x, 30.0, 0.8);
        assert!(cat.front > cat.rear + 0.1, "{} {}", cat.front, cat.rear);
        // It starts gently: no part jumps up in the first frames.
        let mut fresh = Cat::default();
        fresh.wake(x, spots(), WIDTH, Light::default());
        run(&mut fresh, &mut x, 30.0, 0.1);
        assert!(fresh.front < 0.05, "{}", fresh.front);

        let mut cat = Cat {
            front: 1.0,
            rear: 1.0,
            doing: Doing::Settling,
            ..Cat::default()
        };
        run(&mut cat, &mut x, 30.0, 0.8);
        assert!(cat.rear < cat.front - 0.1, "{} {}", cat.front, cat.rear);
        run(&mut cat, &mut x, 30.0, 3.0);
        assert!(cat.up() < 0.02);
    }

    #[test]
    fn it_gathers_pace_keeps_it_and_comes_to_a_stop_where_it_meant_to() {
        let mut cat = Cat::default();
        let start = 500.0;
        cat.start(
            Doing::Walking {
                from: start,
                to: start + 120.0,
            },
            start,
        );
        let mut speeds = Vec::new();
        let mut x = start;
        for _ in 0..30 * 12 {
            run(&mut cat, &mut x, 30.0, 1.0 / 30.0);
            speeds.push(cat.speed);
            assert!(x <= start + 120.0 + 1e-3, "overshot to {x}");
        }
        assert!(speeds[2] < PACE * 0.2, "lurched off at {}", speeds[2]);
        let most = speeds.iter().cloned().fold(0.0, f32::max);
        assert!(most > PACE * 0.9 && most <= PACE + 1e-3, "{most}");
        // Slowing smoothly: never more than a small change in a frame.
        for pair in speeds.windows(2) {
            assert!((pair[1] - pair[0]).abs() < PACE * 0.12, "{pair:?}");
        }
        assert!((x - (start + 120.0)).abs() < 1e-3);
    }

    #[test]
    fn it_turns_round_head_first_smoothly_and_no_further_than_round() {
        let mut cat = Cat::default();
        let mut x = spots()[CAT];
        // Woken with room only to the left, so it has to turn round.
        let mut hemmed = spots();
        hemmed[CANDLE] = x + FOOT[CANDLE] + FOOT[CAT] + GAP + 1.0;
        cat.wake(x, hemmed, WIDTH, Light::default());
        let (mut led, mut fastest) = (false, 0.0f32);
        for _ in 0..30 * 8 {
            let was = cat.body;
            run(&mut cat, &mut x, 30.0, 1.0 / 30.0);
            led |= cat.head > cat.body + 0.1;
            fastest = fastest.max((cat.body - was).abs() * 30.0);
            assert!((0.0..=PI).contains(&cat.body) && (0.0..=PI).contains(&cat.head));
        }
        assert!(led, "its head never led");
        assert!(fastest < 3.0, "turned at {fastest} radians a second");
        assert!((cat.body - PI).abs() < 0.02, "{}", cat.body);
        assert!(cat.shape()[1][3] < -0.99);
    }

    #[test]
    fn the_same_at_15_and_30_fps() {
        let walked = |fps: f32| {
            let mut cat = Cat::default();
            let mut x = spots()[CAT];
            cat.wake(x, spots(), WIDTH, Light::default());
            run(&mut cat, &mut x, fps, 9.0);
            (x, cat.up())
        };
        let (slow, fast) = (walked(15.0), walked(30.0));
        assert!((slow.0 - fast.0).abs() < 4.0, "{slow:?} vs {fast:?}");
        assert!((slow.1 - fast.1).abs() < 0.05, "{slow:?} vs {fast:?}");
    }

    #[test]
    fn after_being_aside_it_is_asleep_where_it_was_going() {
        let mut cat = Cat::default();
        let mut x = spots()[CAT];
        cat.wake(x, spots(), WIDTH, Light::default());
        run(&mut cat, &mut x, 30.0, 3.0);
        cat.advance(
            60.0,
            60.0,
            Feel {
                x,
                ..Feel::default()
            },
        );
        assert_eq!(cat.doing, Doing::Asleep);
        assert!(cat.arrived());
        assert!(cat.walking().is_some_and(|to| to > x));
    }
}
