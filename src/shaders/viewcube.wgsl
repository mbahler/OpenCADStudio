// ViewCube shader — Phong lighting + hover highlight.

struct Uniforms {
    view_proj:    mat4x4<f32>,
    rotation:     mat4x4<f32>,
    hover_region: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VIn  { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) region_f: f32 }
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) color: vec3<f32>, @location(1) world_n: vec3<f32>, @location(2) region_f: f32 }

@vertex
fn vs_main(in: VIn) -> VOut {
    var out: VOut;
    let rotated = (u.rotation * vec4<f32>(in.pos, 1.0)).xyz;
    out.clip     = u.view_proj * vec4<f32>(rotated, 1.0);
    out.color    = in.color;
    out.world_n  = normalize((u.rotation * vec4<f32>(in.normal, 0.0)).xyz);
    out.region_f = in.region_f;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let is_hovered = abs(in.region_f - u.hover_region.x) < 0.001 && u.hover_region.x >= 0.0;
    var final_rgb = in.color;
    if is_hovered {
        let glow = vec3<f32>(0.68, 0.88, 1.0);
        final_rgb = mix(final_rgb, glow, 0.60);
        final_rgb = clamp(final_rgb * 1.30, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(final_rgb, 1.0);
}
