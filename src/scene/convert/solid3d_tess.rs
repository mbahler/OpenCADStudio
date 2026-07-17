// ACIS SAT → MeshModel tessellation for Solid3D (3DSOLID) entities.
//
// Strategy:
//   • plane-surface faces  → collect coedge-loop polygon, fan-triangulate.
//   • cone-surface faces   → sample a parametric grid (handles both cylinders
//                            and true cones).
//   • sphere-surface faces → sample a full UV grid.
//   • torus-surface faces  → sample a full UV grid.
//
// All other surface types are silently skipped; partial results are still
// returned so the solid renders with at least its planar faces.

use rustc_hash::FxHashSet as HashSet;
use std::f64::consts::TAU;

use acadrust::entities::acis::types::Sense;
use acadrust::entities::acis::{
    SabReader, SatCoedge, SatConeSurface, SatDocument, SatEdge, SatEllipseCurve, SatFace, SatLoop,
    SatPlaneSurface, SatPoint, SatPointer, SatSphereSurface, SatTorusSurface, SatVertex,
};
use acadrust::entities::{Body, Region, Solid3D};

use crate::scene::model::mesh_model::{MeshLodSet, MeshModel};

/// Per-LOD sampling density. Higher values = finer mesh = more triangles.
#[derive(Copy, Clone, Debug)]
pub struct LodConfig {
    /// Arc segments per full circle for curved-surface sampling.
    pub circ_segs: usize,
    /// Longitudinal grid count for sphere / torus surfaces.
    pub grid_u: usize,
    /// Latitudinal grid count for sphere / torus surfaces.
    pub grid_v: usize,
}

