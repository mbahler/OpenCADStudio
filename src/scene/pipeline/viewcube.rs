// ViewCube wgpu pipeline — OpenCADStudio-style interactive 3D navigation cube.
//
// 26 selectable regions:  6 faces + 12 edges + 8 corners.
// Phong shading + hover highlight passed as uniform.
// Hit-test is 100% CPU — no GPU readback needed.
//
// The ViewCube rotation matrix is derived directly from the camera quaternion
// (cam_rotation: Mat4) everywhere — shader, text labels, hit-test, hover-id.
// This eliminates gimbal lock at top/bottom views and keeps the cube in sync
// with arcball orbit at all angles.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use iced::wgpu;
use iced::{Rectangle, Size};

// ── ViewCube layout ───────────────────────────────────────────────────────
pub const VIEWCUBE_PX: u32 = 120;
pub const VIEWCUBE_SCALE: f32 = 0.36;
pub const VIEWCUBE_DRAW_PX: f32 = VIEWCUBE_PX as f32 * VIEWCUBE_SCALE * 2.0;
pub const VIEWCUBE_PAD: f32 = 12.0;

const FACE_LABELS: [&str; 6] = ["TOP", "BOTTOM", "FRONT", "BACK", "RIGHT", "LEFT"];
const FACE_CENTERS: [[f32; 3]; 6] = [
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
    [0.0, -1.0, 0.0],
    [0.0, 1.0, 0.0],
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
];

pub const FACE_TOP: usize = 0;
pub const FACE_BOTTOM: usize = 1;
pub const FACE_FRONT: usize = 2;
pub const FACE_BACK: usize = 3;
pub const FACE_RIGHT: usize = 4;
pub const FACE_LEFT: usize = 5;
pub const EDGE_TOP_FRONT: usize = 6;
pub const EDGE_TOP_BACK: usize = 7;
pub const EDGE_TOP_RIGHT: usize = 8;
pub const EDGE_TOP_LEFT: usize = 9;
pub const EDGE_BOT_FRONT: usize = 10;
pub const EDGE_BOT_BACK: usize = 11;
pub const EDGE_BOT_RIGHT: usize = 12;
pub const EDGE_BOT_LEFT: usize = 13;
pub const EDGE_FRONT_RIGHT: usize = 14;
pub const EDGE_FRONT_LEFT: usize = 15;
pub const EDGE_BACK_RIGHT: usize = 16;
pub const EDGE_BACK_LEFT: usize = 17;
pub const CORNER_TPF_R: usize = 18;
pub const CORNER_TPF_L: usize = 19;
pub const CORNER_TBK_R: usize = 20;
pub const CORNER_TBK_L: usize = 21;
pub const CORNER_BTF_R: usize = 22;
pub const CORNER_BTF_L: usize = 23;
pub const CORNER_BBK_R: usize = 24;
pub const CORNER_BBK_L: usize = 25;
pub const NUM_REGIONS: usize = 26;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CubeRegion {
    Face(usize),
    Edge(usize),
    Corner(usize),
}

impl CubeRegion {
    pub fn id(self) -> usize {
        match self {
            Self::Face(i) | Self::Edge(i) | Self::Corner(i) => i,
        }
    }

    /// Unit eye-direction vector (from target toward the camera) that
    /// looks straight at this region. Used by `Camera::snap_to_direction`
    /// which derives the full orientation by re-using the current
    /// camera's up vector, projected onto the plane perpendicular to
    /// this direction — so clicking an edge spins the cube around the
    /// edge without rolling the user's "up" sense.
    pub fn snap_direction(self) -> glam::Vec3 {
        let c = region_centroids()[self.id()];
        glam::Vec3::new(c[0], c[1], c[2]).normalize_or(glam::Vec3::Z)
    }

    pub fn opposite(self) -> CubeRegion {
        match self {
            CubeRegion::Face(FACE_TOP) => CubeRegion::Face(FACE_BOTTOM),
            CubeRegion::Face(FACE_BOTTOM) => CubeRegion::Face(FACE_TOP),
            CubeRegion::Face(FACE_FRONT) => CubeRegion::Face(FACE_BACK),
            CubeRegion::Face(FACE_BACK) => CubeRegion::Face(FACE_FRONT),
            CubeRegion::Face(FACE_RIGHT) => CubeRegion::Face(FACE_LEFT),
            CubeRegion::Face(FACE_LEFT) => CubeRegion::Face(FACE_RIGHT),
            other => other,
        }
    }

