// Explode tool — ribbon definition + command implementation.
//
// Command:  EXPLODE (X)
//   EXPLODE: Breaks compound objects into their constituent simple entities.
//
//   Supported:
//     LwPolyline  → Lines (straight segments) + Arcs (bulge segments)
//     Polyline2D  → Lines + Arcs
//     Polyline3D  → Lines
//     Polyline    → Lines
//     Insert      → constituent entities (via acadrust explode_from_document)
//     MLine       → Lines (spine + offset lines per miter direction)
//     Dimension   → Lines (extension + dimension + arrows) + Text
//
//   Unsupported entity types are skipped silently.

use std::f64::consts::TAU;

use acadrust::entities::EntityCommon;
use acadrust::entities::{
    Arc as ArcEnt, Block, BlockEnd, Circle as CircleEnt, Dimension, Line as LineEnt, LwPolyline,
    MLine,
};
use acadrust::entities::{Polyline, Polyline2D};
use acadrust::tables::BlockRecord;
use acadrust::types::Vector3;
use acadrust::{CadDocument, EntityType, Handle};

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use glam::DVec3;

// ── Ribbon definition ──────────────────────────────────────────────────────

pub fn tool() -> ToolDef {
    ToolDef {
        id: "EXPLODE",
        label: "Explode",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/explode.svg")),
        event: ModuleEvent::Command("EXPLODE".to_string()),
    }
}

// ── Geometry helpers ────────────────────────────────────────────────────────

/// Explode just the polyline family (LwPolyline / Polyline / Polyline2D /
/// Polyline3D) into Line + Arc segments. No document needed — used where a
/// polyline must be treated as its constituent edges (e.g. TRIM boundaries).
/// Returns empty for any other entity type.
pub fn explode_polyline_segments(entity: &EntityType) -> Vec<EntityType> {
    match entity {
        EntityType::LwPolyline(p) => explode_lwpolyline(p),
        EntityType::Polyline2D(p) => explode_polyline2d(p),
        EntityType::Polyline(p) => explode_polyline(p),
        EntityType::Polyline3D(p) => explode_polyline3d(p),
        _ => vec![],
    }
}

/// Decompose an entity into its constituent simple entities.
/// Returns an empty vec if the entity cannot be exploded.
pub fn explode_entity(entity: &EntityType, document: &CadDocument) -> Vec<EntityType> {
    match entity {
        EntityType::LwPolyline(p) => explode_lwpolyline(p),
        EntityType::Polyline2D(p) => explode_polyline2d(p),
        EntityType::Polyline(p) => explode_polyline(p),
        EntityType::Polyline3D(p) => explode_polyline3d(p),
        EntityType::Insert(ins) => ins
            .explode_from_document(document)
            .into_iter()
            .map(normalize_insert_entity)
            .collect(),
        EntityType::MLine(ml) => explode_mline(ml),
        EntityType::Dimension(dim) => explode_dimension(dim, document),
        _ => vec![],
    }
}

fn explode_polyline(p: &Polyline) -> Vec<EntityType> {
    let n = p.vertices.len();
    if n < 2 {
        return vec![];
    }
    let closed = p.flags.is_closed();
    let n_segs = if closed { n } else { n - 1 };
    let mut result = Vec::new();
    for i in 0..n_segs {
        let v0 = &p.vertices[i];
        let v1 = &p.vertices[(i + 1) % n];
        let mut common = p.common.clone();
        common.handle = Handle::NULL;
        result.push(EntityType::Line(LineEnt {
            common,
            start: v0.location.clone(),
            end: v1.location.clone(),
            ..LineEnt::new()
        }));
    }
    result
}

fn explode_polyline3d(p: &acadrust::entities::Polyline3D) -> Vec<EntityType> {
    let n = p.vertices.len();
    if n < 2 {
        return vec![];
    }
    let closed = p.is_closed();
    let n_segs = if closed { n } else { n - 1 };
    let mut result = Vec::new();
    for i in 0..n_segs {
        let v0 = &p.vertices[i];
        let v1 = &p.vertices[(i + 1) % n];
        let mut common = p.common.clone();
        common.handle = Handle::NULL;
        result.push(EntityType::Line(LineEnt {
            common,
            start: v0.position.clone(),
            end: v1.position.clone(),
            ..LineEnt::new()
        }));
    }
    result
}

