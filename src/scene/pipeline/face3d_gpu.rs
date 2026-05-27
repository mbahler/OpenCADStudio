// Face3D GPU buffer — batches all DXF 3DFACE entities into a single
// TriangleList buffer for efficient rendering.
//
// Each Face3D quad (4 corners) produces 2 triangles → 6 vertices.
// All entities are merged into one wgpu::Buffer → 1 draw call total.
//
// Vertex layout (28 bytes):
//   position  [f32; 3]   offset  0   12 B
//   color     [f32; 4]   offset 12   16 B
//                                ------
//                                 28 B / vertex
//
// 3D vs 2D split: `vertex_buffer_3d` holds 3DFACE quads + PolyfaceMesh /
// PolygonMesh face triangles (the "3D" geometry that participates in
// hidden-surface removal). `vertex_buffer_2d` holds the residual fills
// — text-LOD greek dim, MultiLeader background — whose source
// WireModels have an empty `points` list. Splitting them lets the
// render pass send the 3D side through a depth-only pipeline for
// HiddenLine while keeping the 2D side fully visible.

use crate::scene::wire_model::WireModel;
use iced::wgpu;
use iced::wgpu::util::DeviceExt;

// ── Vertex layout ──────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Face3DVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl Face3DVertex {
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Face3DVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

// ── GPU handle ─────────────────────────────────────────────────────────────

pub struct Face3DGpu {
    /// 3DFACE quads + PolyfaceMesh / PolygonMesh face triangles.
    /// HiddenLine routes this through the depth-only pipeline so the
    /// fragments occlude wires behind them without drawing visible
    /// pixels.
    pub vertex_buffer_3d: wgpu::Buffer,
    pub vertex_count_3d: u32,
    /// Text-LOD greek dim, MultiLeader background, etc. — fills whose
    /// source wire has an empty `points` list. Always rendered with the
    /// normal face3d pipeline (visible in every mode).
    pub vertex_buffer_2d: wgpu::Buffer,
    pub vertex_count_2d: u32,
}

impl Face3DGpu {
    /// Build a batched GPU buffer from Face3D wire models and mesh fill_tris.
    ///
    /// - `face3d_wires`: Face3D entities — `key_vertices` holds 4 quad corners;
    ///   emits 2 triangles per face into the 3D buffer.
    /// - `all_wires`: all entity wires — `fill_tris` holds pre-triangulated
    ///   fill data. Wires with non-empty `points` (PolyfaceMesh / PolygonMesh
    ///   face data) feed the 3D buffer; wires with empty `points` (2D fills)
    ///   feed the 2D buffer.
    /// - `keep_3d_mesh_fills`: when false (wireframe modes), the 3D side
    ///   is left empty; the 2D side is always populated.
    pub fn from_wires(
        device: &wgpu::Device,
        face3d_wires: &[WireModel],
        all_wires: &[WireModel],
        keep_3d_mesh_fills: bool,
    ) -> Self {
        let mut verts_3d: Vec<Face3DVertex> = Vec::with_capacity(face3d_wires.len() * 6);
        let mut verts_2d: Vec<Face3DVertex> = Vec::new();

        // Face3D quads (4 key_vertices → 2 triangles) — only when 3D
        // fills are wanted.
        if keep_3d_mesh_fills {
            for wire in face3d_wires {
                if wire.key_vertices.len() < 4 {
                    continue;
                }
                let [r, g, b, a] = wire.color;
                let fill_color = [r * 0.45, g * 0.45, b * 0.45, a];
                let p = &wire.key_vertices;
                let v = |i: usize| Face3DVertex {
                    position: p[i],
                    color: fill_color,
                };
                verts_3d.push(v(0));
                verts_3d.push(v(1));
                verts_3d.push(v(2));
                verts_3d.push(v(0));
                verts_3d.push(v(2));
                verts_3d.push(v(3));
            }
        }

        // PolyfaceMesh / PolygonMesh / unlit fills (text greek, MultiLeader
        // background). Wires whose `points` are empty carry pure 2-D fills
        // that should render at their literal color — applying the 0.45
        // AO-style dim to them would wash out user-picked colors. Wires
        // with both fill_tris and points (mesh edges + faces) keep the dim
        // so PolyfaceMesh / PolygonMesh still look 3-D-shaded.
        //
        // 2-D fills always go to `verts_2d` (visible in every mode).
        // 3-D mesh face data goes to `verts_3d` only when
        // `keep_3d_mesh_fills` is true.
        for wire in all_wires {
            if wire.fill_tris.is_empty() {
                continue;
            }
            let is_3d_mesh_face = !wire.points.is_empty();
            let [r, g, b, a] = wire.color;
            if is_3d_mesh_face {
                if !keep_3d_mesh_fills {
                    continue;
                }
                let fill_color = [r * 0.45, g * 0.45, b * 0.45, a];
                for &position in &wire.fill_tris {
                    verts_3d.push(Face3DVertex {
                        position,
                        color: fill_color,
                    });
                }
            } else {
                let fill_color = [r, g, b, a];
                for &position in &wire.fill_tris {
                    verts_2d.push(Face3DVertex {
                        position,
                        color: fill_color,
                    });
                }
            }
        }

        let vertex_buffer_3d = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("face3d.vbuf.3d"),
            contents: bytemuck::cast_slice(&verts_3d),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let vertex_buffer_2d = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("face3d.vbuf.2d"),
            contents: bytemuck::cast_slice(&verts_2d),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            vertex_buffer_3d,
            vertex_count_3d: verts_3d.len() as u32,
            vertex_buffer_2d,
            vertex_count_2d: verts_2d.len() as u32,
        }
    }
}
