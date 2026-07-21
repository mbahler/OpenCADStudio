//! CPU-side hit-testing for wire geometry.
//!
//! All tests are performed in **screen space** — wire vertices are projected
//! to 2-D pixel coordinates, then compared against the cursor or selection box.
//! This matches the visual result the user sees.

use rustc_hash::FxHashMap as HashMap;

use acadrust::Handle;
use glam::Mat4;
use iced::{Point, Rectangle};

use crate::scene::model::hatch_model::HatchModel;
use crate::scene::model::mesh_model::MeshModel;
use crate::scene::model::wire_model::WireModel;

/// Pixel radius used for single-click wire detection.
pub const CLICK_THRESHOLD_PX: f32 = 8.0;

/// Pick radius for one wire, in screen pixels.
///
/// A wire renders as a band `line_weight_px` wide, so testing every wire at the
/// bare [`CLICK_THRESHOLD_PX`] would leave the outer part of a heavy line
/// unselectable — the cursor would sit on solid ink and miss. Widening to the
/// rendered half-width keeps "looks like I'm on it" and "picks it" the same
/// thing at any zoom: both quantities are screen-space, so the relation holds
/// however far in the view is.
///
/// `lw_display` mirrors the wire shader's `select(0.5, half_width, ...)`
/// (`wire.wgsl`) — with lineweight display off the line collapses to 1 px, so
/// the pick band must collapse with it rather than stay secretly fat.
///
/// The standard weights all land under the threshold today (the widest, 2.11 mm,
/// renders 7.97 px half-width), so this only bites for out-of-range weights —
/// and it keeps the two sides from silently drifting apart if the display boost
/// in `view::render::lineweight_to_px` ever changes.
pub fn pick_tolerance_px(wire: &WireModel, lw_display: bool) -> f32 {
    let half_width = if lw_display {
        wire.line_weight_px * 0.5
    } else {
        0.5
    };
    CLICK_THRESHOLD_PX.max(half_width)
}