fn explode_polyline2d(p: &Polyline2D) -> Vec<EntityType> {
    let n = p.vertices.len();
    if n < 2 {
        return vec![];
    }
    let closed = p.is_closed();
    let n_segs = if closed { n } else { n - 1 };
    let elevation = p.elevation;

    let mut result = Vec::new();
    for i in 0..n_segs {
        let v0 = &p.vertices[i];
        let v1 = &p.vertices[(i + 1) % n];
        let p0 = [v0.location.x, v0.location.y];
        let p1 = [v1.location.x, v1.location.y];

        if v0.bulge.abs() < 1e-10 {
            let mut common = p.common.clone();
            common.handle = Handle::NULL;
            result.push(EntityType::Line(LineEnt {
                common,
                start: Vector3::new(p0[0], p0[1], elevation),
                end: Vector3::new(p1[0], p1[1], elevation),
                ..LineEnt::new()
            }));
        } else if let Some(arc) = bulge_to_arc(p0, p1, v0.bulge, elevation, &p.common) {
            result.push(arc);
        }
    }
    result
}

pub fn normalize_insert_entity(mut entity: EntityType) -> EntityType {
    match &mut entity {
        EntityType::Ellipse(ell) => {
            let major_len = ell.major_axis_length();
            let full_span = {
                let mut span = ell.end_parameter - ell.start_parameter;
                if span < 0.0 {
                    span += std::f64::consts::TAU;
                }
                (span - std::f64::consts::TAU).abs() < 1e-6
            };
            if (ell.minor_axis_ratio - 1.0).abs() < 1e-6 && full_span {
                let mut circle = CircleEnt::new();
                circle.common = ell.common.clone();
                circle.center = ell.center;
                circle.radius = major_len;
                circle.normal = ell.normal;
                entity = EntityType::Circle(circle);
            }
        }
        _ => {}
    }

    entity.common_mut().handle = Handle::NULL;
    entity.common_mut().owner_handle = Handle::NULL;
    entity
}

pub fn normalize_entity_for_block(entity: EntityType) -> EntityType {
    entity
}

fn explode_lwpolyline(p: &LwPolyline) -> Vec<EntityType> {
    let n = p.vertices.len();
    if n < 2 {
        return vec![];
    }

    let elevation = p.elevation;
    let n_segs = if p.is_closed { n } else { n - 1 };

    let mut result = Vec::new();
    for i in 0..n_segs {
        let v0 = &p.vertices[i];
        let v1 = &p.vertices[(i + 1) % n];

        let p0 = [v0.location.x, v0.location.y];
        let p1 = [v1.location.x, v1.location.y];

        if v0.bulge.abs() < 1e-10 {
            // Straight segment → Line
            let mut common = p.common.clone();
            common.handle = Handle::NULL;
            let line = LineEnt {
                common,
                start: Vector3::new(p0[0], p0[1], elevation),
                end: Vector3::new(p1[0], p1[1], elevation),
                ..LineEnt::new()
            };
            result.push(EntityType::Line(line));
        } else {
            // Arc segment from bulge
            if let Some(arc) = bulge_to_arc(p0, p1, v0.bulge, elevation, &p.common) {
                result.push(arc);
            }
        }
    }
    result
}

/// Convert a polyline bulge segment to an Arc entity.
///   Arc angles are measured from the +X axis.
fn bulge_to_arc(
    p0: [f64; 2],
    p1: [f64; 2],
    bulge: f64,
    elevation: f64,
    common_src: &EntityCommon,
) -> Option<EntityType> {
    let ba = crate::entities::common::BulgeArc::from_bulge(p0, p1, bulge)?;

    // acadrust Arc is always CCW from start_angle to end_angle. Negative
    // bulge means the polyline goes p0→p1 the CW way around the centre,
    // which is the same circular arc traversed p1→p0 the CCW way — so
    // swap endpoints when bulge < 0.
    let (start_angle, end_angle) = if bulge > 0.0 {
        (norm_angle(ba.start_angle), norm_angle(ba.end_angle))
    } else {
        (norm_angle(ba.end_angle), norm_angle(ba.start_angle))
    };

    let mut common = common_src.clone();
    common.handle = Handle::NULL;

    let arc = ArcEnt {
        common,
        center: Vector3::new(ba.center[0], ba.center[1], elevation),
        radius: ba.radius,
        start_angle,
        end_angle,
        ..ArcEnt::new()
    };
    Some(EntityType::Arc(arc))
}

fn norm_angle(a: f64) -> f64 {
    ((a % TAU) + TAU) % TAU
}