    pub fn label(self) -> &'static str {
        match self.id() {
            0 => "TOP",
            1 => "BOTTOM",
            2 => "FRONT",
            3 => "BACK",
            4 => "RIGHT",
            5 => "LEFT",
            6 => "Top Front",
            7 => "Top Back",
            8 => "Top Right",
            9 => "Top Left",
            10 => "Bot Front",
            11 => "Bot Back",
            12 => "Bot Right",
            13 => "Bot Left",
            14 => "Front Right",
            15 => "Front Left",
            16 => "Back Right",
            17 => "Back Left",
            18 => "Top Front Right",
            19 => "Top Front Left",
            20 => "Top Back Right",
            21 => "Top Back Left",
            22 => "Bot Front Right",
            23 => "Bot Front Left",
            24 => "Bot Back Right",
            25 => "Bot Back Left",
            _ => "?",
        }
    }
}

// ── Vertex ────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CubeVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub region_f: f32,
}

impl CubeVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32,
    ];
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CubeUniforms {
    pub view_proj: [f32; 16],
    pub rotation: [f32; 16],
    pub hover_region: [f32; 4],
}

impl CubeUniforms {
    /// Build uniforms from the camera quaternion-derived rotation matrix.
    /// `cam_rotation` = `Mat4::from_quat(camera.rotation)`.
    pub fn new(
        cam_rotation: Mat4,
        cube_px: u32,
        vp_w: u32,
        vp_h: u32,
        hover: Option<usize>,
    ) -> Self {
        let (hw, hh) = (vp_w as f32 * 0.5, vp_h as f32 * 0.5);
        let cube_half = cube_px as f32 * VIEWCUBE_SCALE;
        let cx = hw - cube_half - VIEWCUBE_PAD;
        let cy = hh - cube_half - VIEWCUBE_PAD;
        let view_proj = Mat4::orthographic_rh(-hw, hw, -hh, hh, -2000.0, 2000.0)
            * Mat4::from_translation(Vec3::new(cx, cy, 0.0))
            * Mat4::from_scale(Vec3::splat(cube_px as f32 * VIEWCUBE_SCALE));
        Self {
            view_proj: view_proj.to_cols_array(),
            rotation: cam_rotation.to_cols_array(),
            hover_region: [
                hover.map(|h| h as f32 / 25.0).unwrap_or(-1.0),
                0.0,
                0.0,
                0.0,
            ],
        }
    }
}

// ── Bitmap text ───────────────────────────────────────────────────────────

const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;
const CELL_W: usize = 6;
const CELL_H: usize = 8;
const ATLAS_COLS: usize = 8;
const ATLAS_ROWS: usize = 2;
const FONT_SCALE: f32 = 1.35;
const MAX_LABEL_CHARS: usize = 6;
const LABEL_COUNT: usize = 6;
const MAX_GLYPHS: usize = MAX_LABEL_CHARS * LABEL_COUNT;
const MAX_VERTS: usize = MAX_GLYPHS * 6;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct TextUniforms {
    screen: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct TextVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

impl TextVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2, 2 => Float32x4,
    ];
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

fn glyph_index(c: char) -> Option<usize> {
    match c {
        'A' => Some(0),
        'B' => Some(1),
        'C' => Some(2),
        'E' => Some(3),
        'F' => Some(4),
        'G' => Some(5),
        'H' => Some(6),
        'I' => Some(7),
        'K' => Some(8),
        'L' => Some(9),
        'M' => Some(10),
        'N' => Some(11),
        'O' => Some(12),
        'P' => Some(13),
        'R' => Some(14),
        'T' => Some(15),
        _ => None,
    }
}

fn glyph_rows(c: char) -> [u8; GLYPH_H] {
    match c {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10011, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        _ => [0; GLYPH_H],
    }
}

