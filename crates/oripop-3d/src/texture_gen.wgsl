// Generative texture compute shader.
//
// Each invocation writes one texel to the output storage texture.
// The pattern is domain-warped FBM (fractional Brownian motion), colorised
// with Inigo Quilez's cosine-palette technique.
//
// Dispatch with workgroup_size(8, 8) — one workgroup per 8×8 texel tile.

struct GenParams {
    time:          f32,
    seed:          f32,
    frequency:     f32,
    octaves:       u32,
    warp_strength: f32,
    _pad0:         f32,
    _pad1:         f32,
    _pad2:         f32,
}

@group(0) @binding(0)
var output_tex: texture_storage_2d<rgba16float, write>;

@group(0) @binding(1)
var<uniform> params: GenParams;

// ── Hash / noise primitives ──────────────────────────────────────────────────

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(127.1, 311.7));
    q += dot(q, q + 19.19);
    return fract(q.x * q.y);
}

// Smooth value noise on [0, 1].
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    // Quintic smoothstep for C2 continuity
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    let v00 = hash21(i + vec2<f32>(0.0, 0.0));
    let v10 = hash21(i + vec2<f32>(1.0, 0.0));
    let v01 = hash21(i + vec2<f32>(0.0, 1.0));
    let v11 = hash21(i + vec2<f32>(1.0, 1.0));

    return mix(mix(v00, v10, u.x), mix(v01, v11, u.x), u.y);
}

// Fractional Brownian motion — layered octaves of value noise.
fn fbm(p_in: vec2<f32>, octaves: u32) -> f32 {
    var p   = p_in;
    var val = 0.0;
    var amp = 0.5;
    for (var i = 0u; i < octaves; i++) {
        val += amp * vnoise(p);
        p    = p * 2.13 + vec2<f32>(3.7, 1.9);
        amp *= 0.5;
    }
    return val;
}

// Domain-warped FBM (Inigo Quilez technique).
// Two levels of warping give rich swirling structure.
fn warped_fbm(uv: vec2<f32>, t: f32, warp: f32, freq: f32, oct: u32) -> f32 {
    let p = uv * freq;

    // First warp layer
    let q = vec2<f32>(
        fbm(p + vec2<f32>(0.00, 0.00) + t * 0.09, oct),
        fbm(p + vec2<f32>(5.20, 1.30) + t * 0.11, oct),
    );

    // Second warp layer
    let r = vec2<f32>(
        fbm(p + warp * q + vec2<f32>(1.70, 9.20) + t * 0.07, oct),
        fbm(p + warp * q + vec2<f32>(8.30, 2.80) + t * 0.08, oct),
    );

    return fbm(p + warp * r, oct);
}

// Inigo Quilez cosine palette.
fn palette(t: f32, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    return a + b * cos(6.28318 * (c * t + d));
}

// ── Entry point ──────────────────────────────────────────────────────────────

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(output_tex);
    if gid.x >= dims.x || gid.y >= dims.y { return; }

    let uv = vec2<f32>(gid.xy) / vec2<f32>(dims);

    // Offset UVs by seed so different objects or presets look distinct
    let uv_offset = uv + vec2<f32>(params.seed * 0.317, params.seed * 0.193);

    let n = warped_fbm(
        uv_offset,
        params.time,
        params.warp_strength,
        params.frequency,
        params.octaves,
    );

    let t = clamp(n, 0.0, 1.0);

    // Iron-mineral palette — deep cool blues through warm ochre-orange.
    // a + b stays below 0.6 per channel so the texture never blows out.
    let color = palette(
        t,
        vec3<f32>(0.28, 0.32, 0.38),   // midpoint: cool grey-blue
        vec3<f32>(0.26, 0.20, 0.16),   // amplitude: moderate
        vec3<f32>(0.90, 0.70, 0.50),   // frequency per channel
        vec3<f32>(0.10, 0.25, 0.50),   // phase offset per channel
    );

    textureStore(output_tex, vec2<i32>(gid.xy), vec4<f32>(color, 1.0));
}
