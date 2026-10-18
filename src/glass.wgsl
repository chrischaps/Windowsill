// The spike's test pane: a warm band along the bottom that fades to nothing at
// the top, so the desktop shows through it. Colours leave here premultiplied.

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    // 0 at the top edge of the strip, 1 at the bottom.
    @location(0) down: f32,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    // One triangle that covers the whole strip.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.down = 1.0 - corner.y;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let wood = vec3<f32>(0.42, 0.27, 0.16);
    let alpha = smoothstep(0.0, 1.0, in.down);
    return vec4<f32>(wood * alpha, alpha);
}
