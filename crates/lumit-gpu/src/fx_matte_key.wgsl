// Matte key (docs/08-EFFECTS.md §3.21): a soft chroma key (greenscreen
// removal). Mirrors lumit_core::fx::cpu::matte_key op-for-op (§1.6: the CPU is
// the oracle). Straight (unpremultiplied) colour (§2.2, the wrap fused into
// the kernel): unpremultiply -> key + despill -> re-premultiply.
//
// The metric is Euclidean distance in the chroma plane (RGB minus Rec. 709
// luma), so greens of any brightness sit at the same point and key alike. The
// keep-factor is a smoothstep, written out explicitly to match the CPU maths:
// 0 (fully keyed, alpha *= 0) at chroma distance <= tol, 1 (fully kept) at
// >= tol + soft, smooth between -- no hard step, so the effect is continuous
// everywhere. Spill suppression pulls the residual key-hue projection out of
// the kept colour. Mix 0 is the bit-exact identity.

struct Params {
    key: vec4<f32>,   // scene-linear key colour; alpha ignored
    tol: f32,         // 0..1 chroma-distance threshold (fully keyed at/below)
    soft: f32,        // 0..1 soft-edge width above tol
    spill: f32,       // 0..1 key-hue spill removal
    mix_amt: f32,     // 0..1, blended against the unprocessed input
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var orig: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var<uniform> p: Params;

// Rec. 709 luma weights, in linear light (== cpu::LUMA).
const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);

// The unpremultiplied colour of a premultiplied pixel (== cpu::unpremult).
fn unpremult(c: vec4<f32>) -> vec3<f32> {
    if (c.a > 0.0) {
        return c.rgb / c.a;
    }
    return vec3<f32>(0.0);
}

@compute @workgroup_size(8, 8)
fn matte_key(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = vec2<i32>(textureDimensions(src));
    let xy = vec2<i32>(gid.xy);
    if (xy.x >= size.x || xy.y >= size.y) {
        return;
    }
    let o = textureLoad(src, xy, 0);
    let a = o.a;
    let u = unpremult(o);

    // Key chroma (a pure-chroma vector: its own luma is zero) and unit hue
    // direction; a grey key has no hue, so its direction is zero (spill off).
    let kl = p.key.r * LUMA.r + p.key.g * LUMA.g + p.key.b * LUMA.b;
    let kc = p.key.rgb - vec3<f32>(kl);
    let klen = sqrt(kc.x * kc.x + kc.y * kc.y + kc.z * kc.z);
    var kdir = vec3<f32>(0.0);
    if (klen > 1e-6) {
        kdir = kc / klen;
    }

    // Pixel chroma and its distance from the key's -> smoothstep keep-factor.
    let pl = u.r * LUMA.r + u.g * LUMA.g + u.b * LUMA.b;
    let pc = u - vec3<f32>(pl);
    let dc = pc - kc;
    let d = sqrt(dc.x * dc.x + dc.y * dc.y + dc.z * dc.z);
    let e1 = p.tol + max(p.soft, 1e-6);
    let t = clamp((d - p.tol) / (e1 - p.tol), 0.0, 1.0);
    let keep = t * t * (3.0 - 2.0 * t);

    // Spill: remove the key-hue projection from the kept colour.
    let proj = max(pc.x * kdir.x + pc.y * kdir.y + pc.z * kdir.z, 0.0) * p.spill;
    let despilled = u - proj * kdir;

    let out_a = a * keep;
    let proc = despilled * out_a;
    let outv = o.rgb * (1.0 - p.mix_amt) + proc * p.mix_amt;
    let outa = a * (1.0 - p.mix_amt) + out_a * p.mix_amt;
    textureStore(dst, xy, vec4<f32>(outv, outa));
}