fn explode_mline(ml: &MLine) -> Vec<EntityType> {
    let n = ml.vertices.len();
    if n < 2 {
        return vec![];
    }
    let closed = ml.flags.contains(acadrust::entities::MLineFlags::CLOSED);
    let scale = ml.scale_factor;
    let n_segs = if closed { n } else { n - 1 };
    let mut result = Vec::new();

    // Helper: build a Line from two Vector3 positions.
    let make_line = |common: &acadrust::entities::EntityCommon,
                     s: &acadrust::types::Vector3,
                     e: &acadrust::types::Vector3|
     -> EntityType {
        let mut c = common.clone();
        c.handle = Handle::NULL;
        EntityType::Line(LineEnt {
            common: c,
            start: s.clone(),
            end: e.clone(),
            ..LineEnt::new()
        })
    };

    // For each segment, emit the center-spine line and the two ±scale/2 offset lines.
    for i in 0..n_segs {
        let v0 = &ml.vertices[i];
        let v1 = &ml.vertices[(i + 1) % n];

        // Spine line
        result.push(make_line(&ml.common, &v0.position, &v1.position));

        if scale.abs() > 1e-9 {
            let half = scale * 0.5;
            for &sign in &[-1.0_f64, 1.0_f64] {
                let off = half * sign;
                // Use miter direction at each vertex to offset the endpoints.
                let s = Vector3::new(
                    v0.position.x + v0.miter.x * off,
                    v0.position.y + v0.miter.y * off,
                    v0.position.z + v0.miter.z * off,
                );
                let e = Vector3::new(
                    v1.position.x + v1.miter.x * off,
                    v1.position.y + v1.miter.y * off,
                    v1.position.z + v1.miter.z * off,
                );
                result.push(make_line(&ml.common, &s, &e));
            }
        }
    }

    result
}

// ── Dimension explode ──────────────────────────────────────────────────────

/// Convert a Dimension entity into Lines (geometry) + Text (label).
/// A NULL-handle line segment for a baked dimension block.
fn dim_seg(a: Vector3, b: Vector3, common: &acadrust::entities::EntityCommon) -> EntityType {
    let mut c = common.clone();
    c.handle = Handle::NULL;
    EntityType::Line(LineEnt {
        common: c,
        start: a,
        end: b,
        ..LineEnt::new()
    })
}

/// Open (two-stroke) arrowhead with its tip at `tip`, strokes pointing back
/// along the unit vector `(dx,dy)` (from the tip toward the dimension line),
/// sized `size`. Simple, valid in any reader, and far better than a bare line.
fn dim_arrowhead(
    tip: Vector3,
    dx: f64,
    dy: f64,
    size: f64,
    common: &acadrust::entities::EntityCommon,
) -> Vec<EntityType> {
    let ang = dy.atan2(dx);
    let wing = 18f64.to_radians();
    let mk = |a: f64| Vector3::new(tip.x + size * a.cos(), tip.y + size * a.sin(), tip.z);
    vec![
        dim_seg(tip, mk(ang + wing), common),
        dim_seg(tip, mk(ang - wing), common),
    ]
}

/// Center-mark cross at `center`, arm length |DIMCEN|. Empty when DIMCEN == 0.
fn dim_center_mark(
    center: Vector3,
    dimcen: f64,
    common: &acadrust::entities::EntityCommon,
) -> Vec<EntityType> {
    let s = dimcen.abs();
    if s < 1e-9 {
        return Vec::new();
    }
    vec![
        dim_seg(
            Vector3::new(center.x - s, center.y, center.z),
            Vector3::new(center.x + s, center.y, center.z),
            common,
        ),
        dim_seg(
            Vector3::new(center.x, center.y - s, center.z),
            Vector3::new(center.x, center.y + s, center.z),
            common,
        ),
    ]
}

/// Arc from angle `a1` to `a2` (radians, swept the short way) at `radius` about
/// `center`, approximated by straight chords (world f64).
fn dim_arc_segs(
    center: Vector3,
    radius: f64,
    a1: f64,
    a2: f64,
    common: &acadrust::entities::EntityCommon,
) -> Vec<EntityType> {
    use std::f64::consts::PI;
    let mut sweep = a2 - a1;
    while sweep > PI {
        sweep -= 2.0 * PI;
    }
    while sweep < -PI {
        sweep += 2.0 * PI;
    }
    let steps = 12usize.max((sweep.abs() / (PI / 36.0)).ceil() as usize);
    let pt = |a: f64| Vector3::new(center.x + radius * a.cos(), center.y + radius * a.sin(), center.z);
    let mut out = Vec::new();
    let mut prev = pt(a1);
    for i in 1..=steps {
        let cur = pt(a1 + sweep * (i as f64 / steps as f64));
        out.push(dim_seg(prev, cur, common));
        prev = cur;
    }
    out
}