fn build_atlas() -> (Vec<u8>, u32, u32) {
    let w = (ATLAS_COLS * CELL_W) as u32;
    let h = (ATLAS_ROWS * CELL_H) as u32;
    let mut data = vec![0u8; (w * h) as usize];
    let glyphs = [
        'A', 'B', 'C', 'E', 'F', 'G', 'H', 'I', 'K', 'L', 'M', 'N', 'O', 'P', 'R', 'T',
    ];
    for (i, &ch) in glyphs.iter().enumerate() {
        let col = i % ATLAS_COLS;
        let row = i / ATLAS_COLS;
        let x0 = col * CELL_W;
        let y0 = row * CELL_H;
        let rows = glyph_rows(ch);
        for y in 0..GLYPH_H {
            let bits = rows[y];
            for x in 0..GLYPH_W {
                if (bits >> (GLYPH_W - 1 - x)) & 1 == 0 {
                    continue;
                }
                data[(y0 + y) as usize * w as usize + (x0 + x)] = 255;
            }
        }
    }
    (data, w, h)
}

fn glyph_uv(index: usize, atlas_w: f32, atlas_h: f32) -> (f32, f32, f32, f32) {
    let col = index % ATLAS_COLS;
    let row = index / ATLAS_COLS;
    let x0 = (col * CELL_W) as f32;
    let y0 = (row * CELL_H) as f32;
    (
        x0 / atlas_w,
        y0 / atlas_h,
        (x0 + GLYPH_W as f32) / atlas_w,
        (y0 + GLYPH_H as f32) / atlas_h,
    )
}

struct ViewCubeText {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: u32,
    vertex_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    atlas_w: f32,
    atlas_h: f32,
}

