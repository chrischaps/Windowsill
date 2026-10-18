// The sill: a wooden ledge along the bottom of the strip with a potted plant,
// a small glass jar and a candle standing on it, and above them a faint warmth
// on the glass that fades to nothing before the top edge, so the desktop shows
// through. Every shape is a signed distance in logical pixels, so the sill is
// equally crisp at any DPI. Colours leave here premultiplied.

struct Sill {
    // The strip's height in physical pixels.
    height: f32,
    // Physical pixels per logical pixel.
    scale: f32,
    // 0 while the sill is away, easing to 1 as it settles in.
    presence: f32,
    // How strong the warmth on the glass starts, just above the ledge.
    glow_alpha: f32,
    // The palette, from windowsill.toml. Only .rgb is used.
    wood: vec4<f32>,
    wood_shade: vec4<f32>,
    lip_light: vec4<f32>,
    glow: vec4<f32>,
    grain: vec4<f32>,
    // The eased sill state: beam angle in degrees, colour temperature in
    // kelvin, shadow side from -1 to 1, and moonlight from 0 to 1 (not drawn yet).
    light: vec4<f32>,
    // How much of the sun the room's window lets in, from 0 to 1, and which
    // side its light comes from, -1 left, 0 ahead, 1 right.
    room: vec4<f32>,
    // The colour of the room's light at this hour, and in .w how much of their
    // own colour things keep in it.
    sky: vec4<f32>,
    // The candle's flame in .x, from 0 to 1.
    candle: vec4<f32>,
    // Where the plant, jar and candle stand, from 0 to 1 across, and in .w the
    // strip's width in physical pixels.
    places: vec4<f32>,
    // The things on the sill, from windowsill.toml. Only .rgb is used.
    pot: vec4<f32>,
    leaf: vec4<f32>,
    glass: vec4<f32>,
    water: vec4<f32>,
    wax: vec4<f32>,
    flame: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sill: Sill;

// Heights in logical pixels up from the bottom edge. The board shows its front
// face, then a narrow strip of its top running back to the glass.
const FACE: f32 = 22.0;
const LEDGE: f32 = 30.0;
const LIP: f32 = 1.5;
// Where things stand: partway back on the board's top.
const BASE: f32 = 25.0;
// How far apart the window's glazing bars are, and how wide each one is.
const PANE: f32 = 380.0;
const BAR: f32 = 6.0;
// Which side the room's own light comes from, -1 left to 1 right, when the
// window is straight ahead. It models the things on the sill whatever the sun
// is doing.
const ROOM: f32 = -0.45;
// The sky's light through the window when the sun isn't coming in: soft and
// a little cool.
const SKY: vec3<f32> = vec3<f32>(0.86, 0.93, 1.0);

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
// The sun is outside, beyond the glass, so it reaches the sill from behind the
// things on it. It lights the board's top, catches their edges on its side,
// glows through the leaves and the water, and throws their shadows forward and
// away from itself. The window's glazing bars stripe the board, and between
// them the light shows faintly in the air.
//
// The sun only does that while the room's window lets it in. The rest of the
// day the sky lights the board instead, softly and from the window's side.

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

// How much of the sky's light stands in for the sun while it isn't coming in.
fn sky() -> f32 {
    return smoothstep(-6.0, 4.0, sill.light.x) * (1.0 - sill.room.x);
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

// How far the light travels sideways for each pixel it falls: nothing at noon,
// a long way at dawn and dusk. It has the sign of the shadow side.
fn lean() -> f32 {
    let e = radians(clamp(sill.light.x, 3.0, 90.0));
    return sill.light.z * min(cos(e) / sin(e), 4.0);
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

// How much of the sun a glazing bar keeps off the point `x` across and `back`
// logical px in from the glass, the panes lined up on `pane`, from 0 to 1. Its
// edge is `soft` px wide.
fn bars(x: f32, back: f32, pane: f32, soft: f32) -> f32 {
    // The bar's shadow runs forward from the glass, slanting away from the sun.
    let along = x - pane - PANE * 0.5 - sill.light.z * back * 3.0;
    let off = abs(along - PANE * round(along / PANE));
    return 1.0 - smoothstep(BAR * 0.5 - soft * 0.5, BAR * 0.5 + soft, off);
}

// How much of the light something standing at `x`, `half_width` across and
// `height` tall, keeps off the board's top at `p`. It falls forward, toward the
// room, and sideways by `lean` for each pixel of height: longer and softer the
// lower the light.
fn shadow(p: vec2<f32>, x: f32, half_width: f32, height: f32, lean: f32) -> f32 {
    let reach = lean * height * 0.5;
    let centre = vec2<f32>(x + reach * 0.5, BASE - 1.2);
    let d = sd_box(p - centre, vec2<f32>(half_width * 0.85 + abs(reach) * 0.5, 2.4), 2.0);
    let soft = 1.2 + min(abs(reach) * 0.03, 2.0);
    // Darkest at the foot, thinning toward the tip.
    let out = clamp((p.x - x) * sign(reach) / (abs(reach) + half_width), 0.0, 1.0);
    return (1.0 - smoothstep(-soft, soft, d)) * mix(1.0, 0.6, out);
}

// Something lit by the room at this hour (Fishbowl's palette): it takes on the
// colour of the light, and loses some of its own as the light fails. Light
// that makes itself, like the candle's flame, is left alone.
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
    return vec3<f32>(fill) + sunlight() * edge * shine() * 0.4;
}

// A hash without sine (Dave Hoskins), steady at the large x of a wide screen.
fn hash(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let bottom = mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x);
    let top = mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x);
    return mix(bottom, top, u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 3; i++) {
        sum += amp * noise(q);
        q = q * 2.03 + vec2<f32>(17.0, 5.0);
        amp *= 0.5;
    }
    return sum;
}