impl LodConfig {
    /// LOD 0 — full resolution. The pre-Phase-3.4 baseline.
    pub const HIGH: LodConfig = LodConfig {
        circ_segs: 48,
        grid_u: 32,
        grid_v: 16,
    };
    /// LOD 1 — half-resolution. Use between ~50–200 px projected diagonal.
    pub const MID: LodConfig = LodConfig {
        circ_segs: 24,
        grid_u: 16,
        grid_v: 8,
    };
    /// LOD 2 — quarter-resolution. Use below ~50 px.
    pub const LOW: LodConfig = LodConfig {
        circ_segs: 12,
        grid_u: 8,
        grid_v: 4,
    };
    /// Returns the three LOD configs in `[high, mid, low]` order — matches
    /// the `MeshLodSet::lods` slot ordering.
    pub const fn all() -> [LodConfig; 3] {
        [Self::HIGH, Self::MID, Self::LOW]
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Tessellate a SAT document into mesh buffers — shared by all ACIS entities.
fn tessellate_sat(
    sat: &SatDocument,
    name: String,
    color: [f32; 4],
    lod: LodConfig,
) -> Option<MeshModel> {
    let mut verts: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for face in sat.faces() {
        let surf_ptr = face.surface();
        let Some(surf_rec) = sat.resolve(surf_ptr) else {
            continue;
        };
        match surf_rec.entity_type.as_str() {
            "plane-surface" => {
                if let Some(plane) = SatPlaneSurface::from_record(surf_rec) {
                    tess_plane_face(
                        sat,
                        &face,
                        &plane,
                        lod.circ_segs,
                        &mut verts,
                        &mut normals,
                        &mut indices,
                    );
                }
            }
            "cone-surface" => {
                if let Some(cone) = SatConeSurface::from_record(surf_rec) {
                    tess_cone_face(sat, &face, &cone, lod, &mut verts, &mut normals, &mut indices);
                }
            }
            "sphere-surface" => {
                if let Some(sphere) = SatSphereSurface::from_record(surf_rec) {
                    tess_sphere_face(&sphere, lod, &mut verts, &mut normals, &mut indices);
                }
            }
            "torus-surface" => {
                if let Some(torus) = SatTorusSurface::from_record(surf_rec) {
                    tess_torus_face(&torus, lod, &mut verts, &mut normals, &mut indices);
                }
            }
            "spline-surface" => {
                crate::scene::convert::spline_tess::tess_spline_face(
                    sat,
                    &face,
                    lod,
                    &mut verts,
                    &mut normals,
                    &mut indices,
                );
            }
            _ => {}
        }
    }
    if indices.is_empty() {
        return None;
    }
    Some(MeshModel {
        name,
        verts,
        verts_low: Vec::new(),
        normals,
        indices,
        color,
        selected: false,
    })
}

/// Tessellate an ACIS document, preferring the truck B-rep kernel and falling
/// back to the bespoke per-surface sampler when truck can't rebuild the shell
/// (e.g. an unhandled surface type).
fn tessellate_acis(
    sat: &SatDocument,
    name: String,
    color: [f32; 4],
    facet_res: f64,
    isolines: usize,
) -> Option<MeshLodSet> {
    let mut set =
        if let Some(set) = crate::scene::convert::acis_to_truck::tessellate_sat_truck(sat, name.clone(), color, facet_res) {
            if std::env::var_os("OCS_TESS_DEBUG").is_some() {
                let tris = set.lods.first().map(|m| m.indices.len() / 3).unwrap_or(0);
                eprintln!("acis_tess[{name}]: truck ({tris} tris)");
            }
            set
        } else {
            // truck couldn't rebuild the shell — fall back to the bespoke sampler.
            let fallback = tessellate_sat_lods(sat, name.clone(), color, facet_res);
            eprintln!(
                "acis_tess[{name}]: manual fallback ({})",
                if fallback.is_some() { "ok" } else { "empty" }
            );
            fallback?
        };
    // Attach the B-rep face-boundary edges plus ISOLINES on curved faces
    // (body-transformed, split into the double-single pair) so the solid's
    // wireframe shows real edges and curved faces read from any angle.
    attach_feature_edges(&mut set, sat, isolines);
    Some(set)
}

/// Collect the ACIS `edge` records as world-space polyline segments (pairs of
/// endpoints), body-transform them, and store them on the set as the
/// double-single `edge_verts` / `edge_verts_low`.
fn attach_feature_edges(set: &mut MeshLodSet, sat: &SatDocument, isolines: usize) {
    let xform = body_transform(sat);
    let mut seg_pts = collect_feature_edges(sat);
    // ISOLINES ride the same line list and the same body transform as the
    // feature edges, so they inherit the offset / per-INSTANCE re-split for free.
    seg_pts.extend(collect_isolines(sat, isolines));
    set.edge_verts.reserve(seg_pts.len());
    set.edge_verts_low.reserve(seg_pts.len());
    for p in seg_pts {
        let (mut x, mut y, mut z) = (p[0], p[1], p[2]);
        if let Some((m, tr, scale)) = xform {
            let (lx, ly, lz) = (x, y, z);
            x = scale * (lx * m[0] + ly * m[3] + lz * m[6]) + tr[0];
            y = scale * (lx * m[1] + ly * m[4] + lz * m[7]) + tr[1];
            z = scale * (lx * m[2] + ly * m[5] + lz * m[8]) + tr[2];
        }
        let (hx, hy, hz) = (x as f32, y as f32, z as f32);
        set.edge_verts.push([hx, hy, hz]);
        set.edge_verts_low
            .push([(x - hx as f64) as f32, (y - hy as f64) as f32, (z - hz as f64) as f32]);
    }
}

/// ISOLINES line-list endpoints (pairs) for the curved faces of the solid:
/// `count` longitudinal lines spaced across each cone/cylinder face, from its
/// bottom rim to its top rim. These are view-independent tessellation lines
/// (AutoCAD's ISOLINES), so a cylinder reads as a cylinder from any angle
/// rather than showing only its two rim circles. Points are body-local
/// (pre-transform), matching [`collect_feature_edges`], so the caller applies
/// the body transform uniformly.
fn collect_isolines(sat: &SatDocument, count: usize) -> Vec<[f64; 3]> {
    if count == 0 {
        return Vec::new();
    }
    let mut out: Vec<[f64; 3]> = Vec::new();
    for face in sat.faces() {
        let Some(surf_rec) = sat.resolve(face.surface()) else {
            continue;
        };
        if surf_rec.entity_type != "cone-surface" {
            continue;
        }
        let Some(cone) = SatConeSurface::from_record(surf_rec) else {
            continue;
        };
        let (cx, cy, cz) = cone.center();
        let (ax, ay, az) = cone.axis();
        let (ux, uy, uz) = cone.major_axis();
        let radius = cone.radius();
        let sin_a = cone.sin_half_angle();
        let cos_a = cone.cos_half_angle();
        let axis = norm3([ax, ay, az]);
        let u_dir = norm3([ux, uy, uz]);
        let v_dir = cross3(axis, u_dir);

        // Same face-extent recovery as `tess_cone_face`: read the height and
        // angular span from the boundary, and when the boundary is a single
        // closed rim (no height span), recover the top/bottom from the coaxial
        // circle rims.
        let poly = collect_face_polygon(sat, &face, 48);
        let (mut h_min, mut h_max, mut theta_min, mut theta_max, full_circle) =
            angular_range(cx, cy, cz, axis, u_dir, v_dir, &poly);
        if (h_max - h_min).abs() < 1e-9 {
            if let Some((vmin, vmax)) = cone_axis_span(sat, &cone, axis, [cx, cy, cz]) {
                h_min = vmin;
                h_max = vmax;
                if full_circle {
                    theta_min = 0.0;
                    theta_max = TAU;
                }
            }
        }
        let theta_span = if full_circle { TAU } else { theta_max - theta_min };
        if (h_max - h_min).abs() < 1e-10 || theta_span.abs() < 1e-10 {
            continue;
        }
        let r_at = |h: f64| {
            if cos_a.abs() > 1e-9 {
                radius + h * sin_a / cos_a
            } else {
                radius
            }
        };
        let (r0, r1) = (r_at(h_min), r_at(h_max));
        // A closed rim's line at theta and theta+TAU coincide, so step across
        // [0, TAU) with `count` divisions; a bounded arc gets interior lines
        // spanning its own arc (its two ends are already drawn as rim edges).
        for k in 0..count {
            let a = if full_circle {
                theta_min + theta_span * (k as f64 / count as f64)
            } else {
                theta_min + theta_span * ((k as f64 + 1.0) / (count as f64 + 1.0))
            };
            out.push(cone_pt(cx, cy, cz, axis, u_dir, v_dir, r0, a, h_min));
            out.push(cone_pt(cx, cy, cz, axis, u_dir, v_dir, r1, a, h_max));
        }
    }
    out
}

/// Line-list endpoints (pairs) for every `edge` record: straight edges emit
/// their two vertex endpoints; ellipse/circle edges are sampled along their
/// bounded parametric arc. Points are in body-local space (pre-transform).
fn collect_feature_edges(sat: &SatDocument) -> Vec<[f64; 3]> {
    const EDGE_SEGS: usize = 48;
    let mut out: Vec<[f64; 3]> = Vec::new();
    for er in sat.records_of_type("edge") {
        let Some(edge) = SatEdge::from_record(er) else {
            continue;
        };
        // Ordered points along the edge (≥2).
        let mut pts: Vec<[f64; 3]> = Vec::new();
        if let Some(cr) = sat.resolve(edge.curve()) {
            if let Some(ellipse) = SatEllipseCurve::from_record(cr) {
                let reversed = matches!(edge.sense(), Sense::Reversed);
                pts = sample_ellipse_arc(
                    &ellipse,
                    edge.start_param(),
                    edge.end_param(),
                    EDGE_SEGS,
                    reversed,
                );
                // sample_ellipse_arc drops the end param; append the true end so
                // the polyline closes onto the shared vertex.
                if let Some(p) = vertex_point(sat, edge.end_vertex()) {
                    pts.push(p);
                }
            }
        }
        if pts.len() < 2 {
            // Straight edge (or unsampled curve): connect the two vertices.
            pts.clear();
            if let (Some(a), Some(b)) = (
                vertex_point(sat, edge.start_vertex()),
                vertex_point(sat, edge.end_vertex()),
            ) {
                pts.push(a);
                pts.push(b);
            }
        }
        // Emit consecutive points as line-list segment pairs.
        for w in pts.windows(2) {
            out.push(w[0]);
            out.push(w[1]);
        }
    }
    out
}

/// Resolve a vertex pointer to its point coordinates.
fn vertex_point(sat: &SatDocument, vptr: SatPointer) -> Option<[f64; 3]> {
    let vrec = sat.resolve(vptr)?;
    let vertex = SatVertex::from_record(vrec)?;
    let prec = sat.resolve(vertex.point())?;
    let point = SatPoint::from_record(prec)?;
    let (x, y, z) = point.position();
    Some([x, y, z])
}

/// Tessellate a SAT document at all three LODs and bundle them into a
/// `MeshLodSet` ready for the render pipeline to pick a level per frame.
fn tessellate_sat_lods(
    sat: &SatDocument,
    name: String,
    color: [f32; 4],
    facet_res: f64,
) -> Option<MeshLodSet> {
    let configs = LodConfig::all();
    let xform = body_transform(sat);
    let mut lods: Vec<MeshModel> = Vec::with_capacity(3);
    for lod in configs {
        let scaled = scale_lod(lod, facet_res);
        if let Some(mut m) = tessellate_sat(sat, name.clone(), color, scaled) {
            if let Some((mat, tr, scale)) = xform {
                apply_body_transform(&mut m, &mat, &tr, scale);
            }
            lods.push(m);
        }
    }
    if lods.is_empty() {
        return None;
    }
    Some(MeshLodSet::from_lods(lods))
}

/// Extract the body's placement transform from the SAT document: a row-major
/// 3×3 affine, a translation, and the uniform scale. ACIS keeps a solid's
/// geometry in body-local space and records the placement in a `transform`
/// record (`<3×3> <tx ty tz> <scale> rotate reflect shear`). `None` when the
/// document has no transform (treated as identity).
pub(crate) fn body_transform(sat: &SatDocument) -> Option<([f64; 9], [f64; 3], f64)> {
    let t = sat.records.iter().find(|r| r.entity_type == "transform")?;
    // The transform record's numeric payload is its first 13 float-valued
    // tokens: 3×3 matrix, translation, scale. A leading book-keeping pointer
    // (`$-1`) and the trailing rotate/reflect/shear flags aren't floats, so
    // collecting float tokens skips them — reading by raw token index would
    // be thrown off by the leading pointer.
    let v: Vec<f64> = t.tokens.iter().filter_map(|tok| tok.as_float()).take(13).collect();
    if v.len() < 13 {
        return None;
    }
    let m = [v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8]];
    let tr = [v[9], v[10], v[11]];
    Some((m, tr, v[12]))
}

/// Apply a body placement transform to a mesh. ACIS treats points as row
/// vectors (`p' = scale·(p·M) + T`), so the 3×3 is indexed transposed relative
/// to a column-vector multiply. Normals get the rotation only, renormalized.
pub(crate) fn apply_body_transform(mesh: &mut MeshModel, m: &[f64; 9], tr: &[f64; 3], scale: f64) {
    // The body translation `tr` is where a solid gets its world placement, so at
    // UTM scale this is exactly where the coordinate stops fitting in an f32.
    // Compute each world vertex in f64 and split it into the double-single
    // (high, low) pair the mesh shader reconstructs relative to the eye — the
    // same treatment the feature edges already get. Without the low half the
    // shaded faces sit on a ~0.06 m grid while their own edges stay exact, so the
    // surface visibly crawls against its wireframe as the camera moves.
    mesh.verts_low = Vec::with_capacity(mesh.verts.len());
    for i in 0..mesh.verts.len() {
        let [vx, vy, vz] = mesh.verts[i];
        let (x, y, z) = (vx as f64, vy as f64, vz as f64);
        let wx = scale * (x * m[0] + y * m[3] + z * m[6]) + tr[0];
        let wy = scale * (x * m[1] + y * m[4] + z * m[7]) + tr[1];
        let wz = scale * (x * m[2] + y * m[5] + z * m[8]) + tr[2];
        let (hx, hy, hz) = (wx as f32, wy as f32, wz as f32);
        mesh.verts[i] = [hx, hy, hz];
        mesh.verts_low.push([
            (wx - hx as f64) as f32,
            (wy - hy as f64) as f32,
            (wz - hz as f64) as f32,
        ]);
    }
    for n in &mut mesh.normals {
        let (x, y, z) = (n[0] as f64, n[1] as f64, n[2] as f64);
        let nx = x * m[0] + y * m[3] + z * m[6];
        let ny = x * m[1] + y * m[4] + z * m[7];
        let nz = x * m[2] + y * m[5] + z * m[8];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-9 {
            *n = [(nx / len) as f32, (ny / len) as f32, (nz / len) as f32];
        }
    }
}

/// Scale a LOD's segment counts by FACETRES (clamped to AutoCAD's
/// documented [0.01, 10.0] range). 1.0 is the unchanged baseline.
fn scale_lod(base: LodConfig, facet_res: f64) -> LodConfig {
    let m = (facet_res.clamp(0.01, 10.0) as f32).max(0.01);
    let scale = |v: usize| ((v as f32) * m).round().max(4.0) as usize;
    LodConfig {
        circ_segs: scale(base.circ_segs),
        grid_u: scale(base.grid_u),
        grid_v: scale(base.grid_v),
    }
}

/// World-XY AABB of the mesh — used by the render-pipeline LOD selector
/// to pick a level based on projected pixel diagonal.
#[allow(dead_code)] // superseded by mesh_model::compute_mesh_aabb (3D); kept for reference
pub(crate) fn mesh_aabb(mesh: &MeshModel) -> [f32; 4] {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for &[x, y, _] in &mesh.verts {
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        if x < min_x { min_x = x; }
        if y < min_y { min_y = y; }
        if x > max_x { max_x = x; }
        if y > max_y { max_y = y; }
    }
    [min_x, min_y, max_x, max_y]
}

fn parse_acis(
    sat_fn: impl FnOnce() -> Option<SatDocument>,
    is_binary: bool,
    sab_data: &[u8],
) -> Option<SatDocument> {
    if let Some(doc) = sat_fn() {
        return Some(doc);
    }
    if is_binary && !sab_data.is_empty() {
        return SabReader::read(sab_data).ok();
    }
    None
}

/// Tessellate a `Region` entity (2D planar ACIS body) at all three LOD levels.
pub fn tessellate_region(region: &Region, color: [f32; 4], facet_res: f64, isolines: usize) -> Option<MeshLodSet> {
    let sat = parse_acis(
        || region.parse_sat(),
        region.acis_data.is_binary,
        &region.acis_data.sab_data,
    )?;
    let name = region.common.handle.value().to_string();
    tessellate_acis(&sat, name, color, facet_res, isolines)
}

/// Tessellate a `Body` entity (3D ACIS body) at all three LOD levels.
pub fn tessellate_body(body: &Body, color: [f32; 4], facet_res: f64, isolines: usize) -> Option<MeshLodSet> {
    let sat = parse_acis(
        || body.parse_sat(),
        body.acis_data.is_binary,
        &body.acis_data.sab_data,
    )?;
    let name = body.common.handle.value().to_string();
    tessellate_acis(&sat, name, color, facet_res, isolines)
}

/// Tessellate a `Surface` entity (ACAD_SURFACE family) at all three LOD
/// levels. Surfaces are ACIS-backed just like bodies, so the same SAT/SAB
/// path applies — including the B-spline `spline-surface` faces that loft /
/// sweep / revolve produce.
pub fn tessellate_surface(
    surface: &acadrust::entities::Surface,
    color: [f32; 4],
    facet_res: f64,
    isolines: usize,
) -> Option<MeshLodSet> {
    let sat = parse_acis(
        || surface.parse_sat(),
        surface.acis_data.is_binary,
        &surface.acis_data.sab_data,
    )?;
    let name = surface.common.handle.value().to_string();
    tessellate_acis(&sat, name, color, facet_res, isolines)
}

/// Tessellate a `Solid3D` entity at all three LOD levels.
///
/// Returns `None` when the entity has no parseable SAT data or produces no
/// triangles (e.g. the solid uses only unsupported surface types).
/// `facet_res` mirrors the header FACETRES variable (0.01–10.0).
pub fn tessellate_solid3d(solid: &Solid3D, color: [f32; 4], facet_res: f64, isolines: usize) -> Option<MeshLodSet> {
    let sat = parse_acis(
        || solid.parse_sat(),
        solid.acis_data.is_binary,
        &solid.acis_data.sab_data,
    )?;
    let name = solid.common.handle.value().to_string();
    tessellate_acis(&sat, name, color, facet_res, isolines)
}

// ── Topology helpers ──────────────────────────────────────────────────────────

/// Walk a face's outer coedge loop and collect ordered 3-D boundary points.
///
/// Straight edges contribute their start vertex; curved (ellipse / circle)
/// edges are sampled into several points so circular boundaries — e.g. the
/// cap of a cylinder or the rim of a cone — produce a real polygon instead of
/// a single degenerate vertex. `circ_segs` is the sample count for a full
/// circle (scaled down for shorter arcs).
///
/// Returns an empty `Vec` when the loop topology is broken or has fewer than
/// three distinct points.
pub(crate) fn collect_face_polygon(sat: &SatDocument, face: &SatFace, circ_segs: usize) -> Vec<[f64; 3]> {
    let Some(loop_rec) = sat.resolve(face.first_loop()) else {
        return vec![];
    };
    let Some(sat_loop) = SatLoop::from_record(loop_rec) else {
        return vec![];
    };
    collect_loop_polygon(sat, &sat_loop, circ_segs)
}

/// Boundary points of a single coedge loop, in order.
pub(crate) fn collect_loop_polygon(
    sat: &SatDocument,
    sat_loop: &SatLoop,
    circ_segs: usize,
) -> Vec<[f64; 3]> {
    let first_ptr = sat_loop.first_coedge();
    let mut cur = first_ptr;
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let mut visited: HashSet<i32> = HashSet::default();

    loop {
        if cur.is_null() || visited.contains(&cur.0) {
            break;
        }
        visited.insert(cur.0);

        if let Some(ce_rec) = sat.resolve(cur) {
            if let Some(coedge) = SatCoedge::from_record(ce_rec) {
                append_coedge_points(sat, &coedge, circ_segs, &mut pts);
                let next = coedge.next();
                if next == first_ptr {
                    break;
                }
                cur = next;
                continue;
            }
        }
        break;
    }
    pts
}

/// All loops of a face: the outer boundary first, then any inner hole loops.
/// Each loop is returned as an ordered 3-D polygon (≥ 3 points).
#[cfg(feature = "solid3d")]
pub(crate) fn collect_face_loops(
    sat: &SatDocument,
    face: &SatFace,
    circ_segs: usize,
) -> Vec<Vec<[f64; 3]>> {
    let mut loops: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut loop_ptr = face.first_loop();
    let mut seen: HashSet<i32> = HashSet::default();
    while !loop_ptr.is_null() && seen.insert(loop_ptr.0) {
        let Some(loop_rec) = sat.resolve(loop_ptr) else {
            break;
        };
        let Some(sat_loop) = SatLoop::from_record(loop_rec) else {
            break;
        };
        let poly = collect_loop_polygon(sat, &sat_loop, circ_segs);
        if poly.len() >= 3 {
            loops.push(poly);
        }
        loop_ptr = sat_loop.next_loop();
    }
    loops
}

/// Append a coedge's boundary points to `pts`. Ellipse/circle curves are
/// sampled along their parametric arc (excluding the end param so the next
/// coedge's start point provides the junction); all other curve types fall
/// back to the single start vertex, respecting coedge sense.
pub(crate) fn append_coedge_points(
    sat: &SatDocument,
    coedge: &SatCoedge,
    circ_segs: usize,
    pts: &mut Vec<[f64; 3]>,
) {
    let fwd = matches!(coedge.sense(), Sense::Forward);
    let Some(edge_rec) = sat.resolve(coedge.edge()) else {
        return;
    };
    let Some(edge) = SatEdge::from_record(edge_rec) else {
        return;
    };

    if let Some(curve_rec) = sat.resolve(edge.curve()) {
        if let Some(ellipse) = SatEllipseCurve::from_record(curve_rec) {
            // The edge's own sense (relative to its curve) decides the ellipse
            // winding; a reversed edge samples the opposite handedness.
            let reversed = matches!(edge.sense(), Sense::Reversed);
            let mut sampled = sample_ellipse_arc(
                &ellipse,
                edge.start_param(),
                edge.end_param(),
                circ_segs,
                reversed,
            );
            if !sampled.is_empty() {
                if !fwd {
                    sampled.reverse();
                }
                pts.extend(sampled);
                return;
            }
        }
    }

    // Straight / unsupported curve: keep the single start vertex.
    let v_ptr = if fwd {
        edge.start_vertex()
    } else {
        edge.end_vertex()
    };
    if let Some(pt) = resolve_point(sat, v_ptr) {
        pts.push(pt);
    }
}

/// Sample points along an ellipse/circle arc from `sp` to `ep` (radians).
/// Returns points at the start of each segment (the end param is omitted so
/// adjacent coedges don't double up the shared junction point). Segment count
/// scales with the arc's angular span relative to a full circle.
fn sample_ellipse_arc(
    ellipse: &SatEllipseCurve,
    sp: f64,
    ep: f64,
    circ_segs: usize,
    curve_reversed: bool,
) -> Vec<[f64; 3]> {
    let span = ep - sp;
    if span.abs() < 1e-9 {
        return vec![];
    }
    let center = ellipse.center();
    let major = ellipse.major_axis();
    let major_len = (major.0 * major.0 + major.1 * major.1 + major.2 * major.2).sqrt();
    if major_len < 1e-12 {
        return vec![];
    }
    let major_u = [
        major.0 / major_len,
        major.1 / major_len,
        major.2 / major_len,
    ];
    let normal = norm3([ellipse.normal().0, ellipse.normal().1, ellipse.normal().2]);
    let minor_u = cross3(normal, major_u);
    let minor_len = major_len * ellipse.ratio();

    // P(t) = center + major·cos t + (normal×major)·ratio·sin t, with param
    // winding CCW about the curve normal. An edge stored with REVERSED sense
    // traverses the curve backward, i.e. about the opposite normal, so its
    // params index the mirror winding — evaluate with the sin term negated.
    // Ignoring the edge sense mirrors a reversed boundary arc across the major
    // axis to the far side of the circle, ballooning a curved wall into a full
    // cylinder of the surface radius.
    let hand = if curve_reversed { -1.0 } else { 1.0 };

    let segs = (circ_segs as f64 * (span.abs() / TAU)).round().max(2.0) as usize;
    let mut out = Vec::with_capacity(segs);
    for i in 0..segs {
        let t = sp + span * (i as f64 / segs as f64);
        let (c, s) = (t.cos(), t.sin() * hand);
        out.push([
            center.0 + major_u[0] * major_len * c + minor_u[0] * minor_len * s,
            center.1 + major_u[1] * major_len * c + minor_u[1] * minor_len * s,
            center.2 + major_u[2] * major_len * c + minor_u[2] * minor_len * s,
        ]);
    }
    out
}

/// Resolve a vertex pointer all the way to its `[x, y, z]` coordinate.
pub(crate) fn resolve_point(sat: &SatDocument, v_ptr: SatPointer) -> Option<[f64; 3]> {
    let v_rec = sat.resolve(v_ptr)?;
    let vertex = SatVertex::from_record(v_rec)?;
    let pt_rec = sat.resolve(vertex.point())?;
    let point = SatPoint::from_record(pt_rec)?;
    let (x, y, z) = point.position();
    Some([x, y, z])
}

// ── Mesh builder helpers ──────────────────────────────────────────────────────

/// Append one quad (two triangles) to the mesh buffers.
#[inline]
fn push_quad(
    verts: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    p: [[f64; 3]; 4],
    n: [f64; 3],
) {
    let base = verts.len() as u32;
    let nf = [n[0] as f32, n[1] as f32, n[2] as f32];
    for &pt in &p {
        verts.push([pt[0] as f32, pt[1] as f32, pt[2] as f32]);
        normals.push(nf);
    }
    // Two CCW triangles: (0,1,2) and (0,2,3)
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

// ── Planar face ───────────────────────────────────────────────────────────────

pub(crate) fn tess_plane_face(
    sat: &SatDocument,
    face: &SatFace,
    plane: &SatPlaneSurface,
    circ_segs: usize,
    verts: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    let mut poly = collect_face_polygon(sat, face, circ_segs);
    if poly.len() < 3 {
        return;
    }

    let (nx, ny, nz) = plane.normal();
    // Flip normal outward if the face sense is reversed.
    let (nx, ny, nz) = if matches!(face.sense(), Sense::Reversed) {
        (-nx, -ny, -nz)
    } else {
        (nx, ny, nz)
    };
    let nf = [nx as f32, ny as f32, nz as f32];

    if dot3(newell_normal(&poly), [nx, ny, nz]) < 0.0 {
        poly.reverse();
    }

    let base = verts.len() as u32;
    for &pt in &poly {
        verts.push([pt[0] as f32, pt[1] as f32, pt[2] as f32]);
        normals.push(nf);
    }

    // Fan triangulation from vertex 0 (outer loop only; holes are handled by
    // the truck B-rep path in `acis_to_truck`).
    let n = poly.len() as u32;
    for i in 1..(n - 1) {
        indices.extend_from_slice(&[base, base + i, base + i + 1]);
    }
}

// ── Cone / cylinder face ──────────────────────────────────────────────────────

pub(crate) fn tess_cone_face(
    sat: &SatDocument,
    face: &SatFace,
    cone: &SatConeSurface,
    lod: LodConfig,
    verts: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    // Determine the height range and angular span from the boundary polygon.
    let poly = collect_face_polygon(sat, face, lod.circ_segs);

    let (cx, cy, cz) = cone.center();
    let (ax, ay, az) = cone.axis(); // axis direction (unit)
    let (ux, uy, uz) = cone.major_axis(); // u=0 direction
    let radius = cone.radius();
    let sin_a = cone.sin_half_angle();
    let cos_a = cone.cos_half_angle(); // ≈1 for cylinder, <1 for cone

    // Build an orthonormal frame: axis_dir, u_dir, v_dir.
    let axis = norm3([ax, ay, az]);
    let u_dir = norm3([ux, uy, uz]);
    let v_dir = cross3(axis, u_dir);

    // Determine height and angle range from boundary vertices.
    let (mut h_min, mut h_max, mut theta_min, mut theta_max, full_circle) =
        angular_range(cx, cy, cz, axis, u_dir, v_dir, &poly);

    // A full cylinder/cone face is bounded by a single closed rim, so the
    // boundary alone can't span the height — the second extent (top rim or
    // apex) lives on a different face. When the boundary collapses to one
    // height, recover the span from the solid's coaxial circle rims plus the
    // analytic apex of a true cone. Only sweep the full revolution when the
    // boundary really is a closed rim; a bounded arc face (e.g. a curved
    // mullion bar) keeps its own angular span, else it balloons to a circle.
    if (h_max - h_min).abs() < 1e-9 {
        if let Some((vmin, vmax)) = cone_axis_span(sat, cone, axis, [cx, cy, cz]) {
            h_min = vmin;
            h_max = vmax;
            if full_circle {
                theta_min = 0.0;
                theta_max = TAU;
            }
        }
    }

    let theta_span = if full_circle {
        TAU
    } else {
        theta_max - theta_min
    };
    let h_span = h_max - h_min;

    if h_span.abs() < 1e-10 || theta_span.abs() < 1e-10 {
        return;
    }

    // Angular divisions scale with the arc's share of a full circle, so a short
    // boundary arc (a curved wall face) isn't tessellated as finely as a whole
    // rim — matching the wire arc sampler and keeping the triangle budget sane.
    let segs_u = ((lod.circ_segs as f64 * theta_span / TAU).round() as usize).max(1);
    let segs_v = (lod.circ_segs / 4).max(1); // height subdivisions

    for j in 0..segs_v {
        let t0 = h_min + h_span * (j as f64 / segs_v as f64);
        let t1 = h_min + h_span * ((j + 1) as f64 / segs_v as f64);

        for i in 0..segs_u {
            let a0 = theta_min + theta_span * (i as f64 / segs_u as f64);
            let a1 = theta_min + theta_span * ((i + 1) as f64 / segs_u as f64);

            // Cone radius at height t: r(t) = radius + t * sin_a / cos_a
            let r0 = if cos_a.abs() > 1e-9 {
                radius + t0 * sin_a / cos_a
            } else {
                radius
            };
            let r1 = if cos_a.abs() > 1e-9 {
                radius + t1 * sin_a / cos_a
            } else {
                radius
            };

            // Wind the quad so its face (CCW) normal points radially outward,
            // matching the supplied per-vertex normal `n` below. This keeps
            // flat-shaded mode (which derives the normal from winding) and
            // Gouraud mode (which uses `n`) consistent.
            let p = [
                cone_pt(cx, cy, cz, axis, u_dir, v_dir, r0, a0, t0),
                cone_pt(cx, cy, cz, axis, u_dir, v_dir, r0, a1, t0),
                cone_pt(cx, cy, cz, axis, u_dir, v_dir, r1, a1, t1),
                cone_pt(cx, cy, cz, axis, u_dir, v_dir, r1, a0, t1),
            ];

            // Outward normal: perpendicular to axis in the radial direction,
            // tilted by the cone half-angle.
            let mid_a = (a0 + a1) * 0.5;
            let rad_dir = [
                u_dir[0] * mid_a.cos() + v_dir[0] * mid_a.sin(),
                u_dir[1] * mid_a.cos() + v_dir[1] * mid_a.sin(),
                u_dir[2] * mid_a.cos() + v_dir[2] * mid_a.sin(),
            ];
            let n = norm3([
                rad_dir[0] * cos_a - axis[0] * sin_a,
                rad_dir[1] * cos_a - axis[1] * sin_a,
                rad_dir[2] * cos_a - axis[2] * sin_a,
            ]);

            push_quad(verts, normals, indices, p, n);
        }
    }
}

/// Compute a point on a cone/cylinder surface.
#[inline]
fn cone_pt(
    cx: f64,
    cy: f64,
    cz: f64,
    axis: [f64; 3],
    u_dir: [f64; 3],
    v_dir: [f64; 3],
    r: f64,
    theta: f64,
    h: f64,
) -> [f64; 3] {
    [
        cx + r * (u_dir[0] * theta.cos() + v_dir[0] * theta.sin()) + h * axis[0],
        cy + r * (u_dir[1] * theta.cos() + v_dir[1] * theta.sin()) + h * axis[1],
        cz + r * (u_dir[2] * theta.cos() + v_dir[2] * theta.sin()) + h * axis[2],
    ]
}

/// Determine the height range and angular range of a curved face's boundary.
///
/// Returns `(h_min, h_max, theta_min, theta_max, full_circle)`.
/// `full_circle` is true when there are no boundary vertices (e.g. a sphere or
/// a cylinder with no seam edge).
fn angular_range(
    cx: f64,
    cy: f64,
    cz: f64,
    axis: [f64; 3],
    u_dir: [f64; 3],
    v_dir: [f64; 3],
    poly: &[[f64; 3]],
) -> (f64, f64, f64, f64, bool) {
    if poly.is_empty() {
        return (0.0, 0.0, 0.0, TAU, true);
    }

    let mut h_min = f64::MAX;
    let mut h_max = f64::MIN;
    let mut angles: Vec<f64> = Vec::new();

    for &pt in poly {
        let dx = pt[0] - cx;
        let dy = pt[1] - cy;
        let dz = pt[2] - cz;
        let h = dot3([dx, dy, dz], axis);
        h_min = h_min.min(h);
        h_max = h_max.max(h);
        let rv = dot3([dx, dy, dz], v_dir);
        // Project onto the plane perpendicular to the axis.
        let ru = dx * u_dir[0] + dy * u_dir[1] + dz * u_dir[2]
            - h * (axis[0] * u_dir[0] + axis[1] * u_dir[1] + axis[2] * u_dir[2]);
        angles.push(rv.atan2(ru));
    }

    // Find the arc from the LARGEST angular gap, not raw min/max. `atan2`
    // returns [-π, π]; a short arc that straddles the ±π seam splits its points
    // between ≈+π and ≈-π, so naive `max - min` reads ≈2π and the face balloons
    // into a full revolution (a curved wall of radius R drawn as a whole
    // R-cylinder). The empty region a face doesn't cover is its largest gap, so
    // the real arc is the complement: it starts just after the gap and runs to
    // just before it (wrapping past the seam when needed).
    angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = angles.len();
    let (mut max_gap, mut gap_i) = (0.0_f64, 0_usize);
    for i in 0..n {
        let a = angles[i];
        let b = if i + 1 < n { angles[i + 1] } else { angles[0] + TAU };
        let gap = b - a;
        if gap > max_gap {
            max_gap = gap;
            gap_i = i;
        }
    }

    // A genuine full circle has points spread all the way round, so its largest
    // gap is small. A real arc leaves a wide empty wedge.
    let full = max_gap < TAU * 0.05;

    let theta_min = angles[(gap_i + 1) % n];
    let mut theta_max = angles[gap_i];
    if theta_max <= theta_min {
        theta_max += TAU;
    }

    (h_min, h_max, theta_min, theta_max, full)
}

/// Recover a cone/cylinder face's height span (along its axis) from the solid's
/// circular rims when the face boundary collapses to a single height.
///
/// Scans every ellipse/circle curve in the document, keeps those coaxial with
/// this cone (centre on the axis line, normal parallel to the axis), and
/// projects their centres onto the axis to get rim heights. For a true cone
/// with a single rim, the tip is added analytically (the height where the
/// radius reaches zero). Returns `None` when no coaxial rim is found.
pub(crate) fn cone_axis_span(
    sat: &SatDocument,
    cone: &SatConeSurface,
    axis: [f64; 3],
    center: [f64; 3],
) -> Option<(f64, f64)> {
    let mut heights: Vec<f64> = Vec::new();
    for rec in &sat.records {
        if rec.entity_type != "ellipse-curve" {
            continue;
        }
        let Some(e) = SatEllipseCurve::from_record(rec) else {
            continue;
        };
        let ec = e.center();
        let d = [ec.0 - center[0], ec.1 - center[1], ec.2 - center[2]];
        let h = dot3(d, axis);
        // Radial offset from the axis line: must be ~0 to be coaxial.
        let radial = [
            d[0] - h * axis[0],
            d[1] - h * axis[1],
            d[2] - h * axis[2],
        ];
        let radial_len = dot3(radial, radial).sqrt();
        let n = e.normal();
        let n_dot = dot3(norm3([n.0, n.1, n.2]), axis).abs();
        if radial_len < 1e-6 && n_dot > 0.999 {
            heights.push(h);
        }
    }
    if heights.is_empty() {
        return None;
    }
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    heights.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

    let mut h_min = heights[0];
    let mut h_max = *heights.last().unwrap();

    // True cone with a single rim: close the surface at its apex (r = 0).
    let sin_a = cone.sin_half_angle();
    let cos_a = cone.cos_half_angle();
    if sin_a.abs() > 1e-6 && heights.len() <= 1 {
        let apex = -cone.radius() * cos_a / sin_a;
        h_min = h_min.min(apex);
        h_max = h_max.max(apex);
    }

    if (h_max - h_min).abs() < 1e-9 {
        return None;
    }
    Some((h_min, h_max))
}

// ── Sphere face ───────────────────────────────────────────────────────────────

pub(crate) fn tess_sphere_face(
    sphere: &SatSphereSurface,
    lod: LodConfig,
    verts: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    let (cx, cy, cz) = sphere.center();
    let r = sphere.radius();
    let (px, py, pz) = sphere.pole(); // north-pole direction
    let pole = norm3([px, py, pz]);
    let (ux, uy, uz) = sphere.u_direction();
    let u_dir = norm3([ux, uy, uz]);
    let v_dir = cross3(pole, u_dir);

    let nu = lod.grid_u.max(3);
    let nv = lod.grid_v.max(2);

    for j in 0..nv {
        let phi0 = std::f64::consts::PI * (j as f64 / nv as f64); // 0..π
        let phi1 = std::f64::consts::PI * ((j + 1) as f64 / nv as f64);

        for i in 0..nu {
            let theta0 = TAU * (i as f64 / nu as f64);
            let theta1 = TAU * ((i + 1) as f64 / nu as f64);

            let n00 = sphere_dir(pole, u_dir, v_dir, theta0, phi0);
            let n10 = sphere_dir(pole, u_dir, v_dir, theta0, phi1);
            let n11 = sphere_dir(pole, u_dir, v_dir, theta1, phi1);
            let n01 = sphere_dir(pole, u_dir, v_dir, theta1, phi0);

            let p = [
                [cx + r * n00[0], cy + r * n00[1], cz + r * n00[2]],
                [cx + r * n10[0], cy + r * n10[1], cz + r * n10[2]],
                [cx + r * n11[0], cy + r * n11[1], cz + r * n11[2]],
                [cx + r * n01[0], cy + r * n01[1], cz + r * n01[2]],
            ];

            // Average outward normal for the quad.
            let nav = norm3([
                n00[0] + n10[0] + n11[0] + n01[0],
                n00[1] + n10[1] + n11[1] + n01[1],
                n00[2] + n10[2] + n11[2] + n01[2],
            ]);

            push_quad(verts, normals, indices, p, nav);
        }
    }
}

#[inline]
fn sphere_dir(pole: [f64; 3], u_dir: [f64; 3], v_dir: [f64; 3], theta: f64, phi: f64) -> [f64; 3] {
    let sin_phi = phi.sin();
    let cos_phi = phi.cos();
    let cos_theta = theta.cos();
    let sin_theta = theta.sin();
    // pole × cos_phi + (u*cos_theta + v*sin_theta) × sin_phi
    [
        pole[0] * cos_phi + (u_dir[0] * cos_theta + v_dir[0] * sin_theta) * sin_phi,
        pole[1] * cos_phi + (u_dir[1] * cos_theta + v_dir[1] * sin_theta) * sin_phi,
        pole[2] * cos_phi + (u_dir[2] * cos_theta + v_dir[2] * sin_theta) * sin_phi,
    ]
}

// ── Torus face ────────────────────────────────────────────────────────────────

pub(crate) fn tess_torus_face(
    torus: &SatTorusSurface,
    lod: LodConfig,
    verts: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    let (cx, cy, cz) = torus.center();
    let (nx, ny, nz) = torus.normal();
    let axis = norm3([nx, ny, nz]); // revolution axis
    let (ux, uy, uz) = torus.u_direction();
    let u_dir = norm3([ux, uy, uz]);
    let v_dir = cross3(axis, u_dir);
    let major_r = torus.major_radius();
    let minor_r = torus.minor_radius();

    let nu = lod.grid_u.max(3); // around the tube
    let nv = lod.grid_v.max(3); // around the torus

    for j in 0..nv {
        let phi0 = TAU * (j as f64 / nv as f64);
        let phi1 = TAU * ((j + 1) as f64 / nv as f64);

        for i in 0..nu {
            let theta0 = TAU * (i as f64 / nu as f64);
            let theta1 = TAU * ((i + 1) as f64 / nu as f64);

            let p = [
                torus_pt(
                    cx, cy, cz, axis, u_dir, v_dir, major_r, minor_r, theta0, phi0,
                ),
                torus_pt(
                    cx, cy, cz, axis, u_dir, v_dir, major_r, minor_r, theta0, phi1,
                ),
                torus_pt(
                    cx, cy, cz, axis, u_dir, v_dir, major_r, minor_r, theta1, phi1,
                ),
                torus_pt(
                    cx, cy, cz, axis, u_dir, v_dir, major_r, minor_r, theta1, phi0,
                ),
            ];

            // Outward tube normal.
            let mid_phi = (phi0 + phi1) * 0.5;
            let mid_theta = (theta0 + theta1) * 0.5;
            // Direction from tube center to surface point.
            let radial = [
                u_dir[0] * mid_phi.cos() + v_dir[0] * mid_phi.sin(),
                u_dir[1] * mid_phi.cos() + v_dir[1] * mid_phi.sin(),
                u_dir[2] * mid_phi.cos() + v_dir[2] * mid_phi.sin(),
            ];
            let n = norm3([
                radial[0] * mid_theta.cos() + axis[0] * mid_theta.sin(),
                radial[1] * mid_theta.cos() + axis[1] * mid_theta.sin(),
                radial[2] * mid_theta.cos() + axis[2] * mid_theta.sin(),
            ]);

            push_quad(verts, normals, indices, p, n);
        }
    }
}

#[inline]
fn torus_pt(
    cx: f64,
    cy: f64,
    cz: f64,
    axis: [f64; 3],
    u_dir: [f64; 3],
    v_dir: [f64; 3],
    major_r: f64,
    minor_r: f64,
    theta: f64, // tube angle
    phi: f64,   // revolution angle
) -> [f64; 3] {
    // Ring center at angle phi.
    let ring = [
        cx + major_r * (u_dir[0] * phi.cos() + v_dir[0] * phi.sin()),
        cy + major_r * (u_dir[1] * phi.cos() + v_dir[1] * phi.sin()),
        cz + major_r * (u_dir[2] * phi.cos() + v_dir[2] * phi.sin()),
    ];
    // Radial direction from torus axis to ring center.
    let radial = norm3([ring[0] - cx, ring[1] - cy, ring[2] - cz]);
    // Point on tube.
    [
        ring[0] + minor_r * (radial[0] * theta.cos() + axis[0] * theta.sin()),
        ring[1] + minor_r * (radial[1] * theta.cos() + axis[1] * theta.sin()),
        ring[2] + minor_r * (radial[2] * theta.cos() + axis[2] * theta.sin()),
    ]
}

// ── Math helpers ──────────────────────────────────────────────────────────────

#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Area-weighted polygon normal via Newell's method. Robust for non-planar or
/// slightly noisy loops; its sign encodes the winding direction.
fn newell_normal(poly: &[[f64; 3]]) -> [f64; 3] {
    let mut n = [0.0f64; 3];
    let len = poly.len();
    for i in 0..len {
        let a = poly[i];
        let b = poly[(i + 1) % len];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    n
}

#[inline]
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn norm3(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}