impl ViewCubeText {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let (atlas, w, h) = build_atlas();
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vc.text_atlas"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes_per_row = w;
        let aligned_bpr = ((bytes_per_row + 255) / 256) * 256;
        let atlas_bytes = if aligned_bpr == bytes_per_row {
            atlas
        } else {
            let mut padded = vec![0u8; (aligned_bpr * h) as usize];
            for row in 0..h as usize {
                let src = row * bytes_per_row as usize;
                let dst = row * aligned_bpr as usize;
                padded[dst..dst + bytes_per_row as usize]
                    .copy_from_slice(&atlas[src..src + bytes_per_row as usize]);
            }
            padded
        };
        queue.write_texture(
            atlas_tex.as_image_copy(),
            &atlas_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned_bpr),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vc.text_sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vc.text_uniform"),
            size: std::mem::size_of::<TextUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vc.text_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vc.text_bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vc.text_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vc.text_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "../../shaders/viewcube_text.wgsl"
            ))),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vc.text_pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[TextVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });
        let vertex_capacity = MAX_VERTS as u32;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vc.text_vb"),
            size: (vertex_capacity as usize * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
            uniform_buffer,
            bind_group,
            atlas_w: w as f32,
            atlas_h: h as f32,
        }
    }

    /// Update text labels using the quaternion-derived rotation matrix.
    fn update(
        &mut self,
        queue: &wgpu::Queue,
        cam_rotation: Mat4,
        vp_w: u32,
        vp_h: u32,
        cube_px: u32,
    ) {
        let (vw, vh) = (vp_w as f32, vp_h as f32);
        let cube_half = cube_px as f32 * VIEWCUBE_SCALE;
        let (hw, hh) = (vw * 0.5, vh * 0.5);
        let view_proj = Mat4::orthographic_rh(-hw, hw, -hh, hh, -2000.0, 2000.0)
            * Mat4::from_translation(Vec3::new(
                hw - cube_half - VIEWCUBE_PAD,
                hh - cube_half - VIEWCUBE_PAD,
                0.0,
            ))
            * Mat4::from_scale(Vec3::splat(cube_px as f32 * VIEWCUBE_SCALE));

        let glyph_w = GLYPH_W as f32 * FONT_SCALE;
        let glyph_h = GLYPH_H as f32 * FONT_SCALE;
        let advance = CELL_W as f32 * FONT_SCALE;
        let mut verts: Vec<TextVertex> = Vec::with_capacity(MAX_VERTS);
        let view_dir = Vec3::Z;

        for (fi, &c) in FACE_CENTERS.iter().enumerate() {
            let face_n = Vec3::from(c);
            let world_n = cam_rotation.transform_vector3(face_n).normalize();
            let dot = world_n.dot(view_dir);
            if dot < 0.12 {
                continue;
            }
            let world = cam_rotation.transform_point3(face_n * 0.82);
            let clip = view_proj * Vec4::new(world.x, world.y, world.z, 1.0);
            if clip.w.abs() < 1e-6 {
                continue;
            }
            let sx = (clip.x / clip.w + 1.0) * 0.5 * vw;
            let sy = (1.0 - clip.y / clip.w) * 0.5 * vh;
            let alpha = ((dot - 0.12) / 0.88).clamp(0.0, 1.0);
            let label = FACE_LABELS[fi];
            let label_w = label.len() as f32 * advance;
            let mut x = sx - label_w * 0.5;
            let y = sy - glyph_h * 0.5;
            for ch in label.chars() {
                let Some(gi) = glyph_index(ch) else {
                    x += advance;
                    continue;
                };
                let (u0, v0, u1, v1) = glyph_uv(gi, self.atlas_w, self.atlas_h);
                let (x0, y0, x1, y1) = (x, y, x + glyph_w, y + glyph_h);
                let color = [1.0, 1.0, 1.0, alpha];
                verts.push(TextVertex {
                    pos: [x0, y0],
                    uv: [u0, v0],
                    color,
                });
                verts.push(TextVertex {
                    pos: [x1, y0],
                    uv: [u1, v0],
                    color,
                });
                verts.push(TextVertex {
                    pos: [x1, y1],
                    uv: [u1, v1],
                    color,
                });
                verts.push(TextVertex {
                    pos: [x0, y0],
                    uv: [u0, v0],
                    color,
                });
                verts.push(TextVertex {
                    pos: [x1, y1],
                    uv: [u1, v1],
                    color,
                });
                verts.push(TextVertex {
                    pos: [x0, y1],
                    uv: [u0, v1],
                    color,
                });
                x += advance;
                if verts.len() >= self.vertex_capacity as usize {
                    break;
                }
            }
            if verts.len() >= self.vertex_capacity as usize {
                break;
            }
        }
        self.vertex_count = verts.len() as u32;
        if self.vertex_count > 0 {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        }
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&TextUniforms {
                screen: [vw, vh],
                _pad: [0.0; 2],
            }),
        );
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip: Rectangle<u32>,
    ) {
        if self.vertex_count == 0 {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vc.text_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(
            clip.x as f32,
            clip.y as f32,
            clip.width as f32,
            clip.height as f32,
            0.0,
            1.0,
        );
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

// ── Geometry ──────────────────────────────────────────────────────────────

const F: f32 = 0.80;
const E: f32 = 1.00;
const C_TOP: [f32; 3] = [0.70, 0.80, 0.94];
const C_BOTTOM: [f32; 3] = [0.32, 0.32, 0.36];
const C_FRONT: [f32; 3] = [0.80, 0.83, 0.90];
const C_BACK: [f32; 3] = [0.46, 0.47, 0.52];
const C_RIGHT: [f32; 3] = [0.62, 0.60, 0.56];
const C_LEFT: [f32; 3] = [0.54, 0.55, 0.64];
const C_EDGE: [f32; 3] = [0.24, 0.25, 0.28];
const C_CORNER: [f32; 3] = [0.16, 0.17, 0.19];

fn push_quad(
    corners: [[f32; 3]; 4],
    rgb: [f32; 3],
    region: usize,
    vs: &mut Vec<CubeVertex>,
    is: &mut Vec<u32>,
) {
    let mut cs = corners;
    let center = {
        let s = Vec3::from(cs[0]) + Vec3::from(cs[1]) + Vec3::from(cs[2]) + Vec3::from(cs[3]);
        (s * 0.25).normalize_or_zero()
    };
    let mut n = (Vec3::from(cs[1]) - Vec3::from(cs[0]))
        .cross(Vec3::from(cs[3]) - Vec3::from(cs[0]))
        .normalize_or_zero();
    if n.dot(center) < 0.0 {
        cs = [cs[0], cs[3], cs[2], cs[1]];
        n = (Vec3::from(cs[1]) - Vec3::from(cs[0]))
            .cross(Vec3::from(cs[3]) - Vec3::from(cs[0]))
            .normalize_or_zero();
    }
    let n = n.to_array();
    let rf = region as f32 / 25.0;
    let base = vs.len() as u32;
    for pos in cs {
        vs.push(CubeVertex {
            pos,
            normal: n,
            color: rgb,
            region_f: rf,
        });
    }
    is.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn push_tri(
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    rgb: [f32; 3],
    region: usize,
    vs: &mut Vec<CubeVertex>,
    is: &mut Vec<u32>,
) {
    let mut b = b;
    let mut c = c;
    let center = {
        let s = Vec3::from(a) + Vec3::from(b) + Vec3::from(c);
        (s / 3.0).normalize_or_zero()
    };
    let mut n = (Vec3::from(b) - Vec3::from(a))
        .cross(Vec3::from(c) - Vec3::from(a))
        .normalize_or_zero();
    if n.dot(center) < 0.0 {
        std::mem::swap(&mut b, &mut c);
        n = (Vec3::from(b) - Vec3::from(a))
            .cross(Vec3::from(c) - Vec3::from(a))
            .normalize_or_zero();
    }
    let n = n.to_array();
    let rf = region as f32 / 25.0;
    let base = vs.len() as u32;
    for pos in [a, b, c] {
        vs.push(CubeVertex {
            pos,
            normal: n,
            color: rgb,
            region_f: rf,
        });
    }
    is.extend_from_slice(&[base, base + 1, base + 2]);
}

pub fn build_geometry() -> (Vec<CubeVertex>, Vec<u32>) {
    let (mut vs, mut is) = (Vec::<CubeVertex>::new(), Vec::<u32>::new());
    push_quad(
        [[-F, -F, E], [F, -F, E], [F, F, E], [-F, F, E]],
        C_TOP,
        FACE_TOP,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-F, F, -E], [F, F, -E], [F, -F, -E], [-F, -F, -E]],
        C_BOTTOM,
        FACE_BOTTOM,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[F, -E, -F], [-F, -E, -F], [-F, -E, F], [F, -E, F]],
        C_FRONT,
        FACE_FRONT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-F, E, -F], [F, E, -F], [F, E, F], [-F, E, F]],
        C_BACK,
        FACE_BACK,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[E, F, -F], [E, -F, -F], [E, -F, F], [E, F, F]],
        C_RIGHT,
        FACE_RIGHT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-E, -F, -F], [-E, F, -F], [-E, F, F], [-E, -F, F]],
        C_LEFT,
        FACE_LEFT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[F, -F, E], [-F, -F, E], [-F, -E, F], [F, -E, F]],
        C_EDGE,
        EDGE_TOP_FRONT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-F, F, E], [F, F, E], [F, E, F], [-F, E, F]],
        C_EDGE,
        EDGE_TOP_BACK,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[F, F, E], [F, -F, E], [E, -F, F], [E, F, F]],
        C_EDGE,
        EDGE_TOP_RIGHT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-F, -F, E], [-F, F, E], [-E, F, F], [-E, -F, F]],
        C_EDGE,
        EDGE_TOP_LEFT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[F, -F, -E], [-F, -F, -E], [-F, -E, -F], [F, -E, -F]],
        C_EDGE,
        EDGE_BOT_FRONT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-F, F, -E], [F, F, -E], [F, E, -F], [-F, E, -F]],
        C_EDGE,
        EDGE_BOT_BACK,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[F, F, -E], [F, -F, -E], [E, -F, -F], [E, F, -F]],
        C_EDGE,
        EDGE_BOT_RIGHT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-F, -F, -E], [-F, F, -E], [-E, F, -F], [-E, -F, -F]],
        C_EDGE,
        EDGE_BOT_LEFT,
        &mut vs,
        &mut is,
    );
    // Side edges: diagonal chamfer strips connecting vertical face pairs.
    // Each strip spans from one face edge to the adjacent face edge — not flat in one plane.
    push_quad(
        [[F, -E, -F], [F, -E, F], [E, -F, F], [E, -F, -F]],
        C_EDGE,
        EDGE_FRONT_RIGHT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-F, -E, F], [-F, -E, -F], [-E, -F, -F], [-E, -F, F]],
        C_EDGE,
        EDGE_FRONT_LEFT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[F, E, F], [F, E, -F], [E, F, -F], [E, F, F]],
        C_EDGE,
        EDGE_BACK_RIGHT,
        &mut vs,
        &mut is,
    );
    push_quad(
        [[-F, E, F], [-F, E, -F], [-E, F, -F], [-E, F, F]],
        C_EDGE,
        EDGE_BACK_LEFT,
        &mut vs,
        &mut is,
    );
    for &([sx, sy, sz], region) in &[
        ([1.0f32, 1.0, 1.0], CORNER_TBK_R), // sy=+1 → BACK direction
        ([-1.0, 1.0, 1.0], CORNER_TBK_L),
        ([1.0, 1.0, -1.0], CORNER_BBK_R),
        ([-1.0, 1.0, -1.0], CORNER_BBK_L),
        ([1.0, -1.0, 1.0], CORNER_TPF_R), // sy=-1 → FRONT direction
        ([-1.0, -1.0, 1.0], CORNER_TPF_L),
        ([1.0, -1.0, -1.0], CORNER_BTF_R),
        ([-1.0, -1.0, -1.0], CORNER_BTF_L),
    ] {
        push_tri(
            [sx * F, sy * F, sz * E],
            [sx * F, sy * E, sz * F],
            [sx * E, sy * F, sz * F],
            C_CORNER,
            region,
            &mut vs,
            &mut is,
        );
    }
    (vs, is)
}

