// The sill: a potted plant, a small glass jar, a candle and a sleeping cat on
// the taskbar's top edge, which is the strip's bottom edge, in the light of the
// real sky. Everything else is clear, so the desktop shows through all round
// them. Every shape is a signed distance in logical pixels, so the things are
// equally crisp at any DPI. Colours leave here premultiplied.

struct Sill {
    // The strip's height in physical pixels.
    height: f32,
    // Physical pixels per logical pixel.
    scale: f32,
    // 0 while the sill is away, easing to 1 as it settles in.
    presence: f32,
    pad: f32,
    // The jar's wooden lid, and its lit top edge, from windowsill.toml. Only
    // .rgb is used.
    wood: vec4<f32>,
    lip_light: vec4<f32>,
    // The eased sill state: beam angle in degrees, colour temperature in
    // kelvin, shadow side from -1 to 1, and moonlight from 0 to 1.
    light: vec4<f32>,
    // How much of the sun the room's window lets in, from 0 to 1, which side
    // its light comes from, -1 left, 0 ahead, 1 right, and in .z which way
    // the moon's shadows fall.
    room: vec4<f32>,
    // The colour of the room's light at this hour, and in .w how much of their
    // own colour things keep in it.
    sky: vec4<f32>,
    // The candle's flame in .x, from 0 to 1, how much of it is left in .y,
    // and in .zw how its flame breathes: height and sway, each -1 to 1.
    candle: vec4<f32>,
    // How stirred the dust in the air is by the music, from 0 to 1, in .y how
    // far through its cycle it has drifted, from 0 to 1, in .z how many
    // degrees the plant's leaves lean to the right, and in .w how bright the
    // jar's ring of light is, from 0 to 1.
    music: vec4<f32>,
    // Where the plant, jar, candle and cat stand, in physical pixels across.
    places: vec4<f32>,
    // How high each is lifted off the edge by a hand, in logical pixels.
    lift: vec4<f32>,
    // How far each tips as it is carried, in degrees to the right.
    tip: vec4<f32>,
    // How far what is loose inside each trails as it is carried, in degrees to
    // the right: the plant's leaves, the jar's water and the candle's flame.
    swing: vec4<f32>,
    // Which thing (0 to 3) is in front: the last one picked up. In .y, how
    // fast the music is swinging the leaves, in degrees a second.
    order: vec4<f32>,
    // How the cat lies or stands. [0]: how far up, from 0 curled to 1
    // standing; how far into a stretch; how far through a stride; and its
    // breath, -1 to 1 and deeper when petted. [1]: how far its tail tip swings
    // and its ear is flicked, in degrees; how open its eyes are; and which
    // way it faces, -1 left to 1 right. [2]: how petted it is, and how loose
    // it hangs in a hand.
    cat: array<vec4<f32>, 3>,
    // The things on the sill, from windowsill.toml. Only .rgb is used.
    pot: vec4<f32>,
    leaf: vec4<f32>,
    glass: vec4<f32>,
    water: vec4<f32>,
    wax: vec4<f32>,
    flame: vec4<f32>,
    fur: vec4<f32>,
    mark: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sill: Sill;

// Where things stand: on the strip's bottom edge, which is the taskbar's top.
const BASE: f32 = 0.0;
// Which side the room's own light comes from, -1 left to 1 right, when the
// window is straight ahead. It models the things on the sill whatever the sun
// is doing.
const ROOM: f32 = -0.45;
// Moonlight: the sun's light off grey rock, which the eye at night sees as blue.
const MOON: vec3<f32> = vec3<f32>(0.62, 0.74, 1.0);
// How far from each thing anything of it is drawn, in logical pixels: its
// leaves or its light. Beyond, the strip is clear.
const NEAR: f32 = 150.0;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that covers the whole strip.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

// Interleaved gradient noise (Jimenez 2014): a cheap, even dither that keeps an
// 8-bit fade from breaking into bands.
fn dither(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715)))) - 0.5;
}

// ---- The light ------------------------------------------------------------
//
// The sun is outside, beyond the screen, so it reaches the things from behind.
// It catches their edges on its side and glows through the leaves and the
// water. The rest of the day the room's light models them, from the window's
// side, and at night the moon catches an edge coolly.

// How much sun reaches the sill, from 0 at night to 1. It fades through the
// few degrees either side of the horizon.
fn sun() -> f32 {
    return smoothstep(-5.0, 2.0, sill.light.x);
}

// How strongly it shines where it does: a little softer when it is low, and
// not at all while it is round the side of the window.
fn shine() -> f32 {
    return sun() * sill.room.x * (0.78 + 0.22 * sin(radians(clamp(sill.light.x, 0.0, 90.0))));
}

// Which side the room's own light comes from, -1 left to 1 right: the
// window's side, or ROOM when the window is straight ahead.
fn room_side() -> f32 {
    return mix(ROOM, sill.room.y * 0.6, min(abs(sill.room.y) * 3.0, 1.0));
}

// The same, settled to a whole side, for highlights that sit on one side.
fn room_hand() -> f32 {
    return clamp(room_side() * 4.0, -1.0, 1.0);
}

// Which side the sun is on, -1 left to 1 right: the side shadows fall away from.
fn sun_side() -> f32 {
    return -sill.light.z;
}