/// (DIMASZ, DIMCEN), DIMSCALE-applied, resolved from the dimension's style.
fn dim_metrics(dim: &Dimension, doc: &CadDocument) -> (f64, f64) {
    let name = dim.base().style_name.as_str();
    let style = doc.dim_styles.iter().find(|s| {
        s.name.eq_ignore_ascii_case(name)
            || (name.trim().is_empty() && s.name.eq_ignore_ascii_case("Standard"))
    });
    let scale = style
        .map(|s| if s.dimscale > 1e-6 { s.dimscale } else { 1.0 })
        .unwrap_or(1.0);
    let dimasz = style.map(|s| s.dimasz * scale).unwrap_or(0.18 * scale).max(1e-6);
    let dimcen = style.map(|s| s.dimcen * scale).unwrap_or(0.09 * scale);
    (dimasz, dimcen)
}

/// Baked geometry for an angular dimension: an extension line along each ray
/// out to the dimension arc, plus the swept arc itself (which is what makes it
/// read as an angle rather than two crossing lines — #181 / DIM-022).
fn angular_block_segs(
    vertex: Vector3,
    p1: Vector3,
    p2: Vector3,
    arc_loc: Vector3,
    common: &acadrust::entities::EntityCommon,
) -> Vec<EntityType> {
    let a1 = (p1.y - vertex.y).atan2(p1.x - vertex.x);
    let a2 = (p2.y - vertex.y).atan2(p2.x - vertex.x);
    let radius = ((arc_loc.x - vertex.x).powi(2) + (arc_loc.y - vertex.y).powi(2)).sqrt();
    let mut out = Vec::new();
    if radius < 1e-9 {
        out.push(dim_seg(p1, vertex, common));
        out.push(dim_seg(p2, vertex, common));
        return out;
    }
    let e1 = Vector3::new(
        vertex.x + a1.cos() * radius,
        vertex.y + a1.sin() * radius,
        vertex.z,
    );
    let e2 = Vector3::new(
        vertex.x + a2.cos() * radius,
        vertex.y + a2.sin() * radius,
        vertex.z,
    );
    // Extension lines run from each measured ray point out to the arc.
    out.push(dim_seg(p1, e1, common));
    out.push(dim_seg(p2, e2, common));
    out.extend(dim_arc_segs(vertex, radius, a1, a2, common));
    out
}