pub fn region_centroids() -> [[f32; 3]; NUM_REGIONS] {
    let m = (F + E) * 0.5;
    [
        [0.0, 0.0, E],  // FACE_TOP
        [0.0, 0.0, -E], // FACE_BOTTOM
        [0.0, -E, 0.0], // FACE_FRONT  (geometry Y=-E)
        [0.0, E, 0.0],  // FACE_BACK   (geometry Y=+E)
        [E, 0.0, 0.0],  // FACE_RIGHT
        [-E, 0.0, 0.0], // FACE_LEFT
        [0.0, -m, m],   // EDGE_TOP_FRONT
        [0.0, m, m],    // EDGE_TOP_BACK
        [m, 0.0, m],    // EDGE_TOP_RIGHT
        [-m, 0.0, m],   // EDGE_TOP_LEFT
        [0.0, -m, -m],  // EDGE_BOT_FRONT
        [0.0, m, -m],   // EDGE_BOT_BACK
        [m, 0.0, -m],   // EDGE_BOT_RIGHT
        [-m, 0.0, -m],  // EDGE_BOT_LEFT
        [m, -m, 0.0],   // EDGE_FRONT_RIGHT
        [-m, -m, 0.0],  // EDGE_FRONT_LEFT
        [m, m, 0.0],    // EDGE_BACK_RIGHT
        [-m, m, 0.0],   // EDGE_BACK_LEFT
        [m, -m, m],     // CORNER_TPF_R  (geometry sy=-1 = FRONT)
        [-m, -m, m],    // CORNER_TPF_L
        [m, m, m],      // CORNER_TBK_R  (geometry sy=+1 = BACK)
        [-m, m, m],     // CORNER_TBK_L
        [m, -m, -m],    // CORNER_BTF_R  (geometry sy=-1 = FRONT)
        [-m, -m, -m],   // CORNER_BTF_L
        [m, m, -m],     // CORNER_BBK_R  (geometry sy=+1 = BACK)
        [-m, m, -m],    // CORNER_BBK_L
    ]
}