/// Is `aabb` — a wire's world-space XY box — further than `tol` pixels from
/// `cursor` once projected, so the wire can be skipped without touching its
/// geometry?
///
/// Only sound in a flat (untilted) view, where a point's screen x/y depends on
/// its world x/y alone and the box therefore projects exactly. Callers must
/// check that themselves, and must skip the unbounded sentinel.
fn aabb_rejects(
    aabb: [f32; 4],
    cursor: Point,
    tol: f32,
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> bool {
    let [minx, miny, maxx, maxy] = aabb;
    // Project all four corners — a plan view can be rotated about Z, so the
    // screen footprint isn't axis-aligned and the two diagonal corners alone
    // wouldn't bound it.
    let (mut sx0, mut sy0, mut sx1, mut sy1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (cx, cy) in [(minx, miny), (maxx, miny), (maxx, maxy), (minx, maxy)] {
        let s = world_to_screen(
            glam::DVec3::new(cx as f64, cy as f64, 0.0),
            view_rot,
            eye,
            bounds,
        );
        sx0 = sx0.min(s.x);
        sx1 = sx1.max(s.x);
        sy0 = sy0.min(s.y);
        sy1 = sy1.max(s.y);
    }
    cursor.x < sx0 - tol || cursor.x > sx1 + tol || cursor.y < sy0 - tol || cursor.y > sy1 + tol
}

/// Depth of the first triangle in `tris` whose screen projection contains
/// `cursor`, as the mean NDC z of its corners; `None` when none do.
///
/// `tris` is a flat vertex list, 3 per triangle, and `tris_low` its
/// double-single residual — empty meaning an all-zero low half, per the
/// [`WireModel`] contract.
fn tris_hit_depth(
    cursor: Point,
    tris: &[[f32; 3]],
    tris_low: &[[f32; 3]],
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Option<f32> {
    let mut t = 0;
    while t + 2 < tris.len() {
        let mut sp = [Point::ORIGIN; 3];
        let mut depth = 0.0f32;
        for j in 0..3 {
            let k = t + j;
            let hi = tris[k];
            let lo = tris_low.get(k).copied().unwrap_or([0.0; 3]);
            let world = glam::DVec3::new(
                hi[0] as f64 + lo[0] as f64,
                hi[1] as f64 + lo[1] as f64,
                hi[2] as f64 + lo[2] as f64,
            );
            let ndc = view_rot.project_point3((world - eye).as_vec3());
            sp[j] = Point::new(
                (ndc.x + 1.0) * 0.5 * bounds.width,
                (1.0 - ndc.y) * 0.5 * bounds.height,
            );
            depth += ndc.z;
        }
        t += 3;
        if point_in_polygon(cursor, &sp) {
            return Some(depth / 3.0);
        }
    }
    None
}

// ── Single-click hit test ─────────────────────────────────────────────────

/// Return the `name` of the closest wire whose screen-space segments pass
/// within that wire's [`pick_tolerance_px`] of `cursor`.
///
/// Returns `None` when no wire is close enough.
pub fn click_hit<'a>(
    cursor: Point,
    wires: &'a [WireModel],
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
    lw_display: bool,
) -> Option<&'a str> {
    // A click outside the pane rectangle (e.g. on the paper around a floating
    // viewport) must not reach geometry scissored out of the viewport.
    if cursor.x < 0.0 || cursor.x > bounds.width || cursor.y < 0.0 || cursor.y > bounds.height {
        return None;
    }
    // Each wire brings its own threshold now (a heavy line catches over its full
    // rendered width), so the running best can't double as the cut-off.
    let mut best_dist = f32::MAX;
    let mut best: Option<&str> = None;

    // World z only shifts the *screen* x/y when the view is tilted (orbit /
    // perspective). In the flat top-down ortho view — the case where hover lag
    // on large drawings actually bites — a wire's screen position depends only
    // on its world x/y, so its world-space AABB projects exactly and we can
    // reject wires nowhere near the cursor without projecting any of their
    // points (the dominant per-move cost on 100 k-wire drawings).
    let z_flat = view_rot.z_axis.x.abs() < 1e-9 && view_rot.z_axis.y.abs() < 1e-9;

    // Q: lazy projection — no Vec allocation per wire; NaN resets the segment chain.
    for wire in wires {
        let tol = pick_tolerance_px(wire, lw_display);
        // Cheap AABB pre-reject (flat view only; never for the unbounded
        // sentinel used by previews / greeked text).
        if z_flat
            && wire.aabb != WireModel::UNBOUNDED_AABB
            && aabb_rejects(wire.aabb, cursor, tol, view_rot, eye, bounds)
        {
            continue;
        }
        let mut prev: Option<Point> = None;
        for (i, &[px, py, pz]) in wire.points.iter().enumerate() {
            if px.is_nan() {
                prev = None;
                continue;
            }
            let cur = world_to_screen(wp64([px, py, pz], &wire.points_low, i), view_rot, eye, bounds);
            if let Some(p0) = prev {
                let d = dist_point_to_segment(cursor, p0, cur);
                if d < tol && d < best_dist {
                    best_dist = d;
                    best = Some(&wire.name);
                }
            }
            prev = Some(cur);
        }
    }

    if best.is_some() {
        return best;
    }

    // No edge close enough. Mesh entities (PolyfaceMesh / PolygonMesh / SubD
    // Mesh) carry their shaded faces as `fill_tris`; test those so a mesh is
    // selectable by clicking its surface — not only its thin edges — the way a
    // 3D solid is. Same projected-triangle containment as `mesh_click_hit`,
    // front-most wins.
    let mut best_fill: Option<(f32, &str)> = None;
    for wire in wires {
        if wire.fill_tris.is_empty() {
            continue;
        }
        if let Some(d) = tris_hit_depth(
            cursor,
            &wire.fill_tris,
            &wire.fill_tris_low,
            view_rot,
            eye,
            bounds,
        ) {
            if best_fill.map_or(true, |(bd, _)| d < bd) {
                best_fill = Some((d, wire.name.as_str()));
            }
        }
    }
    if let Some((_, n)) = best_fill {
        return Some(n);
    }

    // No fill of this wire's own either. `pick_tris` closes the surfaces that
    // `points` only bounds: a thickness wall (drawn as four edges with nothing
    // between them) and a wide polyline's band (drawn, but by the hatch
    // pipeline, so no fill hangs off this wire). Without them the cursor falls
    // through what plainly reads as solid. Front-most wins.
    //
    // Ranked below `fill_tris` because that geometry is this wire's own drawn
    // surface — where the two overlap, the nearer thing to the eye is decided
    // by depth, but a wire that has a real fill should win on it first.
    let mut best_wall: Option<(f32, &str)> = None;
    for wire in wires {
        if wire.pick_tris.is_empty() {
            continue;
        }
        // This runs on every hover that misses everything else — the common case
        // over empty space — and a wall is two triangles per base segment, so an
        // extruded circle alone is ~128 of them. Reject on the box first or a
        // drawing full of thickness turns every mouse move into a projection of
        // its every wall.
        if z_flat
            && wire.aabb != WireModel::UNBOUNDED_AABB
            && aabb_rejects(wire.aabb, cursor, 0.0, view_rot, eye, bounds)
        {
            continue;
        }
        if let Some(d) = tris_hit_depth(
            cursor,
            &wire.pick_tris,
            &wire.pick_tris_low,
            view_rot,
            eye,
            bounds,
        ) {
            if best_wall.map_or(true, |(bd, _)| d < bd) {
                best_wall = Some((d, wire.name.as_str()));
            }
        }
    }
    if let Some((_, n)) = best_wall {
        return Some(n);
    }

    // SDF text renders as glyph quads, not strokes — its only pick target is
    // the empty wire that carries the quads (`text_verts`) and the text's own
    // tight AABB. Fall back to AABB containment so such text stays click-
    // selectable, and use `text_verts` as the exact discriminator so only real
    // text boxes qualify (never some other empty wire). Lowest priority — real
    // edges and fills above always win. Prefer the tightest box so a click over
    // overlapping text picks the smallest.
    let mut best_area = f32::MAX;
    let mut best_box: Option<&str> = None;
    for wire in wires {
        if wire.text_verts.is_empty()
            || !wire.points.is_empty()
            || !wire.fill_tris.is_empty()
            || wire.aabb == WireModel::UNBOUNDED_AABB
        {
            continue;
        }
        let [minx, miny, maxx, maxy] = wire.aabb;
        let (mut sx0, mut sy0, mut sx1, mut sy1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (cx, cy) in [(minx, miny), (maxx, miny), (maxx, maxy), (minx, maxy)] {
            let s = world_to_screen(glam::DVec3::new(cx as f64, cy as f64, 0.0), view_rot, eye, bounds);
            sx0 = sx0.min(s.x);
            sx1 = sx1.max(s.x);
            sy0 = sy0.min(s.y);
            sy1 = sy1.max(s.y);
        }
        if cursor.x >= sx0 && cursor.x <= sx1 && cursor.y >= sy0 && cursor.y <= sy1 {
            let area = (sx1 - sx0) * (sy1 - sy0);
            if area < best_area {
                best_area = area;
                best_box = Some(wire.name.as_str());
            }
        }
    }
    best_box
}

/// Like `click_hit` but returns every wire within the click threshold,
/// nearest first. Used by selection cycling to step through overlapping
/// objects under the cursor.
pub fn click_hits_all<'a>(
    cursor: Point,
    wires: &'a [WireModel],
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
    lw_display: bool,
) -> Vec<&'a str> {
    if cursor.x < 0.0 || cursor.x > bounds.width || cursor.y < 0.0 || cursor.y > bounds.height {
        return Vec::new();
    }
    let mut hits: Vec<(f32, &str)> = Vec::new();
    for wire in wires {
        let tol = pick_tolerance_px(wire, lw_display);
        let mut prev: Option<Point> = None;
        let mut best_for_wire = tol;
        let mut hit = false;
        for (i, &[px, py, pz]) in wire.points.iter().enumerate() {
            if px.is_nan() {
                prev = None;
                continue;
            }
            let cur = world_to_screen(wp64([px, py, pz], &wire.points_low, i), view_rot, eye, bounds);
            if let Some(p0) = prev {
                let d = dist_point_to_segment(cursor, p0, cur);
                if d < best_for_wire {
                    best_for_wire = d;
                    hit = true;
                }
            }
            prev = Some(cur);
        }
        if hit {
            hits.push((best_for_wire, &wire.name));
        }
    }
    // Thickness walls join the cycle so an extruded entity picked on its wall
    // can be stepped past to whatever sits behind it. Ranked at the threshold,
    // below every proximity hit — same convention the text boxes below use.
    //
    // A wall's own edges live on the same wire, so skip any wire the loop above
    // already caught: cycling must not offer one entity twice.
    for wire in wires {
        if wire.pick_tris.is_empty() || hits.iter().any(|&(_, n)| n == wire.name) {
            continue;
        }
        if tris_hit_depth(
            cursor,
            &wire.pick_tris,
            &wire.pick_tris_low,
            view_rot,
            eye,
            bounds,
        )
        .is_some()
        {
            hits.push((CLICK_THRESHOLD_PX, &wire.name));
        }
    }
    // SDF text: include a text whose box contains the cursor (an empty-stroke
    // wire carrying glyph quads) so selection cycling steps through text too —
    // the same discriminator/fallback `click_hit` uses. Ranked after real
    // geometry (distance = the click threshold, above every proximity hit).
    for wire in wires {
        if wire.text_verts.is_empty()
            || !wire.points.is_empty()
            || !wire.fill_tris.is_empty()
            || wire.aabb == WireModel::UNBOUNDED_AABB
        {
            continue;
        }
        let [minx, miny, maxx, maxy] = wire.aabb;
        let (mut sx0, mut sy0, mut sx1, mut sy1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (cx, cy) in [(minx, miny), (maxx, miny), (maxx, maxy), (minx, maxy)] {
            let s = world_to_screen(
                glam::DVec3::new(cx as f64, cy as f64, 0.0),
                view_rot,
                eye,
                bounds,
            );
            sx0 = sx0.min(s.x);
            sx1 = sx1.max(s.x);
            sy0 = sy0.min(s.y);
            sy1 = sy1.max(s.y);
        }
        if cursor.x >= sx0 && cursor.x <= sx1 && cursor.y >= sy0 && cursor.y <= sy1 {
            hits.push((CLICK_THRESHOLD_PX, &wire.name));
        }
    }
    hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    hits.into_iter().map(|(_, name)| name).collect()
}

/// Pick a 3D solid by clicking anywhere on its shaded body: project each
/// mesh triangle to screen space and test whether the cursor lands inside it.
/// Returns the front-most hit (smallest projected depth). Lets meshed solids
/// (whose only wire geometry is thin edges) be selected on their faces.
/// Conservative broad-phase: does the solid's 3D AABB project to a screen rect
/// that the cursor falls within (± `CLICK_THRESHOLD_PX`)? Lets the caller skip
/// ray-testing the triangles of solids whose footprint isn't under the cursor —
/// O(solids) cheap projections instead of O(triangles) per hover. Returns
/// `true` (don't cull) whenever any corner is at/behind the camera, where the
/// projected rect can't be trusted.
///
/// `world_aabb` is `[min_x, min_y, max_x, max_y]`, `z_aabb` is `[min_z, max_z]`,
/// and `view_proj` is the same matrix `mesh_click_hit` projects vertices with.
pub fn aabb_under_cursor(
    world_aabb: [f32; 4],
    z_aabb: [f32; 2],
    cursor: Point,
    view_proj: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> bool {
    let [min_x, min_y, max_x, max_y] = world_aabb;
    let [min_z, max_z] = z_aabb;
    if !min_x.is_finite() || !min_z.is_finite() {
        return true; // degenerate bound — don't cull
    }
    let (mut sx0, mut sy0, mut sx1, mut sy1) =
        (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &z in &[min_z, max_z] {
        for &(x, y) in &[(min_x, min_y), (max_x, min_y), (min_x, max_y), (max_x, max_y)] {
            let rel = glam::DVec3::new(x as f64 - eye.x, y as f64 - eye.y, z as f64 - eye.z)
                .as_vec3();
            let clip = view_proj * rel.extend(1.0);
            if clip.w <= 1e-6 {
                return true; // corner at/behind the camera — can't bound; keep it
            }
            let ndc = clip.truncate() / clip.w;
            let px = (ndc.x + 1.0) * 0.5 * bounds.width;
            let py = (1.0 - ndc.y) * 0.5 * bounds.height;
            sx0 = sx0.min(px);
            sy0 = sy0.min(py);
            sx1 = sx1.max(px);
            sy1 = sy1.max(py);
        }
    }
    let m = CLICK_THRESHOLD_PX;
    cursor.x >= sx0 - m && cursor.x <= sx1 + m && cursor.y >= sy0 - m && cursor.y <= sy1 + m
}

pub fn mesh_click_hit<'a>(
    cursor: Point,
    meshes: impl Iterator<Item = (Handle, &'a MeshModel)>,
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Option<Handle> {
    let mut best: Option<(f32, Handle)> = None;
    for (handle, mesh) in meshes {
        let v = &mesh.verts;
        let idx = &mesh.indices;
        let lo = &mesh.verts_low;
        let mut t = 0;
        while t + 2 < idx.len() {
            let tri = [idx[t] as usize, idx[t + 1] as usize, idx[t + 2] as usize];
            t += 3;
            let mut sp = [Point::ORIGIN; 3];
            let mut depth = 0.0f32;
            for (j, &k) in tri.iter().enumerate() {
                let ndc = view_rot.project_point3((mesh_vert(v[k], lo, k) - eye).as_vec3());
                sp[j] = Point::new(
                    (ndc.x + 1.0) * 0.5 * bounds.width,
                    (1.0 - ndc.y) * 0.5 * bounds.height,
                );
                depth += ndc.z;
            }
            if point_in_polygon(cursor, &sp) {
                let d = depth / 3.0;
                if best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, handle));
                }
                break; // one hit per mesh is enough
            }
        }
    }
    best.map(|(_, h)| h)
}

/// Reconstruct a mesh vertex's absolute f64 position from its high/low pair —
/// without the low residual the f32 high alone is ~0.5 m off at UTM scale and
/// box / lasso / face selection lands on the wrong place.
#[inline]
fn mesh_vert(hi: [f32; 3], low: &[[f32; 3]], i: usize) -> glam::DVec3 {
    let l = low.get(i).copied().unwrap_or([0.0; 3]);
    glam::DVec3::new(
        hi[0] as f64 + l[0] as f64,
        hi[1] as f64 + l[1] as f64,
        hi[2] as f64 + l[2] as f64,
    )
}

/// Project a mesh's vertices to screen space.
fn project_mesh_verts(mesh: &MeshModel, view_rot: Mat4, eye: glam::DVec3, bounds: Rectangle) -> Vec<Point> {
    mesh.verts
        .iter()
        .enumerate()
        .map(|(i, &w)| {
            let ndc = view_rot.project_point3((mesh_vert(w, &mesh.verts_low, i) - eye).as_vec3());
            Point::new(
                (ndc.x + 1.0) * 0.5 * bounds.width,
                (1.0 - ndc.y) * 0.5 * bounds.height,
            )
        })
        .collect()
}

/// True when any of `mesh`'s projected triangles contains one of `pts`
/// (used so a crossing box / lasso entirely inside a solid still selects it).
fn mesh_covers_any(proj: &[Point], indices: &[u32], pts: &[Point]) -> bool {
    let mut t = 0;
    while t + 2 < indices.len() {
        let tri = [
            proj[indices[t] as usize],
            proj[indices[t + 1] as usize],
            proj[indices[t + 2] as usize],
        ];
        t += 3;
        if pts.iter().any(|p| point_in_polygon(*p, &tri)) {
            return true;
        }
    }
    false
}

/// Solid (mesh) handles caught by a rectangular selection box. Window mode
/// (`crossing == false`) needs every projected vertex inside the box;
/// crossing mode needs any vertex inside, or the box to sit inside the solid.
pub fn mesh_box_hit<'a>(
    a: Point,
    b: Point,
    crossing: bool,
    meshes: impl Iterator<Item = (Handle, &'a MeshModel)>,
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Vec<Handle> {
    let (min_x, max_x) = (a.x.min(b.x), a.x.max(b.x));
    let (min_y, max_y) = (a.y.min(b.y), a.y.max(b.y));
    let in_box = |p: &Point| p.x >= min_x && p.x <= max_x && p.y >= min_y && p.y <= max_y;
    let corners = [
        Point::new(min_x, min_y),
        Point::new(max_x, min_y),
        Point::new(max_x, max_y),
        Point::new(min_x, max_y),
    ];
    let mut out = Vec::new();
    for (h, mesh) in meshes {
        let proj = project_mesh_verts(mesh, view_rot, eye, bounds);
        if proj.is_empty() {
            continue;
        }
        let hit = if crossing {
            proj.iter().any(in_box) || mesh_covers_any(&proj, &mesh.indices, &corners)
        } else {
            proj.iter().all(in_box)
        };
        if hit {
            out.push(h);
        }
    }
    out
}

/// Solid (mesh) handles caught by a lasso polygon. Window mode needs every
/// projected vertex inside the lasso; crossing mode needs any vertex inside,
/// or the lasso to sit inside the solid.
pub fn mesh_poly_hit<'a>(
    poly: &[Point],
    crossing: bool,
    meshes: impl Iterator<Item = (Handle, &'a MeshModel)>,
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Vec<Handle> {
    if poly.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (h, mesh) in meshes {
        let proj = project_mesh_verts(mesh, view_rot, eye, bounds);
        if proj.is_empty() {
            continue;
        }
        let hit = if crossing {
            proj.iter().any(|p| point_in_polygon(*p, poly))
                || mesh_covers_any(&proj, &mesh.indices, poly)
        } else {
            proj.iter().all(|p| point_in_polygon(*p, poly))
        };
        if hit {
            out.push(h);
        }
    }
    out
}

// ── Box / window selection ────────────────────────────────────────────────

/// Return the names of wires selected by a completed rectangular selection box.
///
/// - **Window mode** (`crossing = false`, left→right drag):
///   ALL projected points must lie inside the box.
/// - **Crossing mode** (`crossing = true`, right→left drag):
///   ANY projected point inside the box, OR any wire segment crosses the box
///   boundary (so large entities like viewport frames are caught even when
///   no corner falls inside the selection rectangle).
pub fn box_hit<'a>(
    corner_a: Point,
    corner_b: Point,
    crossing: bool,
    wires: &'a [WireModel],
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Vec<&'a str> {
    // Clamp the selection box to the pane rectangle so it can't reach geometry
    // the GPU scissored out of a floating viewport (the hit-test wire set runs
    // past the visible rect). No-op in model space, where bounds is the canvas.
    let min_x = corner_a.x.min(corner_b.x).max(0.0);
    let max_x = corner_a.x.max(corner_b.x).min(bounds.width);
    let min_y = corner_a.y.min(corner_b.y).max(0.0);
    let max_y = corner_a.y.max(corner_b.y).min(bounds.height);

    // Ignore zero-area boxes (including a box clamped entirely off-pane).
    if (max_x - min_x) < 1.0 || (max_y - min_y) < 1.0 {
        return vec![];
    }

    let inside = |sp: Point| sp.x >= min_x && sp.x <= max_x && sp.y >= min_y && sp.y <= max_y;

    // Box corners for segment-intersection tests (crossing mode only).
    let box_tl = Point { x: min_x, y: min_y };
    let box_tr = Point { x: max_x, y: min_y };
    let box_bl = Point { x: min_x, y: max_y };
    let box_br = Point { x: max_x, y: max_y };

    // Q: lazy projection — accumulate screen points without allocating per-wire Vec.
    wires
        .iter()
        .filter_map(|wire| {
            // Fallback: when wire has no line geometry (e.g. greek text emits
            // only fill_tris) treat the AABB rectangle as the hit-test shape
            // so low-LOD text stays selectable. See #19.
            let aabb_pts: Vec<[f32; 3]>;
            let pts: &[[f32; 3]] = if !wire.points.is_empty() {
                &wire.points
            } else if wire.aabb != WireModel::UNBOUNDED_AABB {
                let [ax, ay, bx, by] = wire.aabb;
                aabb_pts = vec![
                    [ax, ay, 0.0],
                    [bx, ay, 0.0],
                    [bx, by, 0.0],
                    [ax, by, 0.0],
                    [ax, ay, 0.0],
                ];
                &aabb_pts
            } else {
                return None;
            };

            // Low residual parallel to `pts` (empty for the AABB fallback,
            // whose coarse f32 box doesn't carry one).
            let low: &[[f32; 3]] = if !wire.points.is_empty() {
                &wire.points_low
            } else {
                &[]
            };
            let mut hit = false;
            let mut all_inside = true;
            let mut prev: Option<Point> = None;

            for (i, &[px, py, pz]) in pts.iter().enumerate() {
                if px.is_nan() {
                    prev = None;
                    continue;
                }
                let sp = world_to_screen(wp64([px, py, pz], low, i), view_rot, eye, bounds);
                if crossing {
                    if inside(sp) {
                        hit = true;
                    }
                    if let Some(p0) = prev {
                        if !hit {
                            hit = segments_intersect(p0, sp, box_tl, box_tr)
                                || segments_intersect(p0, sp, box_tr, box_br)
                                || segments_intersect(p0, sp, box_br, box_bl)
                                || segments_intersect(p0, sp, box_bl, box_tl);
                        }
                    }
                } else {
                    if !inside(sp) {
                        all_inside = false;
                    }
                }
                prev = Some(sp);
            }

            let result = if crossing {
                hit
            } else {
                all_inside && prev.is_some()
            };
            if result {
                Some(wire.name.as_str())
            } else {
                None
            }
        })
        .collect()
}

// ── Polygon / lasso selection ─────────────────────────────────────────────

/// Return the names of wires selected by a freehand polygon lasso.
///
/// - **Window mode** (`crossing = false`): ALL projected points inside polygon.
/// - **Crossing mode** (`crossing = true`): ANY point inside OR any wire
///   segment crosses a polygon edge.
pub fn poly_hit<'a>(
    poly: &[Point],
    crossing: bool,
    wires: &'a [WireModel],
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Vec<&'a str> {
    if poly.len() < 3 {
        return vec![];
    }

    // Q: lazy projection — no Vec allocation per wire.
    wires
        .iter()
        .filter_map(|wire| {
            // Same AABB fallback as `box_hit`: when a wire has no line
            // geometry (e.g. greek-LOD text emits only fill_tris) treat the
            // AABB rectangle as the hit-test shape so low-LOD text stays
            // selectable. See #19.
            let aabb_pts: Vec<[f32; 3]>;
            let pts: &[[f32; 3]] = if !wire.points.is_empty() {
                &wire.points
            } else if wire.aabb != WireModel::UNBOUNDED_AABB {
                let [ax, ay, bx, by] = wire.aabb;
                aabb_pts = vec![
                    [ax, ay, 0.0],
                    [bx, ay, 0.0],
                    [bx, by, 0.0],
                    [ax, by, 0.0],
                    [ax, ay, 0.0],
                ];
                &aabb_pts
            } else {
                return None;
            };

            let low: &[[f32; 3]] = if !wire.points.is_empty() {
                &wire.points_low
            } else {
                &[]
            };
            let mut hit = false;
            let mut all_inside = true;
            let mut prev: Option<Point> = None;

            for (i, &[px, py, pz]) in pts.iter().enumerate() {
                if px.is_nan() {
                    prev = None;
                    continue;
                }
                let sp = world_to_screen(wp64([px, py, pz], low, i), view_rot, eye, bounds);
                // Reject points the GPU scissored out of a floating viewport so
                // the lasso can't reach clipped geometry. No-op in model space.
                if sp.x < 0.0 || sp.x > bounds.width || sp.y < 0.0 || sp.y > bounds.height {
                    all_inside = false;
                    prev = None;
                    continue;
                }
                if crossing {
                    if point_in_polygon(sp, poly) {
                        hit = true;
                    }
                    if !hit {
                        if let Some(p0) = prev {
                            if segment_crosses_polygon(p0, sp, poly) {
                                hit = true;
                            }
                        }
                    }
                } else {
                    if !point_in_polygon(sp, poly) {
                        all_inside = false;
                    }
                }
                prev = Some(sp);
            }

            let result = if crossing {
                hit
            } else {
                all_inside && prev.is_some()
            };
            if result {
                Some(wire.name.as_str())
            } else {
                None
            }
        })
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn world_to_screen(world: glam::DVec3, view_rot: Mat4, eye: glam::DVec3, bounds: Rectangle) -> Point {
    let ndc = view_rot.project_point3((world - eye).as_vec3());
    Point::new(
        (ndc.x + 1.0) * 0.5 * bounds.width,
        (1.0 - ndc.y) * 0.5 * bounds.height,
    )
}

/// Reconstruct the absolute-f64 world position of wire vertex `i` from its
/// double-single high (`points`) + low (`points_low`) pair. At UTM scale the
/// high f32 alone is ~0.5 m off, which throws box / lasso / click selection
/// edges off by metres; adding the low residual restores f64 precision.
#[inline]
fn wp64(hi: [f32; 3], low: &[[f32; 3]], i: usize) -> glam::DVec3 {
    let l = low.get(i).copied().unwrap_or([0.0; 3]);
    glam::DVec3::new(
        hi[0] as f64 + l[0] as f64,
        hi[1] as f64 + l[1] as f64,
        hi[2] as f64 + l[2] as f64,
    )
}

/// Even-odd ray-casting test: is `p` inside the polygon?
///
/// Handles multi-path boundaries: NaN points (used as path separators by
/// hatches with islands / holes) reset the previous-vertex tracking so
/// that the ray-cast doesn't draw a spurious closing edge between the
/// end of one sub-path and the start of the next. Each sub-path with at
/// least 2 finite vertices contributes its segments to the parity flip.
fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    // Ray-cast crossing test for a single edge a→b.
    fn cross(p: Point, a: Point, b: Point, inside: &mut bool) {
        if (a.y > p.y) != (b.y > p.y)
            && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
        {
            *inside = !*inside;
        }
    }

    let mut inside = false;
    let mut prev: Option<Point> = None;
    let mut path_start: Option<Point> = None;
    // Vertices in the current sub-path. A boundary can be encoded either as a
    // ring (`[v0,v1,v2,v3]`, needs an implicit closing edge) or as an explicit
    // edge list (`[v0,v1, NaN, v1,v2, NaN, …]`, already closed). Only close a
    // sub-path that is a real ring (≥3 verts); closing a 2-point explicit edge
    // would add a degenerate back-edge that cancels its own crossing.
    let mut count = 0usize;
    let close = |prev: Option<Point>, path_start: Option<Point>, count: usize, inside: &mut bool| {
        if count >= 3 {
            if let (Some(pv), Some(sv)) = (prev, path_start) {
                cross(p, pv, sv, inside);
            }
        }
    };
    for &pt in poly {
        if !pt.x.is_finite() || !pt.y.is_finite() {
            close(prev, path_start, count, &mut inside);
            prev = None;
            path_start = None;
            count = 0;
            continue;
        }
        if let Some(prev_v) = prev {
            cross(p, prev_v, pt, &mut inside);
        } else {
            path_start = Some(pt);
        }
        prev = Some(pt);
        count += 1;
    }
    close(prev, path_start, count, &mut inside);
    inside
}

/// Does segment `[a, b]` cross any edge of the polygon?
fn segment_crosses_polygon(a: Point, b: Point, poly: &[Point]) -> bool {
    let n = poly.len();
    for i in 0..n {
        let c = poly[i];
        let d = poly[(i + 1) % n];
        if segments_intersect(a, b, c, d) {
            return true;
        }
    }
    false
}

/// Do segments `[a,b]` and `[c,d]` intersect?
fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let cross = |o: Point, p: Point, q: Point| -> f32 {
        (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x)
    };
    let d1 = cross(c, d, a);
    let d2 = cross(c, d, b);
    let d3 = cross(a, b, c);
    let d4 = cross(a, b, d);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    false
}