fn explode_dimension(dim: &Dimension, doc: &CadDocument) -> Vec<EntityType> {
    use acadrust::entities::Text;

    let base = dim.base();
    let common = base.common.clone();
    let mut result: Vec<EntityType> = Vec::new();

    // Helper: make a line segment
    let make_seg = |a: &Vector3, b: &Vector3, common: &EntityCommon| -> EntityType {
        let mut c = common.clone();
        c.handle = Handle::NULL;
        EntityType::Line(LineEnt {
            common: c,
            start: a.clone(),
            end: b.clone(),
            ..LineEnt::new()
        })
    };

    let v3 = |x: f64, y: f64, z: f64| Vector3::new(x, y, z);

    match dim {
        Dimension::Aligned(d) => {
            let fx = d.first_point.x;
            let fy = d.first_point.y;
            let sx = d.second_point.x;
            let sy = d.second_point.y;
            let dx_s = sx - fx;
            let dy_s = sy - fy;
            let len = (dx_s * dx_s + dy_s * dy_s).sqrt().max(1e-12);
            let axis_angle = dy_s.atan2(dx_s);
            let perp_x = -(axis_angle.sin());
            let perp_y = axis_angle.cos();
            let offset =
                (d.definition_point.x - fx) * perp_x + (d.definition_point.y - fy) * perp_y;
            let d1 = v3(fx + perp_x * offset, fy + perp_y * offset, d.first_point.z);
            let d2 = v3(sx + perp_x * offset, sy + perp_y * offset, d.second_point.z);
            result.push(make_seg(&d.first_point, &d1, &common));
            result.push(make_seg(&d.second_point, &d2, &common));
            result.push(make_seg(&d1, &d2, &common));
            let (asz, _) = dim_metrics(dim, doc);
            let ml = ((d2.x - d1.x).powi(2) + (d2.y - d1.y).powi(2)).sqrt().max(1e-12);
            let (ux, uy) = ((d2.x - d1.x) / ml, (d2.y - d1.y) / ml);
            result.extend(dim_arrowhead(d1, ux, uy, asz, &common));
            result.extend(dim_arrowhead(d2, -ux, -uy, asz, &common));
            let _ = len;
        }
        Dimension::Linear(d) => {
            // `rotation` is the dimension-line angle, already in radians (the
            // same convention the live renderer uses) — do not convert again.
            let angle = d.rotation;
            let perp_x = -(angle.sin());
            let perp_y = angle.cos();
            let fx = d.first_point.x;
            let fy = d.first_point.y;
            let sx = d.second_point.x;
            let sy = d.second_point.y;
            // The dimension line passes through `definition_point` at `rotation`.
            // Project each extension origin onto that line *independently*: a
            // point's landing offset is (def - point)·perp. A single shared
            // offset only lands both origins on the line when they are level;
            // for sloped origins (e.g. a DIMCONTINUE chain over non-level
            // points) it tilts the dimension line — issue #181.
            let dperp = d.definition_point.x * perp_x + d.definition_point.y * perp_y;
            let off1 = dperp - (fx * perp_x + fy * perp_y);
            let off2 = dperp - (sx * perp_x + sy * perp_y);
            let d1 = v3(fx + perp_x * off1, fy + perp_y * off1, d.first_point.z);
            let d2 = v3(sx + perp_x * off2, sy + perp_y * off2, d.second_point.z);
            result.push(make_seg(&d.first_point, &d1, &common));
            result.push(make_seg(&d.second_point, &d2, &common));
            result.push(make_seg(&d1, &d2, &common));
            let (asz, _) = dim_metrics(dim, doc);
            let ml = ((d2.x - d1.x).powi(2) + (d2.y - d1.y).powi(2)).sqrt().max(1e-12);
            let (ux, uy) = ((d2.x - d1.x) / ml, (d2.y - d1.y) / ml);
            result.extend(dim_arrowhead(d1, ux, uy, asz, &common));
            result.extend(dim_arrowhead(d2, -ux, -uy, asz, &common));
        }
        Dimension::Radius(d) => {
            // center -> point on circle, arrowhead at the point toward centre,
            // plus the centre mark.
            let (center, point) = (d.angle_vertex, d.definition_point);
            result.push(make_seg(&center, &point, &common));
            let (asz, cen) = dim_metrics(dim, doc);
            let m = ((center.x - point.x).powi(2) + (center.y - point.y).powi(2))
                .sqrt()
                .max(1e-12);
            result.extend(dim_arrowhead(
                point,
                (center.x - point.x) / m,
                (center.y - point.y) / m,
                asz,
                &common,
            ));
            result.extend(dim_center_mark(center, cen, &common));
        }
        Dimension::Diameter(d) => {
            // Full diameter line through the centre (far edge -> near edge),
            // inward arrows at both edges, plus the centre mark. angle_vertex is
            // the centre and definition_point the point on the circle.
            let (center, edge) = (d.angle_vertex, d.definition_point);
            let far = v3(2.0 * center.x - edge.x, 2.0 * center.y - edge.y, edge.z);
            result.push(make_seg(&far, &edge, &common));
            let (asz, cen) = dim_metrics(dim, doc);
            let m = ((edge.x - far.x).powi(2) + (edge.y - far.y).powi(2))
                .sqrt()
                .max(1e-12);
            let (ux, uy) = ((edge.x - far.x) / m, (edge.y - far.y) / m);
            result.extend(dim_arrowhead(edge, -ux, -uy, asz, &common));
            result.extend(dim_arrowhead(far, ux, uy, asz, &common));
            result.extend(dim_center_mark(center, cen, &common));
        }
        Dimension::Angular2Ln(d) => {
            result.extend(angular_block_segs(
                d.angle_vertex,
                d.first_point,
                d.second_point,
                d.dimension_arc,
                &common,
            ));
        }
        Dimension::Angular3Pt(d) => {
            result.extend(angular_block_segs(
                d.angle_vertex,
                d.first_point,
                d.second_point,
                d.definition_point,
                &common,
            ));
        }
        Dimension::Ordinate(d) => {
            result.push(make_seg(&d.feature_location, &d.definition_point, &common));
            result.push(make_seg(&d.definition_point, &d.leader_endpoint, &common));
        }
    }

    // Measurement text: value, position, height and rotation are all taken
    // from the SAME live-render path (style-resolved formatting incl.
    // DIMDEC/DIMLFAC/DIMPOST/units/`<>` and DIMTAD/DIMGAP placement), so the
    // baked block matches the on-screen dimension and nothing changes when the
    // file is saved and reopened — the reload renders from this block. A `None`
    // means the text is suppressed (user_text " "), so bake no Text. #181.
    if let Some((value, text_pos, text_h, text_rot)) =
        crate::entities::dimension::baked_dimension_text(dim, doc, 1.0)
    {
        let mut text = Text::with_value(value, text_pos)
            .with_height(text_h.max(0.1))
            .with_rotation(text_rot);
        text.common = common.clone();
        text.common.handle = Handle::NULL;
        result.push(EntityType::Text(text));
    }

    result
}

