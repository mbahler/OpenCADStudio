// Mesh GPU buffers — TriangleList rendering for solid objects.
//
// Vertex layout (40 bytes):
//   position   [f32; 3]   offset  0   12 B
//   normal     [f32; 3]   offset 12   12 B
//   color      [f32; 4]   offset 24   16 B
//                                ------
//                                 40 B / vertex

use crate::scene::model::mesh_model::{MeshLodSet, MeshModel};
use iced::wgpu;
use iced::wgpu::util::DeviceExt;

// ── Vertex layout ─────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub position_low: [f32; 3],
}

impl MeshVertex {
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRS: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute {
                offset: std::mem::offset_of!(MeshVertex, position) as u64,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: std::mem::offset_of!(MeshVertex, normal) as u64,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: std::mem::offset_of!(MeshVertex, color) as u64,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: std::mem::offset_of!(MeshVertex, position_low) as u64,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x3,
            },
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: ATTRS,
        }
    }
}

// ── GPU handle ────────────────────────────────────────────────────────────

pub struct MeshGpu {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    /// Line-list index buffer: every triangle `(a, b, c)` from the
    /// solid index buffer is expanded into three segments
    /// `(a,b)(b,c)(c,a)`. Used by the wireframe-mode render path so 3D
    /// solids draw as their triangle edges without needing the
    /// `POLYGON_MODE_LINE` device feature.
    #[allow(dead_code)] // only the highlight overlay builds MeshGpu now (fill only)
    pub wire_index_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    pub wire_index_count: u32,
}

/// GPU-side bundle of MeshLodSet — one MeshGpu per available LOD plus
/// the world-XY AABB needed to pick a level per frame.
pub struct MeshLodGpu {
    pub lods: Vec<MeshGpu>,
    pub world_aabb: [f32; 4],
}

/// How a solid mesh is highlighted this frame.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Highlight {
    #[allow(dead_code)] // the highlight overlay only builds Selected / Hover
    None,
    /// Hovered — light orange wash.
    Hover,
    /// Selected — stronger blue wash.
    Selected,
}

impl Highlight {
    /// Blend colour and mix factor, or `None` when the mesh keeps its colour.
    fn tint(self) -> Option<([f32; 4], f32)> {
        match self {
            Highlight::None => None,
            Highlight::Hover => Some(([0.95, 0.55, 0.10, 1.0], 0.35)),
            Highlight::Selected => Some(([0.15, 0.55, 1.0, 1.0], 0.60)),
        }
    }
}

// ── Batched mesh buffers ──────────────────────────────────────────────────
//
// One MeshGpu per solid means one vertex/index bind + draw call per solid —
// ~10k draw calls a frame on a heavy 3D model, which strangles the GPU front
// end. The batch concatenates every solid's LOD0 geometry into a handful of
// large buffers (split only to stay under the 256 MB per-buffer cap), so the
// whole mesh set draws in a few calls. Vertices already carry their own colour,
// so no per-mesh state is needed between draws. Built once per geometry epoch —
// selection/hover no longer rebuild it (that tint is dropped in the batch path).

/// Cap on vertices per batch buffer: 40 B/vertex × 6 M ≈ 240 MB, under the
/// default 256 MB `max_buffer_size`. Solids beyond the cap spill into the next
/// chunk.
const BATCH_MAX_VERTS: usize = 6_000_000;

pub struct MeshBatchChunk {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub wire_index_buffer: wgpu::Buffer,
    pub wire_index_count: u32,
}