// ── Hatch hit-testing ─────────────────────────────────────────────────────

/// Return the Handle of the first hatch whose screen-space boundary polygon
/// contains `cursor`.
pub fn click_hit_hatch(
    cursor: Point,
    hatches: &HashMap<Handle, HatchModel>,
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Option<Handle> {
    for (&handle, hatch) in hatches {
        if hatch_contains_screen_point(hatch, cursor, view_rot, eye, bounds) {
            return Some(handle);
        }
    }
    None
}

/// Same as `click_hit_hatch` but iterates `(Handle, HatchModel)` pairs
/// where the Handle is the parent Insert handle (block-internal
/// hatches). The first matching pair returns its Insert handle so
/// clicking a sub-hatch of a block selects the Insert, matching
/// AutoCAD's behaviour for block sub-entities.
pub fn click_hit_insert_hatch(
    cursor: Point,
    insert_hatches: &[(Handle, HatchModel)],
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Option<Handle> {
    for (handle, hatch) in insert_hatches {
        if hatch_contains_screen_point(hatch, cursor, view_rot, eye, bounds) {
            return Some(*handle);
        }
    }
    None
}

fn hatch_contains_screen_point(
    hatch: &HatchModel,
    cursor: Point,
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> bool {
    // A cursor outside the pane rectangle can't pick a hatch scissored out of a
    // floating viewport. No-op in model space (bounds is the canvas).
    if cursor.x < 0.0 || cursor.x > bounds.width || cursor.y < 0.0 || cursor.y > bounds.height {
        return false;
    }
    // boundary verts are stored as small f32 offsets from
    // `world_origin` (f64). Reconstruct offset-rel WCS before
    // projecting to screen.
    let (ox, oy) = (hatch.world_origin[0], hatch.world_origin[1]);
    let screen: Vec<Point> = hatch
        .boundary
        .iter()
        .map(|&[x, y]| {
            if x.is_finite() && y.is_finite() {
                world_to_screen(glam::DVec3::new(x as f64 + ox, y as f64 + oy, 0.0), view_rot, eye, bounds)
            } else {
                // Preserve path separators for the NaN-aware
                // point_in_polygon ray-cast.
                Point::new(f32::NAN, f32::NAN)
            }
        })
        .collect();
    screen.len() >= 3 && point_in_polygon(cursor, &screen)
}

/// Return Handles of hatches selected by a completed rectangular selection box.
pub fn box_hit_hatch(
    corner_a: Point,
    corner_b: Point,
    crossing: bool,
    hatches: &HashMap<Handle, HatchModel>,
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Vec<Handle> {
    let min_x = corner_a.x.min(corner_b.x);
    let max_x = corner_a.x.max(corner_b.x);
    let min_y = corner_a.y.min(corner_b.y);
    let max_y = corner_a.y.max(corner_b.y);

    if (max_x - min_x) < 1.0 || (max_y - min_y) < 1.0 {
        return vec![];
    }

    let inside = |sp: Point| sp.x >= min_x && sp.x <= max_x && sp.y >= min_y && sp.y <= max_y;

    hatches
        .iter()
        .filter_map(|(&handle, hatch)| {
            if hatch.boundary.is_empty() {
                return None;
            }
            let (ox, oy) = (hatch.world_origin[0], hatch.world_origin[1]);
            let screen: Vec<Point> = hatch
                .boundary
                .iter()
                .map(|&[x, y]| world_to_screen(glam::DVec3::new(x as f64 + ox, y as f64 + oy, 0.0), view_rot, eye, bounds))
                .collect();
            let hit = if crossing {
                screen.iter().any(|&sp| inside(sp))
            } else {
                screen.iter().all(|&sp| inside(sp))
            };
            if hit {
                Some(handle)
            } else {
                None
            }
        })
        .collect()
}

/// Return Handles of hatches selected by a freehand polygon lasso.
pub fn poly_hit_hatch(
    poly: &[Point],
    crossing: bool,
    hatches: &HashMap<Handle, HatchModel>,
    view_rot: Mat4,
    eye: glam::DVec3,
    bounds: Rectangle,
) -> Vec<Handle> {
    if poly.len() < 3 {
        return vec![];
    }

    hatches
        .iter()
        .filter_map(|(&handle, hatch)| {
            if hatch.boundary.is_empty() {
                return None;
            }
            let (ox, oy) = (hatch.world_origin[0], hatch.world_origin[1]);
            let screen: Vec<Point> = hatch
                .boundary
                .iter()
                .map(|&[x, y]| world_to_screen(glam::DVec3::new(x as f64 + ox, y as f64 + oy, 0.0), view_rot, eye, bounds))
                .collect();
            let hit = if crossing {
                screen.iter().any(|&sp| point_in_polygon(sp, poly))
                    || screen
                        .windows(2)
                        .any(|seg| segment_crosses_polygon(seg[0], seg[1], poly))
            } else {
                screen.iter().all(|&sp| point_in_polygon(sp, poly))
            };
            if hit {
                Some(handle)
            } else {
                None
            }
        })
        .collect()
}

/// Minimum distance from point `p` to line segment `[a, b]` in 2-D.
fn dist_point_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let len2 = abx * abx + aby * aby;
    let t = if len2 < 1e-6 {
        0.0
    } else {
        let apx = p.x - a.x;
        let apy = p.y - a.y;
        ((apx * abx + apy * aby) / len2).clamp(0.0, 1.0)
    };
    let cx = a.x + t * abx;
    let cy = a.y + t * aby;
    let dx = p.x - cx;
    let dy = p.y - cy;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod aabb_reject_tests {
    use super::*;

    fn wire(name: &str, pts: Vec<[f32; 3]>, aabb: [f32; 4]) -> WireModel {
        let mut w = WireModel::solid(name.to_string(), pts, [1.0; 4], false);
        w.aabb = aabb;
        w
    }

    // Identity ortho view: world (x,y) → screen ((x+1)*100, (1-y)*100) for a
    // 200×200 viewport. The view is flat (z_axis.xy == 0) so the AABB pre-reject
    // is active — these tests guard it against false negatives.
    #[test]
    fn aabb_reject_keeps_near_wire_drops_far() {
        let vp = Mat4::IDENTITY;
        let bounds = Rectangle { x: 0.0, y: 0.0, width: 200.0, height: 200.0 };
        let cursor = Point::new(100.0, 100.0); // world origin

        let near = wire("5", vec![[-0.02, 0.0, 0.0], [0.02, 0.0, 0.0]], [-0.02, 0.0, 0.02, 0.0]);
        let far = wire("9", vec![[0.9, 0.9, 0.0], [0.95, 0.9, 0.0]], [0.9, 0.9, 0.95, 0.9]);

        let eye = glam::DVec3::ZERO;
        assert_eq!(click_hit(cursor, std::slice::from_ref(&near), vp, eye, bounds, true), Some("5"));
        assert_eq!(click_hit(cursor, std::slice::from_ref(&far), vp, eye, bounds, true), None);
        // The far wire must be rejected without hiding the near one.
        assert_eq!(click_hit(cursor, &[far, near], vp, eye, bounds, true), Some("5"));
    }
}