// ── Dimension block baking (DWG/DXF interop) ────────────────────────────────

/// Mirror each dimension's authoritative geometric definition point into
/// `base.definition_point`, which is the field the DWG/DXF writer emits as
/// group 10. Edits (grips, properties, transforms) update the per-type struct
/// field but not `base`, so without this the saved group 10 goes stale and the
/// dimension's line / leader / origin jumps on reload (#181). Angular-2-line
/// keeps a distinct base point (the second line's point) and is left alone.
fn sync_dimension_base_points(doc: &mut CadDocument) {
    for e in doc.entities_mut() {
        if let EntityType::Dimension(d) = e {
            let def = match d {
                Dimension::Linear(x) => Some(x.definition_point),
                Dimension::Aligned(x) => Some(x.definition_point),
                Dimension::Radius(x) => Some(x.definition_point),
                Dimension::Diameter(x) => Some(x.definition_point),
                Dimension::Ordinate(x) => Some(x.definition_point),
                Dimension::Angular3Pt(x) => Some(x.definition_point),
                Dimension::Angular2Ln(_) => None,
            };
            if let Some(p) = def {
                d.base_mut().definition_point = p;
            }
        }
    }
}

/// Smallest free `*D<n>` anonymous block name in `doc`.
fn next_dimension_block_name(doc: &CadDocument) -> String {
    let mut n = 0u64;
    loop {
        let cand = format!("*D{n}");
        if doc.block_records.get(&cand).is_none() {
            return cand;
        }
        n += 1;
    }
}

