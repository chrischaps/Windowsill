// The M1 pane: a flat wooden ledge along the bottom of the strip, and above it
// a faint warmth on the glass that fades to nothing before the top edge, so
// the desktop shows through. Colours leave here premultiplied.

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
    // The eased sill state, not drawn yet: beam angle in degrees, colour
    // temperature in kelvin, shadow side from -1 to 1, and moonlight from 0 to 1.
    light: vec4<f32>,
    // The candle's flame in .x, from 0 to 1.
    candle: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sill: Sill;

// Logical pixels, so the ledge is the same size at any DPI.
const LEDGE: f32 = 30.0;
const LIP: f32 = 1.5;

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

@fragment
fn fs_main(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    // Physical pixels up from the bottom edge, and the ledge's top in the same units.
    let up = sill.height - pixel.y;
    let ledge_top = LEDGE * sill.scale;
    let lip = LIP * sill.scale;

    // The glass above: warmth that falls away with a soft tail and is gone by the top.
    let fade_span = max(sill.height - ledge_top, 1.0);
    let t = clamp((up - ledge_top) / fade_span, 0.0, 1.0);
    let glow_alpha = sill.glow_alpha * pow(1.0 - t, 2.4);
    var colour = vec4<f32>(sill.glow.rgb * glow_alpha, glow_alpha);

    // The ledge: a flat face, a touch darker toward the bottom, with a lighter
    // lip where its top edge catches the light.
    let depth = clamp(up / ledge_top, 0.0, 1.0);
    var wood = mix(sill.wood_shade.rgb, sill.wood.rgb, smoothstep(0.0, 0.7, depth));
    let lip_amount = smoothstep(ledge_top - lip, ledge_top - 0.5, up);
    wood = mix(wood, sill.lip_light.rgb, lip_amount);
    // Coverage of the ledge's top edge within this pixel, for a clean line at any scale.
    let ledge = clamp(ledge_top - up + 0.5, 0.0, 1.0);
    colour = mix(colour, vec4<f32>(wood, 1.0), ledge);

    colour *= sill.presence;
    // Dither colour and alpha together so the colour stays premultiplied.
    let n = dither(pixel.xy) / 255.0;
    let a = clamp(colour.a + n, 0.0, 1.0);
    return vec4<f32>(clamp(colour.rgb + vec3<f32>(n), vec3<f32>(0.0), vec3<f32>(a)), a);
}
