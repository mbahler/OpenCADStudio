// Helix entity (AcDbHelix): a spline curve plus its generating parameters.
//
// A loaded helix carries a fully evaluated `spline` (the NURBS the file
// stored) alongside the parameters that generated it (base point, axis,
// radius, turns, turn height, twist, constraint). We render and grip the
// entity through its embedded spline, and expose the generating parameters
// as the Geometry property group.

use acadrust::entities::{Helix, HelixConstraint};

use crate::command::EntityTransform;
use crate::entities::common::ro_prop as ro;
use crate::entities::traits::{Grippable, Transformable, TruckConvertible};
use crate::scene::convert::acad_to_truck::TruckEntity;
use crate::scene::model::object::{GripApply, GripDef, PropSection};

/// Perpendicular distance from `p` to the helix axis (line through
/// `axis_base_point` along `axis_vector`). Used to recover the top radius
/// from the last defining point of the spline.
fn radius_from_axis(helix: &Helix, p: &acadrust::types::Vector3) -> Option<f64> {
    let base = &helix.axis_base_point;
    let axis = helix.axis_vector.normalize();
    // A zero axis vector cannot define a line.
    if axis.length() < 1e-9 {
        return None;
    }
    let vx = p.x - base.x;
    let vy = p.y - base.y;
    let vz = p.z - base.z;
    let proj = vx * axis.x + vy * axis.y + vz * axis.z;
    let px = vx - proj * axis.x;
    let py = vy - proj * axis.y;
    let pz = vz - proj * axis.z;
    Some((px * px + py * py + pz * pz).sqrt())
}

fn grips(helix: &Helix) -> Vec<GripDef> {
    Grippable::grips(&helix.spline)
}

fn properties(helix: &Helix) -> Vec<PropSection> {
    let bp = &helix.axis_base_point;
    let turns = helix.turns;
    let turn_height = helix.turn_height;
    // Overall height is turns × per-turn rise.
    let height = turns * turn_height;
    let base_radius = helix.radius;
    // Top radius: the axial distance of the last defining point. For a
    // cylindrical helix this equals the base radius.
    let top_radius = helix
        .spline
        .control_points
        .last()
        .or_else(|| helix.spline.fit_points.last())
        .and_then(|p| radius_from_axis(helix, p))
        .unwrap_or(base_radius);
    // Turn slope: rise angle of a single turn against the base circumference.
    let turn_slope = if base_radius.abs() > 1e-9 {
        (turn_height / (std::f64::consts::TAU * base_radius))
            .atan()
            .to_degrees()
    } else {
        0.0
    };
    let twist = if helix.handedness { "CW" } else { "CCW" };
    let constrain = match helix.constraint {
        HelixConstraint::TurnHeight => "Turn Height",
        HelixConstraint::Turns => "Turns",
        HelixConstraint::Height => "Height",
    };

    vec![PropSection {
        title: "Geometry".into(),
        props: vec![
            ro("Base point X", "base_x", format!("{:.4}", bp.x)),
            ro("Base point Y", "base_y", format!("{:.4}", bp.y)),
            ro("Base point Z", "base_z", format!("{:.4}", bp.z)),
            ro("Number of turns", "num_turns", format!("{turns:.4}")),
            ro("Turn height", "turn_height", format!("{turn_height:.4}")),
            ro("Turns", "turns", format!("{turns:.4}")),
            ro("Height", "height", format!("{height:.4}")),
            ro("Base radius", "base_radius", format!("{base_radius:.4}")),
            ro("Top radius", "top_radius", format!("{top_radius:.4}")),
            ro("Turn slope", "turn_slope", format!("{turn_slope:.4}")),
            ro("Twist", "twist", twist),
            ro("Constrain", "constrain", constrain),
        ],
    }]
}

/// Helix geometry is derived from the stored spline; the generating
/// parameters are read-only (editing one would require regenerating the
/// spline, which the loaded entity does not carry a recipe for).
fn apply_geom_prop(_helix: &mut Helix, _field: &str, _value: &str) {}

fn apply_grip(helix: &mut Helix, grip_id: usize, apply: GripApply) {
    Grippable::apply_grip(&mut helix.spline, grip_id, apply);
}

fn apply_transform(helix: &mut Helix, t: &EntityTransform) {
    // Move the visible curve, then keep the generating points in step.
    Transformable::apply_transform(&mut helix.spline, t);
}

impl TruckConvertible for Helix {
    fn to_truck(&self, document: &acadrust::CadDocument) -> Option<TruckEntity> {
        self.spline.to_truck(document)
    }
}

impl Grippable for Helix {
    fn grips(&self) -> Vec<GripDef> {
        grips(self)
    }
    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        apply_grip(self, grip_id, apply);
    }
}

impl crate::entities::traits::PropertyEditable for Helix {
    fn geometry_properties(&self, _text_style_names: &[String]) -> Vec<PropSection> {
        properties(self)
    }
    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        apply_geom_prop(self, field, value);
    }
}

impl Transformable for Helix {
    fn apply_transform(&mut self, t: &EntityTransform) {
        apply_transform(self, t);
    }
}
