// Textured-quad shader for raster images (RasterImage entity).
// Renders a four-vertex quad (two triangles) with a sampled texture.

// ── Bind group 0: shared projection uniforms ─────────────────────────────────
struct Uniforms {
    mvp:    mat4x4<f32>,
    view:   mat4x4<f32>,
    width:  f32,
    height: f32,
    cam_z:  f32,
    _pad:   f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

// ── Bind group 1: per-image texture + sampler ────────────────────────────────
@group(1) @binding(0) var img_texture: texture_2d<f32>;
@group(1) @binding(1) var img_sampler: sampler;

// Per-image params (fade, clip flag, etc.)
struct ImageParams {
    opacity:    f32,
    draw_depth: f32,   // signed (-1,1) draw-order bias; 0 = neutral
    _pad1:      f32,
    _pad2:      f32,
};
@group(1) @binding(2) var<uniform> img_params: ImageParams;

// Draw-order depth bias (see wire.wgsl).
const DRAW_ORDER_BIAS: f32 = 0.001;

// ── Vertex stage ──────────────────────────────────────────────────────────────
struct VertIn {
    @location(0) pos:  vec3<f32>,
    @location(1) uv:   vec2<f32>,
};

struct VertOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
};

@vertex
fn vs_main(in: VertIn) -> VertOut {
    var out: VertOut;
    out.clip_pos = u.mvp * vec4<f32>(in.pos, 1.0);
    out.clip_pos.z = out.clip_pos.z - img_params.draw_depth * DRAW_ORDER_BIAS * out.clip_pos.w;
    out.uv = in.uv;
    return out;
}

// ── Fragment stage ────────────────────────────────────────────────────────────
@fragment
fn fs_main(in: VertOut) -> @location(0) vec4<f32> {
    let col = textureSample(img_texture, img_sampler, in.uv);
    return vec4<f32>(col.rgb, col.a * img_params.opacity);
}
