// Triangle mesh model — produced by truck Shell/Solid tessellation.
//
// Stored alongside WireModels in the scene; rendered by the mesh pipeline
// (wgpu TriangleList with depth test, flat normals).

/// A tessellated triangle mesh ready to upload to the GPU.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct MeshModel {
    /// Unique identifier (entity handle value as decimal string).
    pub name: String,
    /// World-space vertex positions (high half of the double-single pair).
    pub verts: Vec<[f32; 3]>,
    /// Low residual paired with `verts` so meshes stay precise at UTM scale.
    /// Empty = all-zero (legacy / interactive meshes near the origin).
    pub verts_low: Vec<[f32; 3]>,
    /// Per-vertex normals (may be empty if not available).
    pub normals: Vec<[f32; 3]>,
    /// Triangle indices into `verts` (every 3 values = one triangle).
    pub indices: Vec<u32>,
    /// RGBA colour in [0, 1].
    pub color: [f32; 4],
    /// Whether this mesh is currently selected.
    pub selected: bool,
}

/// Bundle of mesh tessellations at different sampling densities, picked
/// per frame by the render pipeline based on the projected pixel size of
/// `world_aabb`. Phase 3.4 LOD ladder:
///
/// | LOD | Source     | Use when projected diagonal |
/// |-----|------------|------------------------------|
/// | 0   | HIGH       | > 200 px                     |
/// | 1   | MID (½)    | 50–200 px                    |
/// | 2   | LOW (¼)    | < 50 px                      |
///
/// `lods` holds up to one MeshModel per LOD level (high → low). Empty
/// slots fall back to the nearest available LOD at render time.
#[derive(Clone, Debug)]
pub struct MeshLodSet {
    pub lods: Vec<MeshModel>,
    /// World XY AABB `[min_x, min_y, max_x, max_y]` of the mesh — used
    /// by the per-frame LOD selector to compute the projected pixel
    /// diagonal.
    pub world_aabb: [f32; 4],
    /// World Z extent `[min_z, max_z]`. With `world_aabb` this is the full 3D
    /// box, which the pick path projects to a screen rect to skip solids whose
    /// footprint isn't under the cursor (O(solids) instead of ray-testing every
    /// triangle). `verts` carry only the high half of the double-single
    /// position, so the bound is f32-precise — fine for a conservative cull.
    pub z_aabb: [f32; 2],
}

/// 3D bounds of every LOD's vertices: `([min_x, min_y, max_x, max_y], [min_z, max_z])`.
pub fn compute_mesh_aabb(lods: &[MeshModel]) -> ([f32; 4], [f32; 2]) {
    let (mut min_x, mut min_y, mut min_z) = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y, mut max_z) =
        (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for lod in lods {
        for &[x, y, z] in &lod.verts {
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            min_z = min_z.min(z);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            max_z = max_z.max(z);
        }
    }
    ([min_x, min_y, max_x, max_y], [min_z, max_z])
}

impl MeshLodSet {
    /// Build a set from its LODs, computing the 3D AABB.
    pub fn from_lods(lods: Vec<MeshModel>) -> Self {
        let (world_aabb, z_aabb) = compute_mesh_aabb(&lods);
        Self { lods, world_aabb, z_aabb }
    }

    /// Wrap a single MeshModel as a one-LOD set. Used by interactive
    /// commands that only produce one tessellation (e.g. truck-based
    /// BOX/CYLINDER creation). The LOD selector will pick slot 0 for
    /// every zoom level.
    pub fn from_single(mesh: MeshModel) -> Self {
        Self::from_lods(vec![mesh])
    }

    /// Recompute `world_aabb` / `z_aabb` after the LODs' vertices were rewritten
    /// (relative-to-eye re-split, INSERT transform).
    pub fn recompute_aabb(&mut self) {
        let (xy, z) = compute_mesh_aabb(&self.lods);
        self.world_aabb = xy;
        self.z_aabb = z;
    }
}
