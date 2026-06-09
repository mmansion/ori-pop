// 2D canvas shader — vertex format v2.
//
// Vertices carry position, RGBA color, UV, and a texture slot. Slot 0.0 means
// "solid color" (the sampled texel is replaced by white); slot 1.0 multiplies
// the color by the bound texture (images, glyph atlases, offscreen canvases).

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var tex_2d: texture_2d<f32>;

@group(0) @binding(2)
var samp_2d: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tex: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tex: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let x = in.position.x / uniforms.resolution.x * 2.0 - 1.0;
    let y = 1.0 - in.position.y / uniforms.resolution.y * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    out.tex = in.tex;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(tex_2d, samp_2d, in.uv);
    let texel = mix(vec4<f32>(1.0), sampled, step(0.5, in.tex));
    return in.color * texel;
}
