// 3D render shader — vertex projection + Lambertian lighting + texture sampling.
//
// Bind group 0 (uniform, dynamic offset): per-object Uniforms
// Bind group 1:  texture + sampler (the generative texture from compute pass)

// ── Uniforms ─────────────────────────────────────────────────────────────────
//
// Layout (std140 / wgpu uniform rules, verified against Rust repr(C)):
//   offset   0 : mvp         mat4x4<f32>  64 B
//   offset  64 : model       mat4x4<f32>  64 B
//   offset 128 : camera_pos  vec4<f32>    16 B  (.xyz used)
//   offset 144 : light_dir   vec4<f32>    16 B  (.xyz used)
//   offset 160 : time        f32           4 B
//   offset 164 : _pad[3]     f32 × 3      12 B
// Total: 176 B

struct Uniforms {
    mvp:        mat4x4<f32>,
    model:      mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir:  vec4<f32>,
    time:       f32,
    _pad0:      f32,
    _pad1:      f32,
    _pad2:      f32,
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

@group(1) @binding(0)
var gen_texture: texture_2d<f32>;

@group(1) @binding(1)
var tex_sampler: sampler;

// ── Vertex stage ─────────────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos:     vec4<f32>,
    @location(0)       world_normal: vec3<f32>,
    @location(1)       world_pos:    vec3<f32>,
    @location(2)       uv:           vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_pos     = u.mvp * vec4<f32>(in.position, 1.0);
    // Transform normal by the inverse-transpose of the model matrix.
    // For uniform-scale transforms, (model * normal).xyz is equivalent.
    out.world_normal = normalize((u.model * vec4<f32>(in.normal, 0.0)).xyz);
    out.world_pos    = (u.model * vec4<f32>(in.position, 1.0)).xyz;
    out.uv           = in.uv;
    return out;
}

// ── Fragment stage ────────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(gen_texture, tex_sampler, in.uv);

    let light  = normalize(u.light_dir.xyz);
    let normal = normalize(in.world_normal);

    // Lambertian diffuse + ambient
    let diff    = max(dot(normal, light), 0.0);
    let ambient = 0.12;
    let lit     = (ambient + diff * 0.88) * tex_color.rgb;

    // Very subtle rim — just enough to read depth at silhouette edges.
    let view_dir = normalize(u.camera_pos.xyz - in.world_pos);
    let rim      = pow(1.0 - max(dot(normal, view_dir), 0.0), 4.0) * 0.06;

    return vec4<f32>(lit + rim, tex_color.a);
}