fn threshold_sq(id: usize, cube_half_px: f32) -> f32 {
    let r = if id < 6 {
        cube_half_px * 0.92
    } else if id < 18 {
        cube_half_px * 0.38
    } else {
        cube_half_px * 0.28
    };
    r * r
}

// ── Pipeline ──────────────────────────────────────────────────────────────

pub struct ViewCubePipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    depth_texture_size: Size<u32>,
    depth_view: wgpu::TextureView,
    pub cube_px: u32,
    text: ViewCubeText,
}

impl ViewCubePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        use wgpu::util::DeviceExt;
        let (verts, idxs) = build_geometry();
        let cube_px = VIEWCUBE_PX;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vc.vb"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vc.ib"),
            contents: bytemuck::cast_slice(&idxs),
            usage: wgpu::BufferUsages::INDEX,
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vc.ub"),
            size: std::mem::size_of::<CubeUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vc.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vc.bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vc.layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vc.shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "../../shaders/viewcube.wgsl"
            ))),
        });
        let depth_tex = create_depth_texture(device, Size::new(1, 1));
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vc.pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[CubeVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });
        let text = ViewCubeText::new(device, queue, format);
        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: idxs.len() as u32,
            uniform_buffer,
            uniform_bind_group,
            depth_texture_size: Size::new(1, 1),
            depth_view,
            cube_px,
            text,
        }
    }

    /// Upload using the quaternion rotation matrix.
    /// `cam_rotation` = `camera.view_rotation_mat()` = `Mat4::from_quat(camera.rotation)`.
    pub fn upload(
        &mut self,
        queue: &wgpu::Queue,
        cam_rotation: Mat4,
        vp_w: u32,
        vp_h: u32,
        hover: Option<usize>,
    ) {
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&CubeUniforms::new(
                cam_rotation,
                self.cube_px,
                vp_w,
                vp_h,
                hover,
            )),
        );
        self.text
            .update(queue, cam_rotation, vp_w, vp_h, self.cube_px);
    }

    pub fn ensure_depth_texture(&mut self, device: &wgpu::Device, size: Size<u32>) {
        if self.depth_texture_size != size {
            let tex = create_depth_texture(device, size);
            self.depth_view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.depth_texture_size = size;
        }
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip: Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vc.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(
            clip.x as f32,
            clip.y as f32,
            clip.width as f32,
            clip.height as f32,
            0.0,
            1.0,
        );
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
        drop(pass);
        self.text.render(encoder, target, clip);
    }
}