// How much of the moon's light reaches the sill, from 0 to 1: its phase, and
// whether it is up and the sky dark, are already counted.
fn moon() -> f32 {
    return sill.light.w;
}

// Which side the moon is on, -1 left to 1 right.
fn moon_side() -> f32 {
    return -sill.room.z;
}

// The sunlight's colour at its temperature (Tanner Helland's fit to the
// blackbody curve), scaled so its brightest channel is 1.
fn sunlight() -> vec3<f32> {
    let t = clamp(sill.light.y, 1000.0, 12000.0) / 100.0;
    let warm = t <= 66.0;
    let r = select(329.698727446 * pow(max(t - 60.0, 1.0), -0.1332047592) / 255.0, 1.0, warm);
    let g = select(
        288.1221695283 * pow(max(t - 60.0, 1.0), -0.0755148492),
        99.4708025861 * log(t) - 161.1195681661,
        warm,
    ) / 255.0;
    let b = select(
        select(0.0, (138.5177312231 * log(max(t - 10.0, 1.0)) - 305.0447927307) / 255.0, t > 19.0),
        1.0,
        t >= 66.0,
    );
    let rgb = clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
    return rgb / max(max(rgb.r, rgb.g), rgb.b);
}

// Something lit by the room at this hour (Fishbowl's palette): it takes on the
// colour of the light, and loses some of its own as the light fails. Light
// that makes itself, like the candle's flame, is left alone. It is linear, so
// it grades premultiplied colour as well as plain.
fn graded(rgb: vec3<f32>) -> vec3<f32> {
    let lit = rgb * sill.sky.rgb;
    let grey = dot(lit, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(grey), lit, sill.sky.w);
}

// ---- Drawing helpers ------------------------------------------------------

// How much of this pixel a shape covers, given its distance in logical pixels.
fn cover(d: f32) -> f32 {
    return clamp(0.5 - d * sill.scale, 0.0, 1.0);
}

// Paints `rgb` at opacity `a` over what is already there.
fn over(under: vec4<f32>, rgb: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(rgb * a, a) + under * (1.0 - a);
}

// Lays a premultiplied `layer` over what is already there.
fn under_layer(under: vec4<f32>, layer: vec4<f32>) -> vec4<f32> {
    return layer + under * (1.0 - layer.a);
}

// `q`, measured from a thing's foot, as the thing sees it while it tips `deg`
// degrees to the right: turned back upright about its foot.
fn upright(q: vec2<f32>, deg: f32) -> vec2<f32> {
    let a = radians(deg);
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(q.x * c - q.y * s, q.x * s + q.y * c);
}

// How a rounded face is lit, `u` from -1 at its left edge to 1 at its right, as
// if it were a cylinder: the room's light from the front, and the sun behind
// it catching the edge it can reach.
fn form(u: f32) -> vec3<f32> {
    let x = clamp(u, -1.0, 1.0);
    let normal = vec3<f32>(x, 0.0, sqrt(1.0 - x * x));
    let room = normalize(vec3<f32>(room_side(), 0.0, 0.9));
    let fill = 0.62 + 0.45 * max(dot(normal, room), 0.0);
    // Both edges at noon, with the sun straight behind; one edge as it swings.
    let edge = smoothstep(0.55, 1.0, abs(x)) * clamp(0.5 + 0.75 * sign(x) * sun_side() * 2.0, 0.0, 1.0);
    // The moon catches an edge the same way, coolly.
    let moon_edge = smoothstep(0.5, 1.0, abs(x)) * clamp(0.5 + 1.5 * sign(x) * moon_side(), 0.0, 1.0);
    return vec3<f32>(fill) + sunlight() * edge * shine() * 0.4 + MOON * moon_edge * moon() * 0.9;
}