fn make_chunk(
    device: &wgpu::Device,
    verts: &[MeshVertex],
    indices: &[u32],
    wire_indices: &[u32],
) -> MeshBatchChunk {
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh.batch.vbuf"),
        contents: bytemuck::cast_slice(verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh.batch.ibuf"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let wire_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh.batch.wire_ibuf"),
        contents: bytemuck::cast_slice(wire_indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    MeshBatchChunk {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
        wire_index_buffer,
        wire_index_count: wire_indices.len() as u32,
    }
}

/// Concatenate every set's first non-empty LOD into a few large GPU buffers.
/// Returns the chunks plus the total triangle count drawn (for diagnostics).
pub fn build_mesh_batch(device: &wgpu::Device, sets: &[MeshLodSet]) -> (Vec<MeshBatchChunk>, u64) {
    let mut chunks = Vec::new();
    let mut verts: Vec<MeshVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut wire_indices: Vec<u32> = Vec::new();
    let mut total_tris = 0u64;
    for set in sets {
        let Some(mesh) = set.lods.iter().find(|m| !m.indices.is_empty()) else {
            continue;
        };
        if !verts.is_empty() && verts.len() + mesh.verts.len() > BATCH_MAX_VERTS {
            chunks.push(make_chunk(device, &verts, &indices, &wire_indices));
            verts.clear();
            indices.clear();
            wire_indices.clear();
        }
        let base = verts.len() as u32;
        let has_normals = mesh.normals.len() == mesh.verts.len();
        for (i, &pos) in mesh.verts.iter().enumerate() {
            verts.push(MeshVertex {
                position: pos,
                normal: if has_normals { mesh.normals[i] } else { [0.0, 1.0, 0.0] },
                color: mesh.color,
                position_low: mesh.verts_low.get(i).copied().unwrap_or([0.0; 3]),
            });
        }
        for &idx in &mesh.indices {
            indices.push(base + idx);
        }
        for tri in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (base + tri[0], base + tri[1], base + tri[2]);
            wire_indices.extend_from_slice(&[a, b, b, c, c, a]);
        }
        total_tris += (mesh.indices.len() / 3) as u64;
    }
    if !indices.is_empty() {
        chunks.push(make_chunk(device, &verts, &indices, &wire_indices));
    }
    (chunks, total_tris)
}

impl MeshLodGpu {
    #[allow(dead_code)] // built by the bypassed per-mesh upload_meshes path
    pub fn new(device: &wgpu::Device, set: &MeshLodSet, highlight: Highlight) -> Self {
        Self {
            lods: set
                .lods
                .iter()
                .filter(|m| !m.indices.is_empty())
                .map(|m| MeshGpu::new(device, m, highlight))
                .collect(),
            world_aabb: set.world_aabb,
        }
    }
}

impl MeshGpu {
    pub fn new(device: &wgpu::Device, mesh: &MeshModel, highlight: Highlight) -> Self {
        let has_normals = mesh.normals.len() == mesh.verts.len();
        // Blend the base colour toward the highlight so a selected / hovered
        // solid reads clearly while keeping some shape shading.
        let color = match highlight.tint() {
            Some((hl, t)) => {
                let mut c = [0.0f32; 4];
                for k in 0..4 {
                    c[k] = mesh.color[k] * (1.0 - t) + hl[k] * t;
                }
                c
            }
            None => mesh.color,
        };
        let vertices: Vec<MeshVertex> = mesh
            .verts
            .iter()
            .enumerate()
            .map(|(i, &pos)| {
                let normal = if has_normals {
                    mesh.normals[i]
                } else {
                    [0.0, 1.0, 0.0]
                };
                MeshVertex {
                    position: pos,
                    normal,
                    color,
                    position_low: mesh.verts_low.get(i).copied().unwrap_or([0.0; 3]),
                }
            })
            .collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("mesh.vbuf.{}", mesh.name)),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("mesh.ibuf.{}", mesh.name)),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Wireframe-mode index buffer: expand each triangle into its
        // three edge segments. Allocates ~2× the solid index count but
        // is cheap compared to mesh tessellation and only happens when
        // a new mesh is uploaded.
        let mut wire_indices: Vec<u32> = Vec::with_capacity(mesh.indices.len() * 2);
        for tri in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            wire_indices.extend_from_slice(&[a, b, b, c, c, a]);
        }
        let wire_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("mesh.wire_ibuf.{}", mesh.name)),
            contents: bytemuck::cast_slice(&wire_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            wire_index_buffer,
            wire_index_count: wire_indices.len() as u32,
        }
    }
}
