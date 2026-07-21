// SOLID entity — 2D filled quadrilateral (or triangle when p3 == p4).
//
// Wireframe: the 4 perimeter edges as TruckObject::Lines.
// Filled:    two triangles on `fill_tris`, so a SOLID nested in a block (e.g.
//            a `_BoxFilled` dimension arrowhead) renders filled — block
//            expansion has no separate hatch-model injection. Top-level SOLIDs
//            also get a solid-fill HatchModel; the duplicate fill is invisible
//            for the opaque solids these are (arrowheads, cell fills).
// Grips:     4 corner grip points.

use acadrust::entities::Solid;

use crate::command::EntityTransform;
use crate::entities::common::{edit_prop as edit, ro_prop as ro, square_grip};
use crate::entities::traits::{Grippable, PropertyEditable, Transformable, TruckConvertible};
use crate::scene::convert::acad_to_truck::{TruckEntity, TruckObject};
use crate::scene::model::object::{GripApply, GripDef, PropSection};
use crate::scene::model::wire_model::SnapHint;

fn dvec3(v: &acadrust::types::Vector3) -> glam::DVec3 {
    glam::DVec3::new(v.x, v.y, v.z)
}

impl TruckConvertible for Solid {
    fn to_truck(&self, _document: &acadrust::CadDocument) -> Option<TruckEntity> {
        // SOLID corners are OCS (planar entity) — map through the arbitrary
        // axis so mirrored solids (normal 0,0,-1) land on the correct side.
        let n = (self.normal.x, self.normal.y, self.normal.z);
        let w = |v: &acadrust::types::Vector3| -> [f64; 3] {
            let (x, y, z) = crate::scene::view::transform::ocs_point_to_wcs((v.x, v.y, v.z), n);
            [x, y, z]
        };
        let p0 = w(&self.first_corner);
        let p1 = w(&self.second_corner);
        let p2 = w(&self.third_corner);
        let p3 = w(&self.fourth_corner);

        // DXF SOLID vertex order: 1-2-4-3 (Z-shaped), render as closed quad outline.
        // AutoCAD stores corners in "Z" order: p0-p1 are top edge, p2-p3 are bottom edge,
        // so the visual quad is p0→p1→p3→p2→p0.
        let pts = vec![
            p0,
            p1,
            [f64::NAN; 3],
            p1,
            p3,
            [f64::NAN; 3],
            p3,
            p2,
            [f64::NAN; 3],
            p2,
            p0,
        ];

        let dvp = |p: [f64; 3]| glam::DVec3::new(p[0], p[1], p[2]);
        let snap = vec![
            (dvp(p0), SnapHint::Node),
            (dvp(p1), SnapHint::Node),
            (dvp(p2), SnapHint::Node),
            (dvp(p3), SnapHint::Node),
        ];

        // Fill the visual quad p0→p1→p3→p2 (Z-order) as two triangles; a
        // triangle SOLID (p2 == p3) degenerates the second, which is harmless.
        let fill_tris = vec![p0, p1, p3, p0, p3, p2];

        Some(TruckEntity {
            pick_tris: Vec::new(),
            object: TruckObject::Lines(pts),
            snap_pts: snap,
            tangent_geoms: vec![],
            key_vertices: vec![p0, p1, p2, p3],
            fill_tris,
        })
    }
}

impl Grippable for Solid {
    fn grips(&self) -> Vec<GripDef> {
        vec![
            square_grip(0, dvec3(&self.first_corner)),
            square_grip(1, dvec3(&self.second_corner)),
            square_grip(2, dvec3(&self.third_corner)),
            square_grip(3, dvec3(&self.fourth_corner)),
        ]
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        let corner = match grip_id {
            0 => &mut self.first_corner,
            1 => &mut self.second_corner,
            2 => &mut self.third_corner,
            3 => &mut self.fourth_corner,
            _ => return,
        };
        match apply {
            GripApply::Translate(d) => {
                corner.x += d.x as f64;
                corner.y += d.y as f64;
                corner.z += d.z as f64;
            }
            GripApply::Absolute(p) => {
                corner.x = p.x as f64;
                corner.y = p.y as f64;
                corner.z = p.z as f64;
            }
        }
    }
}

impl PropertyEditable for Solid {
    fn geometry_properties(&self, _text_style_names: &[String]) -> Vec<PropSection> {
        // Elevation is the OCS Z shared by the planar corners (no dedicated
        // field on the entity); reported from the first corner's Z.
        let elevation = self.first_corner.z;
        vec![PropSection {
            title: "Geometry".into(),
            props: vec![
                edit("Point 1 X", "sl_p1x", self.first_corner.x),
                edit("Point 1 Y", "sl_p1y", self.first_corner.y),
                edit("Point 1 Z", "sl_p1z", self.first_corner.z),
                edit("Point 2 X", "sl_p2x", self.second_corner.x),
                edit("Point 2 Y", "sl_p2y", self.second_corner.y),
                edit("Point 2 Z", "sl_p2z", self.second_corner.z),
                edit("Point 3 X", "sl_p3x", self.third_corner.x),
                edit("Point 3 Y", "sl_p3y", self.third_corner.y),
                edit("Point 3 Z", "sl_p3z", self.third_corner.z),
                edit("Point 4 X", "sl_p4x", self.fourth_corner.x),
                edit("Point 4 Y", "sl_p4y", self.fourth_corner.y),
                edit("Point 4 Z", "sl_p4z", self.fourth_corner.z),
                ro("Elevation", "sl_elev", format!("{:.4}", elevation)),
            ],
        }]
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        let Ok(v) = value.trim().parse::<f64>() else {
            return;
        };
        match field {
            "sl_p1x" => self.first_corner.x = v,
            "sl_p1y" => self.first_corner.y = v,
            "sl_p1z" => self.first_corner.z = v,
            "sl_p2x" => self.second_corner.x = v,
            "sl_p2y" => self.second_corner.y = v,
            "sl_p2z" => self.second_corner.z = v,
            "sl_p3x" => self.third_corner.x = v,
            "sl_p3y" => self.third_corner.y = v,
            "sl_p3z" => self.third_corner.z = v,
            "sl_p4x" => self.fourth_corner.x = v,
            "sl_p4y" => self.fourth_corner.y = v,
            "sl_p4z" => self.fourth_corner.z = v,
            _ => {}
        }
    }
}

impl Transformable for Solid {
    fn apply_transform(&mut self, t: &EntityTransform) {
        crate::scene::view::transform::apply_standard_entity_transform(
            self,
            t,
            |entity, p1, p2| {
                for corner in [
                    &mut entity.first_corner,
                    &mut entity.second_corner,
                    &mut entity.third_corner,
                    &mut entity.fourth_corner,
                ] {
                    crate::scene::view::transform::reflect_xy_point(
                        &mut corner.x,
                        &mut corner.y,
                        p1,
                        p2,
                    );
                }
            },
        );
    }
}