impl iced::widget::shader::Pipeline for ViewCubePipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self::new(device, queue, format)
    }
}

fn create_depth_texture(device: &wgpu::Device, size: Size<u32>) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vc.depth_texture"),
        size: wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

// ── Hit test ──────────────────────────────────────────────────────────────
//
// hit_test and hover_id now take cam_rotation: Mat4 (same matrix the shader
// uses) so click regions always match what is drawn — including after free
// arcball orbit from any angle.

/// Returns the ViewCube region under screen position (mx, my), or None.
/// `cam_rotation` must be `camera.view_rotation_mat()`.
pub fn hit_test(
    mx: f32,
    my: f32,
    vp_w: f32,
    vp_h: f32,
    cam_rotation: Mat4,
    cube_px: u32,
) -> Option<CubeRegion> {
    let half = cube_px as f32 * VIEWCUBE_SCALE;
    let cx = vp_w - half - VIEWCUBE_PAD;
    let cy = half + VIEWCUBE_PAD;
    if (mx - cx).abs() > half || (my - cy).abs() > half {
        return None;
    }

    let (hw, hh) = (vp_w * 0.5, vp_h * 0.5);
    let vp = Mat4::orthographic_rh(-hw, hw, -hh, hh, -2000.0, 2000.0)
        * Mat4::from_translation(Vec3::new(
            hw - half - VIEWCUBE_PAD,
            hh - half - VIEWCUBE_PAD,
            0.0,
        ))
        * Mat4::from_scale(Vec3::splat(cube_px as f32 * VIEWCUBE_SCALE));

    let view_dir = Vec3::Z;
    let centroids = region_centroids();
    let (mut best, mut best_d) = (None, f32::MAX);

    for (id, &c) in centroids.iter().enumerate() {
        let world = cam_rotation.transform_point3(Vec3::from(c));
        if world.normalize().dot(view_dir) < 0.05 {
            continue;
        }
        let clip = vp * Vec4::new(world.x, world.y, world.z, 1.0);
        if clip.w.abs() < 1e-6 {
            continue;
        }
        let sx = (clip.x / clip.w + 1.0) * 0.5 * vp_w;
        let sy = (1.0 - clip.y / clip.w) * 0.5 * vp_h;
        let d = (sx - mx).powi(2) + (sy - my).powi(2);
        if d < threshold_sq(id, half) && d < best_d {
            best_d = d;
            best = Some(if id < 6 {
                CubeRegion::Face(id)
            } else if id < 18 {
                CubeRegion::Edge(id)
            } else {
                CubeRegion::Corner(id)
            });
        }
    }
    best
}

/// Returns the hovered region id (0-25), or None.
pub fn hover_id(
    mx: f32,
    my: f32,
    vp_w: f32,
    vp_h: f32,
    cam_rotation: Mat4,
    cube_px: u32,
) -> Option<usize> {
    hit_test(mx, my, vp_w, vp_h, cam_rotation, cube_px).map(|r| r.id())
}