// How much darker grain shows at `p`, from 0 to 1. The lines run along the
// board and wander a little, the way sawn timber does.
fn grain(p: vec2<f32>) -> f32 {
    let wander = fbm(vec2<f32>(p.x * 0.006, p.y * 0.09)) * 10.0;
    let lines = 0.5 + 0.5 * sin(p.y * 1.1 + wander);
    let fibre = fbm(vec2<f32>(p.x * 0.05, p.y * 1.3));
    return clamp(smoothstep(0.55, 1.0, lines) * 0.7 + fibre * 0.4 - 0.12, 0.0, 1.0);
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

// ---- The ledge ------------------------------------------------------------

// The ledge, with the things standing at `x` (plant, jar, candle) shading it.
fn ledge(under: vec4<f32>, p: vec2<f32>, x: vec3<f32>) -> vec4<f32> {
    var colour = under;

    // The top of the board, lit from above: lightest at its front edge, falling
    // toward the glass. Its grain is foreshortened.
    let back = clamp((p.y - FACE) / (LEDGE - FACE), 0.0, 1.0);
    var top = mix(mix(sill.lip_light.rgb, sill.wood.rgb, 0.35), sill.wood.rgb * 0.92, back);
    top = mix(top, sill.grain.rgb, grain(vec2<f32>(p.x, p.y * 3.0 + 40.0)) * 0.3);

    // The sun on it, striped by the glazing bars and shaded by what stands there.
    // The panes line up on the jar, so it always stands in the light.
    var shade = bars(p.x, LEDGE - p.y, x.y, 1.5 + (LEDGE - p.y) * 0.4);
    shade = max(shade, shadow(p, x.x, 12.0, 46.0, lean()) * 0.9);
    shade = max(shade, shadow(p, x.y, 11.0, 33.0, lean()) * 0.4);
    shade = max(shade, shadow(p, x.z, 7.0, 24.0, lean()) * 0.8);
    // The sky's light casts only faint, short shadows away from the window.
    let away = -sill.room.y * 0.6;
    var sky_shade = shadow(p, x.x, 12.0, 46.0, away) * 0.45;
    sky_shade = max(sky_shade, shadow(p, x.y, 11.0, 33.0, away) * 0.25);
    sky_shade = max(sky_shade, shadow(p, x.z, 7.0, 24.0, away) * 0.4);
    top *= 0.6 + sunlight() * shine() * 0.72 * (1.0 - shade) + SKY * sky() * 0.36 * (1.0 - sky_shade);
    colour = over(colour, top, cover(p.y - LEDGE));

    // The front face, a touch darker toward the bottom, with a lighter lip
    // where it turns into the top.
    let depth = clamp(p.y / FACE, 0.0, 1.0);
    var face = mix(sill.wood_shade.rgb, sill.wood.rgb, smoothstep(0.0, 0.7, depth));
    face = mix(face, sill.grain.rgb, grain(p) * 0.45);
    face = mix(face, sill.lip_light.rgb, smoothstep(FACE - LIP, FACE - 0.3, p.y));
    return over(colour, face, cover(p.y - FACE));
}

// A soft dark pool on the board's top where something stands on it.
fn contact(colour: vec4<f32>, p: vec2<f32>, x: f32, half_width: f32) -> vec4<f32> {
    let e = length(vec2<f32>((p.x - x) / (half_width * 1.2), (p.y - BASE) / 2.8));
    let shade = (1.0 - smoothstep(0.35, 1.0, e)) * 0.42 * step(p.y, LEDGE);
    return vec4<f32>(colour.rgb * (1.0 - shade), colour.a);
}

// Sunlight gathered by the jar's water into a small bright patch on the wood,
// just in front of it. Only when the sun is high enough to come down through it.
fn caustic(colour: vec4<f32>, p: vec2<f32>, x: f32) -> vec4<f32> {
    let high = smoothstep(22.0, 48.0, sill.light.x) * shine();
    let on_top = step(FACE, p.y) * step(p.y, LEDGE);
    if high * on_top <= 0.0 {
        return colour;
    }
    let centre = vec2<f32>(x + lean() * 33.0 * 0.3, BASE - 1.6);
    let q = (p - centre) / vec2<f32>(10.0, 2.1);
    let r = length(q);
    // A soft pool with a brighter rim, and a few fine threads through it, as
    // a curved glass of water draws them.
    let pool = exp(-r * r * 2.2);
    let rim = exp(-pow((r - 0.8) / 0.16, 2.0)) * smoothstep(-0.4, 0.6, q.y + 0.3);
    let threads = smoothstep(0.5, 0.85, noise(vec2<f32>(p.x * 0.45, p.y * 0.3) + 7.0)) * pool;
    let light = (pool * 0.5 + rim * 0.7 + threads * 0.4) * high * on_top;
    let tint = mix(sunlight(), sill.water.rgb * 1.6, 0.2);
    return vec4<f32>(colour.rgb + tint * light * 0.8 * colour.a, colour.a);
}

// ---- The plant ------------------------------------------------------------

fn plant(under: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    var colour = under;
    let root = vec2<f32>(0.0, BASE + 21.0);
    let dark = sill.leaf.rgb * 0.5;

    // Seven leaves fanned from the pot, the outer ones behind and in shade,
    // each drooping a little more the further out it leans.
    var angles = array<f32, 7>(-64.0, -40.0, -18.0, 2.0, 21.0, 42.0, 63.0);
    var lengths = array<f32, 7>(19.0, 26.0, 31.0, 34.0, 30.0, 25.0, 18.0);
    var stems = array<f32, 7>(5.0, 8.0, 11.0, 13.0, 10.0, 8.0, 5.0);
    var order = array<i32, 7>(0, 6, 1, 5, 2, 4, 3);
    var depth = array<f32, 7>(0.74, 0.74, 0.86, 0.86, 0.95, 0.95, 1.0);
    for (var k = 0; k < 7; k++) {
        let i = order[k];
        let angle = radians(angles[i]);
        let len = lengths[i];
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

fn jar(under: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    var colour = under;
    let body = sd_box(p - vec2<f32>(0.0, BASE + 12.5), vec2<f32>(11.0, 12.5), 5.0);
    let neck = sd_box(p - vec2<f32>(0.0, BASE + 26.5), vec2<f32>(7.5, 2.0), 0.8);
    let glass = smin(body, neck, 3.0);
    let glass_rgb = sill.glass.rgb;

    // Faintly tinted through the middle.
    colour = over(colour, glass_rgb, cover(glass) * 0.08);

    // Water a little over half way, denser where the glass curves away, with a
    // bright line where it meets the air.
    let level = BASE + 15.0;
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

    // A long highlight down the lit side, and a small one on the shoulder.
    let side = room_hand();
    let streak = sd_segment(p, vec2<f32>(side * 6.8, BASE + 6.0), vec2<f32>(side * 6.8, BASE + 19.0)) - 0.75;
    colour = over(colour, mix(glass_rgb, vec3<f32>(1.0), 0.6), cover(streak) * 0.55);
    let glint = length(p - vec2<f32>(side * 4.0, BASE + 23.2)) - 0.9;
    colour = over(colour, vec3<f32>(1.0), cover(glint) * 0.5);

    // A wooden lid, cut from the same board as the ledge.
    let lid = sd_box(p - vec2<f32>(0.0, BASE + 30.5), vec2<f32>(9.0, 2.5), 1.2);
    var wood = mix(sill.wood.rgb, sill.lip_light.rgb, 0.5) * form(p.x / 9.0);
    wood = mix(wood, sill.lip_light.rgb * 1.1, smoothstep(BASE + 31.6, BASE + 32.8, p.y));
    return over(colour, wood, cover(lid));
}

// ---- The candle -----------------------------------------------------------

fn candle(under: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    var colour = under;
    let lit = sill.candle.x;
    let top = BASE + 24.0;

    // A pillar of wax with one drip down its side.
    let pillar = sd_box(p - vec2<f32>(0.0, BASE + 12.0), vec2<f32>(7.0, 12.0), 1.2);
    let drip = sd_segment(p, vec2<f32>(6.6, top - 1.0), vec2<f32>(6.7, top - 8.5)) - 1.1;
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
    // touch of blue at its foot.
    let size = smoothstep(0.0, 1.0, lit);
    let r = 2.7 * size + 0.01;
    let height = 10.0 * size + 0.02;
    let q = p - (wick_top + vec2<f32>(0.0, r * 0.4));
    let flame = sd_teardrop(q, r, 0.3 * size + 0.005, height - r);
    let core = sd_teardrop(q - vec2<f32>(0.0, -r * 0.2), r * 0.55, 0.1, (height - r) * 0.55);
    var fire = sill.flame.rgb;
    fire = mix(fire, vec3<f32>(0.42, 0.52, 0.95), (1.0 - smoothstep(-r, 0.0, q.y)) * 0.35);
    fire = mix(fire, vec3<f32>(1.0, 0.96, 0.86), cover(core));
    colour = over(colour, fire, cover(flame) * smoothstep(0.02, 0.3, lit));

    // The warmth it throws around itself, onto the glass and the wood alike.
    let reach = length(q - vec2<f32>(0.0, height * 0.3));
    let warmth = lit * 0.28 * exp(-reach * reach / (2.0 * 15.0 * 15.0));
    return vec4<f32>(colour.rgb + sill.flame.rgb * warmth, colour.a + warmth * (1.0 - colour.a));
}

// ---- The whole sill -------------------------------------------------------

@fragment
fn fs_main(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    // Logical pixels across from the left and up from the bottom edge.
    let up = sill.height - pixel.y;
    let p = vec2<f32>(pixel.x, up) / sill.scale;
    let ledge_top = LEDGE * sill.scale;

    // The glass above: warmth that falls away with a soft tail and is gone by the top.
    let fade_span = max(sill.height - ledge_top, 1.0);
    let t = clamp((up - ledge_top) / fade_span, 0.0, 1.0);
    let glow_alpha = sill.glow_alpha * pow(1.0 - t, 2.4);
    // The warmth takes on a little of the sun's colour while it shines.
    let warmth = mix(sill.glow.rgb, sill.glow.rgb * sunlight() * 1.15, shine() * 0.5);
    var colour = vec4<f32>(warmth * glow_alpha, glow_alpha);

    // Each thing stands on a whole physical pixel, so it draws the same way
    // wherever it is put.
    let x = floor(sill.places.xyz * sill.places.w) / sill.scale;

    // The light in the air between the glazing bars, leaning toward the sun as
    // it rises from the board. It reaches higher when the sun is low, and the
    // dust in it is never quite even.
    let above = max(p.y - LEDGE, 0.0);
    let reach = mix(1.0, 0.45, sin(radians(clamp(sill.light.x, 0.0, 90.0))));
    let rise = pow(1.0 - clamp(t / reach, 0.0, 1.0), 1.6);
    let dust = 0.6 + 0.4 * noise(vec2<f32>(p.x * 0.012, p.y * 0.03));
    // Light scatters in the air, so the bars blur as the shafts rise.
    let between = 1.0 - 0.7 * bars(p.x + lean() * above * 0.6, 0.0, x.y, 8.0 + above * 0.5);
    let shaft = shine() * between * rise * dust * 0.13 * step(LEDGE, p.y);
    colour = over(colour, sunlight(), shaft);

    colour = ledge(colour, p, x);
    colour = contact(colour, p, x.x, 12.0);
    colour = contact(colour, p, x.y, 11.0);
    colour = contact(colour, p, x.z, 7.5);
    colour = caustic(colour, p, x.y);
    // Only work out the shapes near where they stand.
    if abs(p.x - x.x) < 48.0 {
        colour = plant(colour, p - vec2<f32>(x.x, 0.0));
    }
    if abs(p.x - x.y) < 16.0 {
        colour = jar(colour, p - vec2<f32>(x.y, 0.0));
    }
    // All of it so far is lit by the room. The candle grades its own wax.
    colour = vec4<f32>(graded(colour.rgb), colour.a);
    if abs(p.x - x.z) < 60.0 {
        colour = candle(colour, p - vec2<f32>(x.z, 0.0));
    }

    colour *= sill.presence;
    // Dither colour and alpha together so the colour stays premultiplied.
    let n = dither(pixel.xy) / 255.0;
    let a = clamp(colour.a + n, 0.0, 1.0);
    return vec4<f32>(clamp(colour.rgb + vec3<f32>(n), vec3<f32>(0.0), vec3<f32>(a)), a);
}
