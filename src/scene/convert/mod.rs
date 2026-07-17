pub mod acad_to_truck;
#[cfg(feature = "solid3d")]
pub mod acis_export;
#[cfg(feature = "solid3d")]
pub mod acis_to_truck;
/// Without `solid3d` (e.g. wasm) ACIS/SAT meshing is unavailable; the entry
/// point stays so callers compile, returning no mesh.
#[cfg(not(feature = "solid3d"))]
pub mod acis_to_truck {
    use crate::scene::model::mesh_model::MeshLodSet;
    use acadrust::entities::acis::SatDocument;
    pub fn tessellate_sat_truck(
        _sat: &SatDocument,
        _name: String,
        _color: [f32; 4],
        _facet_res: f64,
    ) -> Option<MeshLodSet> {
        None
    }
}
pub mod dgn_linestyle;
pub mod truck_tess;
pub mod tessellate;
pub(crate) mod tess;
pub mod proxy_graphics;
pub mod tess_util;
pub mod solid3d_tess;
pub mod spline_tess;