// A hash without sine (Dave Hoskins), steady at the large x of a wide screen.
fn hash(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// ---- Signed distances (after Inigo Quilez) --------------------------------

fn sd_box(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// A trapezoid `bottom` and `top` half-widths wide and `half_height` half high.
fn sd_trapezoid(p: vec2<f32>, bottom: f32, top: f32, half_height: f32) -> f32 {
    let k1 = vec2<f32>(top, half_height);
    let k2 = vec2<f32>(top - bottom, 2.0 * half_height);
    let q = vec2<f32>(abs(p.x), p.y);
    let ca = vec2<f32>(q.x - min(q.x, select(top, bottom, q.y < 0.0)), abs(q.y) - half_height);
    let cb = q - k1 + k2 * clamp(dot(k1 - q, k2) / dot(k2, k2), 0.0, 1.0);
    let s = select(1.0, -1.0, cb.x < 0.0 && ca.y < 0.0);
    return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}

fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
}

// A pointed leaf shape, `len` tall along y and `width` wide.
fn sd_leaf(p: vec2<f32>, len: f32, width: f32) -> f32 {
    let b = len * 0.5;
    let d = (b * b - width * width * 0.25) / width;
    let r = d + width * 0.5;
    let q = abs(p);
    if (q.y - b) * d > q.x * b {
        return distance(q, vec2<f32>(0.0, b));
    }
    return distance(q, vec2<f32>(-d, 0.0)) - r;
}

// A teardrop: a circle of radius `r1` at the origin narrowing to `r2` at `h` up.
fn sd_teardrop(p: vec2<f32>, r1: f32, r2: f32, h: f32) -> f32 {
    let q = vec2<f32>(abs(p.x), p.y);
    let b = (r1 - r2) / h;
    let a = sqrt(1.0 - b * b);
    let k = dot(q, vec2<f32>(-b, a));
    if k < 0.0 {
        return length(q) - r1;
    }
    if k > a * h {
        return length(q - vec2<f32>(0.0, h)) - r2;
    }
    return dot(q, vec2<f32>(a, b)) - r1;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

// ---- Where things touch the edge ------------------------------------------

// A thin dark line just above the edge where something `half_width` wide stands
// at `x`, so it rests on the taskbar rather than floats over it. Lifted `lift`
// px, it thins and spreads, and the gap shows.
fn foot(colour: vec4<f32>, p: vec2<f32>, x: f32, half_width: f32, lift: f32) -> vec4<f32> {
    let up = 1.0 - lift / 6.0;
    let across = abs(p.x - x) / (half_width * (1.0 + lift * 0.06));
    let shade = (1.0 - smoothstep(0.0, 2.2 + lift * 0.4, p.y))
        * (1.0 - smoothstep(0.55, 1.2, across)) * 0.3 * clamp(up, 0.35, 1.0);
    return over(colour, vec3<f32>(0.0), shade);
}

// ---- The plant ------------------------------------------------------------

fn plant(under: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    var colour = under;
    let root = vec2<f32>(0.0, BASE + 21.0);
    let dark = sill.leaf.rgb * 0.5;

    // Seven leaves fanned from the pot, the outer ones behind and in shade,
    // each drooping a little more the further out it leans. The bass leans
    // them all one way, the long ones a little further than the short, and
    // the long ones trail the short as they swing, so the plant ripples.
    // Carried, they trail back against the air the same way.
    var angles = array<f32, 7>(-64.0, -40.0, -18.0, 2.0, 21.0, 42.0, 63.0);
    var lengths = array<f32, 7>(19.0, 26.0, 31.0, 34.0, 30.0, 25.0, 18.0);
    var stems = array<f32, 7>(5.0, 8.0, 11.0, 13.0, 10.0, 8.0, 5.0);
    var order = array<i32, 7>(0, 6, 1, 5, 2, 4, 3);
    var depth = array<f32, 7>(0.74, 0.74, 0.86, 0.86, 0.95, 0.95, 1.0);
    let lean = sill.music.z + sill.swing.x;
    for (var k = 0; k < 7; k++) {
        let i = order[k];
        let len = lengths[i];
        let reach = len / 34.0;
        let trail = sill.order.y * 0.14 * reach * reach;
        let angle = radians(angles[i] + lean * (0.55 + 0.45 * reach) - trail);
        let dir = vec2<f32>(sin(angle), cos(angle));
        let across = vec2<f32>(dir.y, -dir.x);
        let start = root + dir * stems[i];

        let stem = sd_segment(p, root, start) - 0.55;
        colour = over(colour, dark * depth[k], cover(stem));

        let q = p - (start + dir * len * 0.5);
        let along = dot(q, dir);
        let t = clamp(along / len + 0.5, 0.0, 1.0);
        let side = dot(q, across) - 0.2 * len * abs(dir.x) * t * t;
        let d = sd_leaf(vec2<f32>(side, along), len, len * 0.4);
        var green = mix(dark, sill.leaf.rgb, smoothstep(0.0, 0.75, t));
        // Lit on the sun's side of the midrib, with the rib itself catching
        // light, and the sun glowing through the leaf from behind.
        green *= 1.0 + 0.12 * sign(side) * room_hand();
        green += sill.leaf.rgb * sunlight() * shine() * (0.16 + 0.1 * sign(side) * sun_side()) * t;
        let rib = (1.0 - smoothstep(0.0, 0.55, abs(side))) * step(0.06, t) * (1.0 - t);
        green = mix(green, sill.leaf.rgb * 1.3, rib * 0.45);
        // A slightly darker rim keeps overlapping leaves apart.
        green *= mix(0.8, 1.0, smoothstep(0.0, 1.1, -d));
        colour = over(colour, green * depth[k], cover(d));
    }

    // The pot, over the stems' feet: a tapered clay body under a broader rim
    // that shades the top of the body.
    let body = sd_trapezoid(p - vec2<f32>(0.0, BASE + 9.0), 8.5, 11.5, 9.0);
    let body_half = mix(8.5, 11.5, clamp((p.y - BASE) / 18.0, 0.0, 1.0));
    var clay = sill.pot.rgb * form(p.x / body_half);
    clay *= mix(0.72, 1.0, smoothstep(BASE + 18.0, BASE + 14.5, p.y));
    colour = over(colour, clay, cover(body));

    let rim = sd_box(p - vec2<f32>(0.0, BASE + 20.5), vec2<f32>(13.0, 3.0), 1.0);
    var rim_colour = sill.pot.rgb * 1.08 * form(p.x / 13.0);
    rim_colour = mix(rim_colour, rim_colour * 1.15, smoothstep(BASE + 22.0, BASE + 23.4, p.y));
    return over(colour, rim_colour, cover(rim));
}

// ---- The jar --------------------------------------------------------------

// The jar, tipped `tip` degrees, with its water leaning `slosh` degrees more
// than the world's level.
fn jar(under: vec4<f32>, p: vec2<f32>, tip: f32, slosh: f32) -> vec4<f32> {
    var colour = under;
    let body = sd_box(p - vec2<f32>(0.0, BASE + 12.5), vec2<f32>(11.0, 12.5), 5.0);
    let neck = sd_box(p - vec2<f32>(0.0, BASE + 26.5), vec2<f32>(7.5, 2.0), 0.8);
    let glass = smin(body, neck, 3.0);
    let glass_rgb = sill.glass.rgb;

    // Faintly tinted through the middle.
    colour = over(colour, glass_rgb, cover(glass) * 0.08);

    // Water a little over half way, denser where the glass curves away, with a
    // bright line where it meets the air. It lies level with the world, not the
    // jar, and piles up a little against whichever side it is pulled from.
    let level = BASE + 15.0 + p.x * tan(radians(clamp(slosh - tip, -30.0, 30.0)));
    let water = max(body + 1.2, p.y - level);
    let curve = abs(p.x) / 11.0;
    colour = over(colour, sill.water.rgb, cover(water) * (0.26 + 0.2 * curve * curve));
    // The sun comes through the water from behind and lights it from within.
    colour = over(colour, mix(sill.water.rgb, sunlight(), 0.6), cover(water) * shine() * 0.16 * (1.0 - curve));
    let meniscus = max(abs(p.y - level) - 0.45, body + 1.2);
    colour = over(colour, mix(glass_rgb, vec3<f32>(1.0), 0.4), cover(meniscus) * 0.55);

    // The glass itself shows mostly at its edges.
    let wall = abs(glass + 0.7) - 0.7;
    let edge_light = mix(0.75, 1.1, 0.5 + 0.5 * sign(p.x) * room_hand());
    let edge_sun = sunlight() * shine() * 0.35 * smoothstep(0.0, 1.0, sign(p.x) * sun_side() * 2.0 + 0.5);
    colour = over(colour, glass_rgb * edge_light + edge_sun, cover(wall) * 0.5);

    // When a kick strikes it, the glass rings with a faint light: round its
    // walls, along the water's edge, and in a soft halo just outside.
    let ring = sill.music.w;
    if ring > 0.002 {
        let rung = mix(glass_rgb, vec3<f32>(1.0), 0.7);
        colour = over(colour, rung, cover(wall) * ring * 0.35);
        colour = over(colour, rung, cover(meniscus) * ring * 0.3);
        let out = max(glass, 0.0);
        let halo = exp(-out * out / (2.0 * 3.0 * 3.0)) * (1.0 - cover(glass));
        colour = over(colour, rung, halo * ring * 0.1);
    }

    // A long highlight down the lit side, and a small one on the shoulder.
    let side = room_hand();
    let streak = sd_segment(p, vec2<f32>(side * 6.8, BASE + 6.0), vec2<f32>(side * 6.8, BASE + 19.0)) - 0.75;
    colour = over(colour, mix(glass_rgb, vec3<f32>(1.0), 0.6), cover(streak) * 0.55);
    let glint = length(p - vec2<f32>(side * 4.0, BASE + 23.2)) - 0.9;
    colour = over(colour, vec3<f32>(1.0), cover(glint) * 0.5);

    // A wooden lid.
    let lid = sd_box(p - vec2<f32>(0.0, BASE + 30.5), vec2<f32>(9.0, 2.5), 1.2);
    var wood = mix(sill.wood.rgb, sill.lip_light.rgb, 0.5) * form(p.x / 9.0);
    wood = mix(wood, sill.lip_light.rgb * 1.1, smoothstep(BASE + 31.6, BASE + 32.8, p.y));
    return over(colour, wood, cover(lid));
}

// ---- The candle -----------------------------------------------------------

// The candle, tipped `tip` degrees, its flame leaning `trail` degrees more than
// straight up as it is carried through the air.
fn candle(under: vec4<f32>, p: vec2<f32>, tip: f32, trail: f32) -> vec4<f32> {
    var colour = under;
    let lit = sill.candle.x;
    // It burns down through the night.
    let tall = 24.0 * sill.candle.y;
    let top = BASE + tall;

    // A pillar of wax with one drip down its side.
    let pillar = sd_box(p - vec2<f32>(0.0, BASE + tall * 0.5), vec2<f32>(7.0, tall * 0.5), 1.2);
    let drip = sd_segment(p, vec2<f32>(6.6, top - 1.0), vec2<f32>(6.7, top - min(8.5, tall * 0.4))) - 1.1;
    let wax_shape = smin(pillar, drip, 1.6);
    var wax = graded(sill.wax.rgb * form(p.x / 7.0));
    // Lit wax glows from inside near the flame.
    let inner = lit * exp(-(top - p.y) / 5.0);
    wax = mix(wax, sill.flame.rgb, inner * 0.35) * (1.0 + inner * 0.2);
    // The softened rim where the wax has pooled.
    wax = mix(wax, sill.wax.rgb * 1.08, (1.0 - smoothstep(0.0, 1.0, abs(p.y - (top - 0.8)))) * 0.5);
    colour = over(colour, wax, cover(wax_shape));

    // The wick, with an ember at its tip while it burns.
    let wick_top = vec2<f32>(0.5, top + 3.5);
    let wick = sd_segment(p, vec2<f32>(0.0, top - 0.5), wick_top) - 0.45;
    let ember = lit * smoothstep(top + 1.5, top + 3.5, p.y);
    colour = over(colour, mix(graded(vec3<f32>(0.12, 0.09, 0.07)), sill.flame.rgb * 1.2, ember), cover(wick));

    // The flame: a teardrop that grows as the candle catches, a pale core and a
    // touch of blue at its foot. It breathes a little taller and shorter, and
    // its tip sways, slowly. It burns straight up whichever way the candle
    // tips, and trails back as it is carried.
    let size = smoothstep(0.0, 1.0, lit);
    let breath = sill.candle.z;
    let r = 2.7 * size + 0.01;
    let height = (10.0 + 0.8 * breath) * size + 0.02;
    var q = p - (wick_top + vec2<f32>(0.0, r * 0.4));
    let rise = clamp(q.y / height, 0.0, 1.0);
    let bend = tan(radians(clamp(trail - tip, -40.0, 40.0))) * height;
    q.x -= (sill.candle.w * 0.7 * size + bend) * rise * rise;
    let flame = sd_teardrop(q, r, 0.3 * size + 0.005, height - r);
    let core = sd_teardrop(q - vec2<f32>(0.0, -r * 0.2), r * 0.55, 0.1, (height - r) * 0.55);
    var fire = sill.flame.rgb;
    fire = mix(fire, vec3<f32>(0.42, 0.52, 0.95), (1.0 - smoothstep(-r, 0.0, q.y)) * 0.35);
    fire = mix(fire, vec3<f32>(1.0, 0.96, 0.86), cover(core));
    colour = over(colour, fire, cover(flame) * smoothstep(0.02, 0.3, lit));

    // The warmth it throws around itself, into the air close by.
    let reach = length(q - vec2<f32>(0.0, height * 0.3));
    let warmth = lit * (0.28 + 0.02 * breath) * exp(-reach * reach / (2.0 * 15.0 * 15.0));
    return vec4<f32>(colour.rgb + sill.flame.rgb * warmth, colour.a + warmth * (1.0 - colour.a));
}

// Where the candle's flame is, in logical pixels.
fn flame_at(x: vec4<f32>) -> vec2<f32> {
    return vec2<f32>(x.z + 0.5, sill.lift.z + BASE + 24.0 * sill.candle.y + 6.0);
}

// The candle's light on the things near it, warmed in a wide pool that
// breathes with the flame. It lights what is there rather than glowing in the
// air, so the desktop stays clear.
fn candlelight(layer: vec4<f32>, p: vec2<f32>, x: vec4<f32>) -> vec4<f32> {
    let d = length((p - flame_at(x)) * vec2<f32>(1.0, 1.6));
    let pool = sill.candle.x * (0.55 + 0.03 * sill.candle.z) * exp(-d * d / (2.0 * 44.0 * 44.0));
    let warm = sill.flame.rgb * pool;
    return vec4<f32>(layer.rgb * (1.0 + warm * 2.6) + warm * 0.06 * layer.a, layer.a);
}

// ---- The cat --------------------------------------------------------------
//
// One body that gets up and lies down, rather than a drawing for each pose:
// how far up it is moves every part from curled, head tucked against its
// flank and tail wrapped round its feet, to standing, head up and tail high.
// Seen side on, facing right; it is mirrored to face left, and squeezed
// narrow through the turn.

// An ellipse with half-axes `r` (after Inigo Quilez), near enough at this size.
fn sd_ellipse(p: vec2<f32>, r: vec2<f32>) -> f32 {
    let k0 = length(p / r);
    let k1 = max(length(p / (r * r)), 1e-4);
    return k0 * (k0 - 1.0) / k1;
}

// `v` turned `a` radians anticlockwise.
fn turn(v: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(v.x * c - v.y * s, v.x * s + v.y * c);
}

// How far `p` is from the cat's tail, which runs through `t`, thinning to the tip.
fn sd_tail(p: vec2<f32>, t: array<vec2<f32>, 5>) -> f32 {
    var d = 1e3;
    for (var k = 0; k < 4; k++) {
        let r = mix(1.9, 1.2, f32(k) / 3.0);
        d = min(d, sd_segment(p, t[k], t[k + 1]) - r);
    }
    return d;
}

fn cat(under: vec4<f32>, q: vec2<f32>) -> vec4<f32> {
    var colour = under;
    let up = smoothstep(0.0, 1.0, sill.cat[0].x);
    let reach = sill.cat[0].y;
    let stride = sill.cat[0].z;
    let breath = sill.cat[0].w;
    let flick = radians(sill.cat[1].y);
    let eyes = sill.cat[1].z;
    let pet = sill.cat[2].x;
    let held = sill.cat[2].y;
    let fur = sill.fur.rgb;
    let mark = sill.mark.rgb;

    // Facing its way. Distances shrink with the squeeze, so edges stay soft
    // rather than doubling up.
    let f = select(-1.0, 1.0, sill.cat[1].w >= 0.0) * max(abs(sill.cat[1].w), 0.2);
    let p = vec2<f32>(q.x / f, q.y);
    let squeeze = abs(f);

    // Its body, curled low and long or carried high on its legs. A stretch
    // tips it forward, chest down and rump up. It swells as it breathes.
    let tilt = -0.22 * reach;
    let centre = vec2<f32>(mix(-2.0, -1.0, up), mix(7.4, 12.5, up) - 2.0 * reach);
    let r = vec2<f32>(
        mix(12.8, 11.5, up) + 1.5 * held,
        mix(7.4, 5.0, up) + 0.4 * breath * (1.0 - 0.6 * up),
    );
    let b = turn(p - centre, -tilt);
    let body = sd_ellipse(b, r);

    // Its head: tucked against its front asleep, lifted a little when petted,
    // held up when it stands, and low and forward in a stretch.
    let head_at = vec2<f32>(
        mix(9.6, 12.5, up) + 3.0 * reach,
        mix(7.6, 18.5, up) - 8.0 * reach + 1.2 * pet * (1.0 - up),
    );
    let hr = mix(5.6, 4.8, up);
    let skull = length(p - head_at) - hr;
    let snout = head_at + vec2<f32>(hr * 0.72, -hr * 0.38);
    let muzzle = length(p - snout) - hr * 0.42;
    let shoulder = centre + turn(vec2<f32>(r.x * 0.55, r.y * 0.45), tilt);
    let neck = sd_segment(p, shoulder, head_at - vec2<f32>(1.0, 1.0)) - mix(2.5, 3.0, up);
    let head = smin(skull, muzzle, 1.6);

    // Four legs, drawn up out of sight when it lies down. They swing in
    // diagonal pairs as it walks; in a stretch the front paws reach forward.
    let out = smoothstep(0.15, 0.9, sill.cat[0].x);
    var hips = array<f32, 4>(6.5, 4.5, -6.5, -8.5);
    var phases = array<f32, 4>(0.0, 0.5, 0.5, 0.0);
    var near_legs = 1e3;
    var far_legs = 1e3;
    for (var k = 0; k < 4; k++) {
        let front = k < 2;
        let hip = centre + turn(vec2<f32>(hips[k] + 1.0, -r.y * 0.3), tilt);
        let swing = 0.38 * out * sin(6.2831853 * (stride + phases[k]));
        let foot_x = hip.x + sin(swing) * hip.y + select(0.0, 7.0 * reach, front);
        let foot_at = vec2<f32>(foot_x, mix(hip.y, 0.9, out));
        let leg = sd_segment(p, hip, foot_at) - select(1.9, 1.6, front);
        if k % 2 == 0 {
            near_legs = min(near_legs, leg);
        } else {
            far_legs = min(far_legs, leg);
        }
    }

    // Its tail, from the rump: wrapped round its feet asleep, held high with
    // a crook at the tip when it stands. The tip keeps time with the beat, and
    // curls up a little more when it is petted.
    let base = centre + turn(vec2<f32>(-r.x * 0.92, mix(-0.3, 0.4, up) * r.y), tilt);
    var curled = array<vec2<f32>, 5>(
        vec2<f32>(0.0, 0.0), vec2<f32>(6.5, -2.4), vec2<f32>(14.5, -2.7),
        vec2<f32>(21.5, -2.2), vec2<f32>(25.5, -0.8 + 1.2 * pet),
    );
    var high = array<vec2<f32>, 5>(
        vec2<f32>(0.0, 0.0), vec2<f32>(-3.5, 2.5), vec2<f32>(-5.5, 7.0),
        vec2<f32>(-5.0, 11.5), vec2<f32>(-2.8, 13.5),
    );
    var tail: array<vec2<f32>, 5>;
    let beat = radians(sill.cat[1].x);
    for (var k = 0; k < 5; k++) {
        tail[k] = base + mix(curled[k], high[k], up);
        if k >= 3 {
            tail[k] = tail[2] + turn(tail[k] - tail[2], beat);
        }
    }
    let tail_d = sd_tail(p, tail) * squeeze;
    let tail_rgb = mix(fur, mark, 0.45) * mix(0.8, 1.0, smoothstep(0.0, 1.0, -tail_d));

    // Lit like the plant's pot: rounded by the room's light and edged by the
    // sun behind, a little darker underneath where the light doesn't reach.
    let across = (q.x - centre.x * f) / (r.x * squeeze + 1.0);
    let lit = form(across) * mix(0.72, 1.0, smoothstep(centre.y - r.y, centre.y + r.y * 0.3, p.y));

    // Behind it all: the far legs and the far ear, in shade.
    let far = far_legs * squeeze;
    colour = over(colour, fur * lit * 0.68, cover(far));
    let ear_lie = mix(-0.55, -0.12, up);
    let far_ear_at = head_at + vec2<f32>(-hr * 0.45, hr * 0.72);
    let far_ear = sd_trapezoid(turn(p - far_ear_at, -ear_lie - 0.25) - vec2<f32>(0.0, 2.0), 2.3, 0.25, 2.3) * squeeze;
    colour = over(colour, fur * lit * 0.7, cover(far_ear));

    // Standing, its tail is behind it.
    if up >= 0.5 {
        colour = over(colour, tail_rgb * lit, cover(tail_d));
    }

    // The body, neck and near legs as one, with tabby stripes across its back.
    // A slightly darker rim keeps its outline clear against the desktop.
    let torso = smin(smin(body, neck, 2.5), near_legs, 1.0) * squeeze;
    let band = smoothstep(-0.2 * r.y, 0.7 * r.y, b.y) * (1.0 - smoothstep(0.3 * r.x, 0.6 * r.x, b.x));
    let stripe = smoothstep(0.25, 0.85, sin(b.x * 0.95 + 0.6)) * band;
    var coat = mix(fur, mark, stripe * 0.6);
    coat *= mix(0.82, 1.0, smoothstep(0.0, 1.2, -torso));
    colour = over(colour, coat * lit, cover(torso));

    // Its head over the body, outlined the same way so it reads as a head,
    // with two short stripes on the brow and a paler muzzle.
    let head_d = head * squeeze;
    let h = p - head_at;
    let brow = smoothstep(0.2, 0.8, sin(h.x * 2.2 + 1.2)) * smoothstep(hr * 0.2, hr * 0.75, h.y) * (1.0 - smoothstep(0.0, hr * 0.6, abs(h.x)));
    var face = mix(fur, mark, brow * 0.5);
    face = mix(face, fur * 1.2, (1.0 - smoothstep(-0.6, 0.6, muzzle)) * 0.6);
    face *= mix(0.8, 1.0, smoothstep(0.0, 1.0, -head_d));
    let face_lit = form((q.x - head_at.x * f) / (hr * squeeze + 1.0)) * mix(0.85, 1.0, smoothstep(-hr, hr * 0.3, h.y));
    colour = over(colour, face * face_lit, cover(head_d));

    // The near ear, with a warmer inside. A kick flicks it back.
    let ear_at = head_at + vec2<f32>(hr * 0.15, hr * 0.78);
    let ear_q = turn(p - ear_at, -ear_lie - flick) - vec2<f32>(0.0, 2.0);
    let ear = sd_trapezoid(ear_q, 2.3, 0.25, 2.3) * squeeze;
    colour = over(colour, fur * face_lit, cover(ear));
    let inner = sd_trapezoid(ear_q - vec2<f32>(0.2, -0.4), 1.2, 0.1, 1.5) * squeeze;
    colour = over(colour, mix(fur, vec3<f32>(0.85, 0.55, 0.52), 0.45) * face_lit, cover(inner));

    // Asleep, its tail lies round its feet in front of it.
    if up < 0.5 {
        colour = over(colour, tail_rgb * lit, cover(tail_d));
    }

    // Its eye: a closed curve asleep. Open, amber with a slit for a pupil,
    // and a glint that the candle's light makes brighter.
    let eye_at = head_at + vec2<f32>(hr * 0.38, hr * 0.12);
    let e = p - eye_at;
    let shut = max(abs(length(e - vec2<f32>(0.0, 1.4)) - 1.6) - 0.28, e.y - 0.3) * squeeze;
    colour = over(colour, mark * 0.6, cover(shut) * (1.0 - smoothstep(0.08, 0.3, eyes)));
    if eyes > 0.02 {
        let open = sd_ellipse(e, vec2<f32>(1.25, max(eyes, 0.05))) * squeeze;
        colour = over(colour, vec3<f32>(0.86, 0.68, 0.25), cover(open));
        let pupil = sd_ellipse(e - vec2<f32>(0.2, 0.0), vec2<f32>(0.35, max(eyes * 0.9, 0.04))) * squeeze;
        colour = over(colour, vec3<f32>(0.06, 0.05, 0.04), cover(pupil));
        let glint = (length(e - vec2<f32>(0.45, 0.3 * eyes)) - 0.3) * squeeze;
        colour = over(colour, vec3<f32>(1.0), cover(glint) * eyes * (0.45 + 0.4 * sill.candle.x));
    }

    // A nose at the end of the muzzle.
    let nose = (length(p - snout - vec2<f32>(hr * 0.36, hr * 0.1)) - 0.6) * squeeze;
    return over(colour, vec3<f32>(0.55, 0.36, 0.36), cover(nose));
}

// ---- The dust -------------------------------------------------------------
//
// While music plays, a few dozen motes drift in the light around the things.
// Each lives in a cell of a grid that rises slowly through the air, wandering
// about the middle of its cell, and turns as it goes so it catches the light
// and loses it. How far the grid has risen, how far each mote has wandered and
// how far it has turned are all whole cycles of `sill.music.y`, and the grid
// repeats after DUST_TILE cells, so the dust comes round to where it began
// without a jump. In silence `sill.music.y` stands still and the motes fade
// where they are.

const DUST_CELL: f32 = 28.0;
const DUST_TILE: vec2<f32> = vec2<f32>(256.0, 128.0);

// How much of a mote covers `p`, from 0 to 1, and how much light it catches.
fn motes(p: vec2<f32>) -> f32 {
    let cycle = sill.music.y;
    let q = p - vec2<f32>(0.0, cycle * DUST_TILE.y * DUST_CELL);
    let cell = floor(q / DUST_CELL);
    let id = cell - DUST_TILE * floor(cell / DUST_TILE);
    // Not every cell has one.
    if hash(id) > 0.6 {
        return 0.0;
    }
    let a = hash(id + vec2<f32>(31.7, 11.3));
    let b = hash(id + vec2<f32>(5.1, 47.9));
    let c = hash(id + vec2<f32>(73.4, 2.6));
    // Each wanders on its own slow path, and never out of its cell.
    let turns = floor(vec2<f32>(28.0, 34.0) + vec2<f32>(a, b) * 30.0);
    let wander = sin(6.2831853 * (turns * cycle + vec2<f32>(b, c))) * 5.5;
    let centre = (cell + 0.3 + 0.4 * vec2<f32>(a, b)) * DUST_CELL + wander;
    // A speck no finer than a physical pixel, so it shows at any DPI.
    let r = max(0.6 + 0.7 * c, 0.8 / sill.scale);
    let d = length(q - centre) / r;
    let turn = 0.55 + 0.45 * sin(6.2831853 * (floor(14.0 + c * 16.0) * cycle + a));
    return exp(-d * d) * turn;
}

// The dust in the air behind the things, where light falls on it: the sun's,
// in a soft pool about each thing it lights, and the candle's close round its
// flame.
fn drifting(colour: vec4<f32>, p: vec2<f32>, x: vec4<f32>) -> vec4<f32> {
    let stir = sill.music.x;
    if stir < 0.002 {
        return colour;
    }
    let around = vec3<f32>(
        length(p - vec2<f32>(x.x, 40.0)),
        length(p - vec2<f32>(x.y, 30.0)),
        length(p - vec2<f32>(x.z, 30.0)),
    );
    let pool = exp(-around * around / (2.0 * 70.0 * 70.0));
    let beam = shine() * max(max(pool.x, pool.y), pool.z) * 0.5;
    let near = length(p - flame_at(x));
    let by_candle = sill.candle.x * 0.8 * exp(-near * near / (2.0 * 34.0 * 34.0));
    let light = beam + by_candle;
    if light < 0.004 {
        return colour;
    }
    let rgb = (graded(sunlight()) * beam + sill.flame.rgb * by_candle) / light;
    let a = motes(p) * min(light * 3.5, 1.0) * smoothstep(0.0, 0.35, stir) * 0.95;
    return over(colour, rgb, a);
}

// ---- The whole sill -------------------------------------------------------

// Lays thing `i` (0 the plant, 1 the jar, 2 the candle, 3 the cat) over
// `colour`, standing at `x`, lifted, tipped and trailing as the hand has it.
fn thing(colour: vec4<f32>, i: i32, p: vec2<f32>, x: vec4<f32>) -> vec4<f32> {
    let foot_at = vec2<f32>(x[i], sill.lift[i]);
    let q = upright(p - foot_at, sill.tip[i]);
    let empty = vec4<f32>(0.0);
    switch i {
        case 0 {
            if abs(p.x - x.x) < 48.0 {
                let drawn = plant(empty, q);
                let layer = candlelight(vec4<f32>(graded(drawn.rgb), drawn.a), p, x);
                return under_layer(colour, layer);
            }
        }
        case 1 {
            if abs(p.x - x.y) < 20.0 {
                let drawn = jar(empty, q, sill.tip.y, sill.swing.y);
                let layer = candlelight(vec4<f32>(graded(drawn.rgb), drawn.a), p, x);
                return under_layer(colour, layer);
            }
        }
        case 2 {
            // The candle grades its own wax, and its flame makes its own light.
            if abs(p.x - x.z) < 60.0 {
                return under_layer(colour, candle(empty, q, sill.tip.z, sill.swing.z));
            }
        }
        default {
            if abs(p.x - x.w) < 40.0 {
                let drawn = cat(empty, q);
                let layer = candlelight(vec4<f32>(graded(drawn.rgb), drawn.a), p, x);
                return under_layer(colour, layer);
            }
        }
    }
    return colour;
}

@fragment
fn fs_main(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    // Logical pixels across from the left and up from the bottom edge.
    let up = sill.height - pixel.y;
    let p = vec2<f32>(pixel.x, up) / sill.scale;
    // Each thing stands on a whole physical pixel, so it draws the same way
    // wherever it is put.
    let x = floor(sill.places) / sill.scale;

    // Most of the strip is nowhere near anything, and stays clear.
    let gap = min(min(abs(p.x - x.x), abs(p.x - x.y)), min(abs(p.x - x.z), abs(p.x - x.w)));
    if gap > NEAR {
        return vec4<f32>(0.0);
    }

    var colour = drifting(vec4<f32>(0.0), p, x);
    colour = foot(colour, p, x.x, 12.0, sill.lift.x);
    colour = foot(colour, p, x.y, 11.0, sill.lift.y);
    colour = foot(colour, p, x.z, 7.5, sill.lift.z);
    colour = foot(colour, p, x.w, mix(14.0, 11.0, sill.cat[0].x), sill.lift.w);

    // The one last picked up in front, the others in their usual order.
    let front = i32(sill.order.x);
    for (var i = 0; i < 4; i++) {
        if i != front {
            colour = thing(colour, i, p, x);
        }
    }
    colour = thing(colour, front, p, x);

    colour *= sill.presence;
    if colour.a <= 0.0 {
        return vec4<f32>(0.0);
    }
    // Dither colour and alpha together so the colour stays premultiplied.
    let n = dither(pixel.xy) / 255.0;
    let a = clamp(colour.a + n, 0.0, 1.0);
    return vec4<f32>(clamp(colour.rgb + vec3<f32>(n), vec3<f32>(0.0), vec3<f32>(a)), a);
}
