// Datamosh (docs/08-EFFECTS.md §3.12, the Glitch effect's third section,
// K-104): re-warp the previous source frame along the flow measured from the
// current frame to it, blended against the current (already block/
// scanline'd) frame by Intensity. Mirrors lumit_core::fx::cpu::datamosh
// op-for-op (§1.6: the CPU is the oracle) — one bilinear tap per pixel, not
// a streak integral (this looks up a single displaced source pixel — a
// motion-compensated prediction — not a line integral of motion).
//
// binding 0 is the current frame the warp mixes against; binding 1 is the
// raw -1 neighbour source frame the flow warps; binding 2 is the dense
// current→previous flow field (the same convention fx_motionblur.wgsl's
// flow texture uses for its own +1 neighbour, just measured against -1
// instead). Reuses the shared three-sampled-input layout Motion blur's pass
// uses (modelled on it directly, including its bilinear helper, just
// sampling `prev` instead of `src`).

struct Params {
    intensity: f32, // 0..1, blended against the current frame
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var cur: texture_2d<f32>;
@group(0) @binding(1) var prev: texture_2d<f32>;
@group(0) @binding(2) var flow: texture_2d<f32>;
@group(0) @binding(3) var dst: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var<uniform> p: Params;

// Clamp-addressed bilinear at continuous pixel-centre coordinates (== the
// cpu::bilinear rule the reference uses, same arithmetic order): the texel at
// index x covers [x, x+1), centre x+0.5; out-of-frame taps read the edge.
fn bilinear_clamp(sx: f32, sy: f32, size: vec2<i32>) -> vec4<f32> {
    let fx = sx - 0.5;
    let fy = sy - 0.5;
    let x0 = floor(fx);
    let y0 = floor(fy);
    let tx = fx - x0;
    let ty = fy - y0;
    let x0i = i32(x0);
    let y0i = i32(y0);
    let c00 = textureLoad(prev, vec2<i32>(clamp(x0i, 0, size.x - 1), clamp(y0i, 0, size.y - 1)), 0);
    let c10 = textureLoad(prev, vec2<i32>(clamp(x0i + 1, 0, size.x - 1), clamp(y0i, 0, size.y - 1)), 0);
    let c01 = textureLoad(prev, vec2<i32>(clamp(x0i, 0, size.x - 1), clamp(y0i + 1, 0, size.y - 1)), 0);
    let c11 = textureLoad(prev, vec2<i32>(clamp(x0i + 1, 0, size.x - 1), clamp(y0i + 1, 0, size.y - 1)), 0);
    let top = c00 * (1.0 - tx) + c10 * tx;
    let bottom = c01 * (1.0 - tx) + c11 * tx;
    return top * (1.0 - ty) + bottom * ty;
}

@compute @workgroup_size(8, 8)
fn datamosh(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = vec2<i32>(textureDimensions(cur));
    let xy = vec2<i32>(gid.xy);
    if (xy.x >= size.x || xy.y >= size.y) {
        return;
    }
    let pos = vec2<f32>(xy) + vec2<f32>(0.5);
    let uv = textureLoad(flow, xy, 0).xy;
    let warped = bilinear_clamp(pos.x + uv.x, pos.y + uv.y, size);
    let c = textureLoad(cur, xy, 0);
    textureStore(dst, xy, mix(c, warped, p.intensity));
}
