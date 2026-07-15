// Regression for #385: text and dimension text must reach the PDF / print
// export. From v0.8.2 text renders on-screen only as GPU SDF glyph quads
// (`WireModel::text_verts`); the CPU PDF exporter used to draw only wire
// stroke `points`, so all text — standalone TEXT/MTEXT and dimension text —
// vanished from exported PDFs (present in v0.7.6). This drives the real
// entity -> scene.entity_wires() -> export_pdf path and asserts text is
// carried to the exporter and lands in the file.
use acadrust::entities::{Dimension, DimensionLinear, Text};
use acadrust::types::Vector3;
use acadrust::EntityType;
use OpenCADStudio::io::pdf_export::export_pdf;
use OpenCADStudio::scene::Scene;

#[test]
fn text_and_dim_reach_pdf_export() {
    let mut scene = Scene::new();

    // A standalone TEXT entity.
    let t = Text::with_value("HELLO", Vector3::new(10.0, 10.0, 0.0)).with_height(5.0);
    scene.add_entity(EntityType::Text(t));

    // A linear dimension — its measurement value is synthesized as SDF text
    // too, so it exercises the dimension-text path the reporters called out.
    let dim = DimensionLinear::new(
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(40.0, 0.0, 0.0),
    );
    scene.add_entity(EntityType::Dimension(Dimension::Linear(dim)));

    let wires = scene.entity_wires();

    // The regression symptom was that text produced no exportable geometry.
    // It must now ride the export wire set as SDF glyph quads.
    let text_wires = wires.iter().filter(|w| !w.text_verts.is_empty()).count();
    assert!(
        text_wires > 0,
        "no text_verts on the export wire set — text/dim text would be missing from the PDF"
    );

    // End-to-end: exporting with the text present must produce a valid PDF that
    // is larger than the same wire set with the text stripped — i.e. the glyph
    // geometry actually reaches the file.
    let dir = std::env::temp_dir();
    let p_text = dir.join("ocs_385_with_text.pdf");
    export_pdf(&wires, &[], &[], 210.0, 297.0, 0.0, 0.0, 0, 1.0, None, &p_text, None)
        .expect("export with text");
    let with_text = std::fs::read(&p_text).expect("read pdf");
    assert!(with_text.starts_with(b"%PDF"), "not a PDF");

    let stripped: Vec<_> = wires
        .iter()
        .cloned()
        .map(|mut w| {
            w.text_verts.clear();
            w
        })
        .collect();
    let p_bare = dir.join("ocs_385_no_text.pdf");
    export_pdf(&stripped, &[], &[], 210.0, 297.0, 0.0, 0.0, 0, 1.0, None, &p_bare, None)
        .expect("export without text");
    let no_text = std::fs::read(&p_bare).expect("read pdf");

    assert!(
        with_text.len() > no_text.len(),
        "text added no content to the PDF: {} !> {}",
        with_text.len(),
        no_text.len()
    );
}