/// Bake an anonymous `*D<n>` geometry block for every DIMENSION that doesn't
/// already own one, so the file is valid for AutoCAD-family readers.
///
/// OCS renders dimensions by re-tessellating them on the fly and never
/// materialises the `*D` block that a DWG `DIMENSION` is supposed to reference
/// (the lines / arrows / text that AutoCAD actually draws). A dimension created
/// in OCS therefore goes out referencing a block that doesn't exist, and the
/// writer emits a null block handle — strict readers (DWG TrueView, QCAD) drop
/// the dimension or demand a recovery, and lenient ones (BricsCAD) regenerate it
/// at a different position. Call this on the document about to be written so each
/// such dimension gets a real block built from its exploded geometry (extension
/// lines + dimension line + measurement text, the same decomposition EXPLODE
/// uses) and its `block_name` points at it.
///
/// Dimensions that already reference an existing block (e.g. imported from a real
/// DWG, or copied via the `*D`-cloning copy path) are left untouched so their
/// original graphics are preserved.
pub fn bake_dimension_blocks(doc: &mut CadDocument) {
    // Keep group-10 (base.definition_point) in step with the per-type geometry
    // before writing — see sync_dimension_base_points.
    sync_dimension_base_points(doc);

    // Handles of dimensions whose block reference is missing or dangling.
    let pending: Vec<Handle> = doc
        .entities()
        .filter_map(|e| match e {
            EntityType::Dimension(d) => {
                let bn = &d.base().block_name;
                if bn.trim().is_empty() || doc.block_records.get(bn).is_none() {
                    Some(d.base().common.handle)
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();

    for handle in pending {
        let dim = match doc.get_entity(handle) {
            Some(EntityType::Dimension(d)) => d.clone(),
            _ => continue,
        };
        let subs = explode_dimension(&dim, doc);
        if subs.is_empty() {
            continue;
        }

        let name = next_dimension_block_name(doc);
        // Reserve three consecutive handles for the record / block / endblk.
        // Adding the block + endblk (which carry explicit handles) advances the
        // document's handle counter past them, so the NULL-handle sub-entities
        // added afterwards get fresh handles without colliding.
        let next = doc.next_handle();
        let br_handle = Handle::new(next);
        let block_handle = Handle::new(next + 1);
        let end_handle = Handle::new(next + 2);

        let mut br = BlockRecord::new(&name);
        br.handle = br_handle;
        br.block_entity_handle = block_handle;
        br.block_end_handle = end_handle;
        br.flags.anonymous = true;
        if doc.block_records.add(br).is_err() {
            continue;
        }

        let mut block = Block::new(&name, Vector3::new(0.0, 0.0, 0.0));
        block.common.handle = block_handle;
        block.common.owner_handle = br_handle;
        let _ = doc.add_entity(EntityType::Block(block));

        let mut block_end = BlockEnd::new();
        block_end.common.handle = end_handle;
        block_end.common.owner_handle = br_handle;
        let _ = doc.add_entity(EntityType::BlockEnd(block_end));

        for mut sub in subs {
            sub.common_mut().handle = Handle::NULL;
            sub.common_mut().owner_handle = br_handle;
            let _ = doc.add_entity(sub);
        }

        if let Some(EntityType::Dimension(d)) = doc.get_entity_mut(handle) {
            d.base_mut().block_name = name;
        }
    }
}

// ── Command stub (kept for future interactive selection mode) ───────────────

pub struct ExplodeCommand;

impl ExplodeCommand {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }
}

impl CadCommand for ExplodeCommand {
    fn name(&self) -> &'static str {
        "EXPLODE"
    }
    fn prompt(&self) -> String {
        "EXPLODE  Select objects to explode:".into()
    }

    fn on_point(&mut self, _pt: DVec3) -> CmdResult {
        CmdResult::Cancel
    }
    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }
    fn on_escape(&mut self) -> CmdResult {
        CmdResult::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::entities::DimensionLinear;

    /// A dimension created without a geometry block gets a real `*D` block on
    /// bake, its `block_name` resolves to that block, and a second bake is a
    /// no-op (already has a valid block).
    #[test]
    fn bakes_block_for_blockless_dimension_and_is_idempotent() {
        let mut doc = CadDocument::new();

        let mut d = DimensionLinear::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 0.0, 0.0));
        d.definition_point = Vector3::new(0.0, 5.0, 0.0);
        d.base.text_middle_point = Vector3::new(5.0, 5.0, 0.0);
        // block_name is left empty — exactly what OCS-created dimensions carry.
        let handle = doc
            .add_entity(EntityType::Dimension(Dimension::Linear(d)))
            .unwrap();

        bake_dimension_blocks(&mut doc);

        let block_name = match doc.get_entity(handle) {
            Some(EntityType::Dimension(d)) => d.base().block_name.clone(),
            _ => panic!("dimension missing"),
        };
        assert!(!block_name.trim().is_empty(), "block_name should be set");
        assert!(
            doc.block_records.get(&block_name).is_some(),
            "baked block must exist in the block table"
        );

        // Second pass must not create another block for the same dimension.
        let before = doc.block_records.len();
        bake_dimension_blocks(&mut doc);
        assert_eq!(
            doc.block_records.len(),
            before,
            "a dimension that already owns a block must not be re-baked"
        );
    }

    // Collect the line segments baked into the dimension's `*D` block.
    fn baked_segments(doc: &CadDocument, block_name: &str) -> Vec<(Vector3, Vector3)> {
        let rec = doc.block_records.get(block_name).expect("block record");
        doc.entities()
            .filter_map(|e| match e {
                EntityType::Line(l) if l.common.owner_handle == rec.handle => {
                    Some((l.start, l.end))
                }
                _ => None,
            })
            .collect()
    }

    // A horizontal (rotation = 0) linear dimension whose two measured points sit
    // at *different* heights must still bake a level dimension line — both
    // extension origins project onto the same line. Regression test for #181,
    // where a shared offset tilted the dimension line.
    #[test]
    fn linear_dim_line_stays_level_over_sloped_points() {
        let mut doc = CadDocument::new();
        let mut d = DimensionLinear::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 5.0, 0.0));
        d.rotation = 0.0;
        d.definition_point = Vector3::new(0.0, 8.0, 0.0);
        let handle = doc
            .add_entity(EntityType::Dimension(Dimension::Linear(d)))
            .unwrap();
        bake_dimension_blocks(&mut doc);
        let name = match doc.get_entity(handle) {
            Some(EntityType::Dimension(d)) => d.base().block_name.clone(),
            _ => panic!("dimension missing"),
        };
        // The dimension line is the segment spanning both x extents; it must be
        // horizontal at the definition-point level (y = 8).
        let dim_line = baked_segments(&doc, &name)
            .into_iter()
            .find(|(a, b)| (a.x - 0.0).abs() < 1e-6 && (b.x - 10.0).abs() < 1e-6)
            .expect("dimension line segment");
        assert!(
            (dim_line.0.y - 8.0).abs() < 1e-6 && (dim_line.1.y - 8.0).abs() < 1e-6,
            "dimension line must be level at y=8, got {:?}",
            dim_line
        );
    }

    // An angular dimension must bake its swept ARC (not just two rays), else a
    // saved+reloaded angular dim collapses to a V. The block should carry the
    // two extension lines plus many arc chords.
    #[test]
    fn angular_dim_bakes_an_arc() {
        use acadrust::entities::DimensionAngular3Pt;
        let mut doc = CadDocument::new();
        let mut d = DimensionAngular3Pt::new(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(10.0, 0.0, 0.0),
            Vector3::new(0.0, 10.0, 0.0),
        );
        d.definition_point = Vector3::new(5.0, 5.0, 0.0); // arc location
        let handle = doc
            .add_entity(EntityType::Dimension(Dimension::Angular3Pt(d)))
            .unwrap();
        bake_dimension_blocks(&mut doc);
        let name = match doc.get_entity(handle) {
            Some(EntityType::Dimension(d)) => d.base().block_name.clone(),
            _ => panic!("dimension missing"),
        };
        let rec = doc.block_records.get(&name).expect("block");
        let lines = doc
            .entities()
            .filter(|e| matches!(e, EntityType::Line(l) if l.common.owner_handle == rec.handle))
            .count();
        assert!(lines > 5, "angular bake must include arc chords, got {lines} lines");
    }

    // A diameter dimension bakes a line edge-to-edge THROUGH the centre, not a
    // radius-length line. The two extreme endpoints must be equidistant from the
    // centre (angle_vertex) and the centre must lie between them.
    #[test]
    fn diameter_dim_bakes_through_center() {
        use acadrust::entities::DimensionDiameter;
        let mut doc = CadDocument::new();
        let center = Vector3::new(3.0, 4.0, 0.0);
        let edge = Vector3::new(8.0, 4.0, 0.0); // radius 5 along +x
        let mut d = DimensionDiameter::new(center, edge);
        d.base.text_middle_point = Vector3::new(3.0, 9.0, 0.0);
        let handle = doc
            .add_entity(EntityType::Dimension(Dimension::Diameter(d)))
            .unwrap();
        bake_dimension_blocks(&mut doc);
        let name = match doc.get_entity(handle) {
            Some(EntityType::Dimension(d)) => d.base().block_name.clone(),
            _ => panic!("dimension missing"),
        };
        // The longest baked segment is the diameter line; its endpoints span the
        // full diameter (length ~= 2*radius = 10) centred on `center`.
        let diam = baked_segments(&doc, &name)
            .into_iter()
            .find(|(a, b)| {
                let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                (len - 10.0).abs() < 1e-6
            })
            .expect("diameter line spanning 2*radius");
        let mid_x = (diam.0.x + diam.1.x) * 0.5;
        let mid_y = (diam.0.y + diam.1.y) * 0.5;
        assert!(
            (mid_x - center.x).abs() < 1e-6 && (mid_y - center.y).abs() < 1e-6,
            "diameter line must be centred on the circle centre, mid=({mid_x},{mid_y})"
        );
    }

    // A rotated linear dimension uses `rotation` directly (radians). Before the
    // fix `to_radians()` shrank a 90° dim to ~1.57°, baking a nearly-horizontal
    // line instead of a vertical one.
    #[test]
    fn rotated_linear_dim_bakes_at_its_angle() {
        let mut doc = CadDocument::new();
        let mut d = DimensionLinear::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 10.0, 0.0));
        d.rotation = std::f64::consts::FRAC_PI_2; // 90°, vertical dimension line
        d.definition_point = Vector3::new(8.0, 0.0, 0.0);
        let handle = doc
            .add_entity(EntityType::Dimension(Dimension::Linear(d)))
            .unwrap();
        bake_dimension_blocks(&mut doc);
        let name = match doc.get_entity(handle) {
            Some(EntityType::Dimension(d)) => d.base().block_name.clone(),
            _ => panic!("dimension missing"),
        };
        // Dimension line spans both y extents and must be vertical at x = 8.
        let dim_line = baked_segments(&doc, &name)
            .into_iter()
            .find(|(a, b)| (a.y - 0.0).abs() < 1e-6 && (b.y - 10.0).abs() < 1e-6)
            .expect("dimension line segment");
        assert!(
            (dim_line.0.x - 8.0).abs() < 1e-6 && (dim_line.1.x - 8.0).abs() < 1e-6,
            "dimension line must be vertical at x=8, got {:?}",
            dim_line
        );
    }
}
