// PDF export — converts the paper-space wire model to a PDF file using printpdf.
//
// Each WireModel becomes a sequence of DrawLine operations.  NaN values in the
// points array act as segment separators (pen-up).
//
// Coordinate system: CAD uses mm units with origin at bottom-left and Y up.
// printpdf's Point::new(Mm, Mm) also has origin at bottom-left, so no Y-flip
// is needed — we shift the coordinates by (offset_x, offset_y) to place the
// drawing origin at the paper origin.

use crate::io::plot_style::PlotStyleTable;
use crate::scene::model::hatch_model::HatchModel;
#[cfg(not(target_arch = "wasm32"))]
use crate::scene::model::hatch_model::HatchPattern;
use crate::scene::WireModel;
#[cfg(not(target_arch = "wasm32"))]
use printpdf::{
    Color, Line, LineCapStyle, LineDashPattern, LineJoinStyle, LinePoint, Mm, Op, PaintMode,
    PdfDocument, PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt, Rgb, WindingOrder,
};
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
use std::path::Path;

// The web build has no `printpdf` (it pulls a wasm-incompatible `memchr` via
// lopdf → nom_locate) and no filesystem, so PDF export is native-only; the web
// build gets these stubs so the call sites still compile.
#[cfg(target_arch = "wasm32")]
pub fn export_pdf(
    _wires: &[WireModel],
    _hatches: &[HatchModel],
    _wipeouts: &[HatchModel],
    _paper_w: f64,
    _paper_h: f64,
    _offset_x: f32,
    _offset_y: f32,
    _rotation_deg: i32,
    _scale: f32,
    _clip: Option<(f32, f32, f32, f32)>,
    _path: &Path,
    _plot_style: Option<&PlotStyleTable>,
) -> Result<(), String> {
    Err("PDF export is not available in the web version.".into())
}

#[cfg(target_arch = "wasm32")]
pub async fn pick_pdf_path_owned(_stem: String) -> Option<std::path::PathBuf> {
    None
}

// ── Public entry point ────────────────────────────────────────────────────

/// Export `wires` to a PDF file.
///
/// - `paper_w` / `paper_h`: page dimensions in mm (already swapped for 90°/270° by caller).
/// - `offset_x` / `offset_y`: added to every wire coordinate so the drawing
///   origin maps to the bottom-left corner of the page.
/// - `rotation_deg`: 0 | 90 | 180 | 270 — rotates the entire drawing on the page.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_pdf(
    wires: &[WireModel],
    hatches: &[HatchModel],
    wipeouts: &[HatchModel],
    paper_w: f64,
    paper_h: f64,
    offset_x: f32,
    offset_y: f32,
    rotation_deg: i32,
    scale: f32,
    clip: Option<(f32, f32, f32, f32)>,
    path: &Path,
    plot_style: Option<&PlotStyleTable>,
) -> Result<(), String> {
    let bytes = build_pdf(
        wires,
        hatches,
        wipeouts,
        paper_w as f32,
        paper_h as f32,
        offset_x,
        offset_y,
        rotation_deg,
        scale,
        clip,
        plot_style,
    );
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())
}

/// Show a PDF save-file dialog and return the chosen path (or None if cancelled).
#[cfg(not(target_arch = "wasm32"))]
pub async fn pick_pdf_path_owned(stem: String) -> Option<std::path::PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Export as PDF")
        .set_file_name(&format!("{stem}.pdf"))
        .add_filter("PDF Files", &["pdf"])
        .add_filter("All Files", &["*"])
        .save_file()
        .await
        .map(|h| crate::sys::handle_path(&h))
}

// ── PDF builder ───────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn build_pdf(
    wires: &[WireModel],
    hatches: &[HatchModel],
    wipeouts: &[HatchModel],
    paper_w: f32,
    paper_h: f32,
    ox: f32,
    oy: f32,
    rotation_deg: i32,
    scale: f32,
    clip: Option<(f32, f32, f32, f32)>,
    plot_style: Option<&PlotStyleTable>,
) -> Vec<u8> {
    let mut doc = PdfDocument::new("Open CAD Studio Export");
    let mut ops: Vec<Op> = Vec::new();

    // White page background.
    ops.push(Op::SetFillColor {
        col: Color::Rgb(Rgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            icc_profile: None,
        }),
    });
    ops.push(Op::DrawRectangle {
        rectangle: printpdf::Rect::from_wh(Mm(paper_w).into(), Mm(paper_h).into()),
    });

    // Round line caps/joins for CAD aesthetics.
    ops.push(Op::SetLineCapStyle {
        cap: LineCapStyle::Round,
    });
    ops.push(Op::SetLineJoinStyle {
        join: LineJoinStyle::Round,
    });

    // Apply rotation/scale/clip transform if needed.
    // PDF uses mm-based coordinate system with origin at bottom-left.
    // We save state, apply a CTM (+ optional clip path), then restore after drawing.
    let needs_state = rotation_deg != 0 || (scale - 1.0).abs() > 1e-6 || clip.is_some();
    if needs_state {
        let (cos_a, sin_a, tx, ty) = match rotation_deg {
            90 => (0.0_f64, 1.0_f64, 0.0, paper_h as f64),
            180 => (-1.0_f64, 0.0_f64, paper_w as f64, paper_h as f64),
            270 => (0.0_f64, -1.0_f64, paper_w as f64, 0.0),
            _ => (1.0_f64, 0.0_f64, 0.0, 0.0),
        };
        let s = scale as f64;
        // PDF CTM: [a b c d e f] = [cos*s sin*s -sin*s cos*s tx ty]
        ops.push(Op::SaveGraphicsState);
        // Convert mm translation to points (1 mm = 2.834645 pt).
        let tx_pt = (tx * 2.834645) as f32;
        let ty_pt = (ty * 2.834645) as f32;
        ops.push(Op::SetTransformationMatrix {
            matrix: printpdf::CurTransMat::Raw([
                (cos_a * s) as f32,
                (sin_a * s) as f32,
                (-(sin_a) * s) as f32,
                (cos_a * s) as f32,
                tx_pt,
                ty_pt,
            ]),
        });
        // Clip rectangle (mm), applied in the pre-scale coordinate space so it
        // matches the wires drawn under the same CTM.
        if let Some((cx, cy, cw, ch)) = clip {
            const MM_TO_PT: f32 = 2.834645;
            ops.push(Op::DrawPolygon {
                polygon: Polygon {
                    rings: vec![PolygonRing {
                        points: vec![
                            LinePoint {
                                p: Point { x: Pt(cx * MM_TO_PT), y: Pt(cy * MM_TO_PT) },
                                bezier: false,
                            },
                            LinePoint {
                                p: Point { x: Pt((cx + cw) * MM_TO_PT), y: Pt(cy * MM_TO_PT) },
                                bezier: false,
                            },
                            LinePoint {
                                p: Point {
                                    x: Pt((cx + cw) * MM_TO_PT),
                                    y: Pt((cy + ch) * MM_TO_PT),
                                },
                                bezier: false,
                            },
                            LinePoint {
                                p: Point { x: Pt(cx * MM_TO_PT), y: Pt((cy + ch) * MM_TO_PT) },
                                bezier: false,
                            },
                        ],
                    }],
                    mode: PaintMode::Clip,
                    winding_order: WindingOrder::NonZero,
                },
            });
        }
    }

    // mm to PDF points (1 mm = 2.834645 pt).
    const MM_TO_PT: f32 = 2.834645;
    // `wire.line_weight_px` is the on-screen pixel weight: mm × (96/25.4) × 2.0,
    // where the ×2 is a screen-legibility boost (see render.rs). Print wants the
    // true physical weight, so undo both the 96-dpi scaling and the boost before
    // converting to points — otherwise weights export ~2× too heavy in pixels
    // (and the old `× 0.35278` left them inconsistent with the physical mm).
    const LW_PX_TO_PT: f32 = MM_TO_PT / ((96.0 / 25.4) * 2.0);

    // ── Hatch / wipeout fills (rendered before wires so wires draw on top,
    //    matching paper_canvas ordering). Each `emit_hatch` sets its own
    //    fill / stroke colour, so the wire pass below starts fresh.
    for hatch in wipeouts.iter().chain(hatches.iter()) {
        emit_hatch(&mut ops, hatch, ox, oy);
    }

    let mut last_color: Option<[f32; 3]> = None;
    let mut last_lw: Option<f32> = None;
    // Current PDF dash array (empty = solid). Tracked so the dash op is only
    // re-emitted when it actually changes between wires.
    let mut last_dash: Option<Vec<i64>> = None;

    for wire in wires {
        let [mut r, mut g, mut b, a] = wire.color;
        if a < 0.01 {
            continue;
        }
        // Skip the paper-boundary wire — the white PDF background already provides it.
        if wire.name == "__paper_boundary__" {
            continue;
        }
        // Apply CTB plot style table overrides (color + lineweight).
        let mut lw_override: Option<f32> = None;
        if let Some(ctb) = plot_style {
            if wire.aci > 0 {
                if let Some([cr, cg, cb]) = ctb.resolve_color(wire.aci) {
                    r = cr;
                    g = cg;
                    b = cb;
                }
                lw_override = ctb
                    .resolve_lineweight(wire.aci)
                    .map(|mm| (mm * MM_TO_PT).max(0.1) / scale.max(1e-6));
            }
        }
        // Near-white and near-yellow (viewport active border) → dark grey for print
        // (only when no CTB override was applied).
        if lw_override.is_none() {
            let is_light = r > 0.80 && g > 0.80 && b > 0.80;
            let is_yellow = r > 0.80 && g > 0.70 && b < 0.30;
            let is_cyan = r < 0.30 && g > 0.70 && b > 0.70;
            if is_light || is_yellow {
                r = 0.0;
                g = 0.0;
                b = 0.0;
            } else if is_cyan {
                // Viewport border: print as dark blue.
                r = 0.0;
                g = 0.15;
                b = 0.50;
            }
        }

        if last_color
            .map(|c| (c[0] - r).abs() > 0.01 || (c[1] - g).abs() > 0.01 || (c[2] - b).abs() > 0.01)
            .unwrap_or(true)
        {
            ops.push(Op::SetOutlineColor {
                col: Color::Rgb(Rgb {
                    r,
                    g,
                    b,
                    icc_profile: None,
                }),
            });
            last_color = Some([r, g, b]);
        }

        // Line weight: CTB override (in pt) or screen px → points. Divided by
        // `scale` in both branches so pen widths stay absolute under the
        // scaled CTM above (AutoCAD keeps lineweights independent of plot
        // scale) — without this a Fit plot of a large window renders
        // near-invisible hairlines.
        let lw_pt = lw_override
            .unwrap_or_else(|| (wire.line_weight_px * LW_PX_TO_PT).max(0.1) / scale.max(1e-6));
        if last_lw.map(|l| (l - lw_pt).abs() > 0.01).unwrap_or(true) {
            ops.push(Op::SetOutlineThickness { pt: Pt(lw_pt) });
            last_lw = Some(lw_pt);
        }

        // Linetype dash pattern. Without this every wire exported as a solid
        // line regardless of its linetype (dashed / centre / dash-dot). (#155)
        let dash_arr = dash_array_from_pattern(wire.pattern_length, &wire.pattern, MM_TO_PT);
        if last_dash.as_deref() != Some(dash_arr.as_slice()) {
            let dash = if dash_arr.is_empty() {
                LineDashPattern::default()
            } else {
                LineDashPattern::from_array(&dash_arr, 0)
            };
            ops.push(Op::SetLineDashPattern { dash });
            last_dash = Some(dash_arr);
        }

        // Emit segments (NaN = pen-up).
        let mut segment: Vec<LinePoint> = Vec::new();
        for &[x, y, _z] in &wire.points {
            if x.is_nan() || y.is_nan() {
                flush_line(&mut ops, &segment);
                segment.clear();
            } else {
                segment.push(LinePoint {
                    p: Point::new(Mm(x + ox), Mm(y + oy)),
                    bezier: false,
                });
            }
        }
        flush_line(&mut ops, &segment);
    }

    if needs_state {
        ops.push(Op::RestoreGraphicsState);
    }

    let page = PdfPage::new(Mm(paper_w), Mm(paper_h), ops);
    doc.pages.push(page);

    let mut warnings = Vec::new();
    doc.save(&PdfSaveOptions::default(), &mut warnings)
}

/// Build a PDF dash array (in points) from a WireModel linetype pattern.
///
/// `pattern` holds the linetype run lengths in paper-mm: positive = dash,
/// negative = gap, exactly 0 = a dot, and trailing zeros are padding — so the
/// real length is the index of the last non-zero element + 1 (same convention
/// the wire shader uses). Returns an empty vec for a solid line. printpdf's
/// `LineDashPattern` holds at most six entries, so longer patterns are
/// truncated to three dash/gap pairs.
#[cfg(not(target_arch = "wasm32"))]
fn dash_array_from_pattern(pattern_length: f32, pattern: &[f32; 8], mm_to_pt: f32) -> Vec<i64> {
    if pattern_length <= 1e-6 {
        return Vec::new();
    }
    let count = match pattern.iter().rposition(|&v| v != 0.0) {
        Some(i) => (i + 1).min(6),
        None => return Vec::new(),
    };
    pattern[..count]
        .iter()
        // Round to whole points (printpdf dash entries are integers) and keep a
        // 1 pt floor so a zero-length dot still prints as a short mark.
        .map(|&v| (((v.abs() * mm_to_pt).round()) as i64).max(1))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn flush_line(ops: &mut Vec<Op>, pts: &[LinePoint]) {
    if pts.len() < 2 {
        return;
    }
    ops.push(Op::DrawLine {
        line: Line {
            points: pts.to_vec(),
            is_closed: false,
        },
    });
}

/// Emit a single hatch / wipeout as a filled (or stroked, for pattern fills)
/// polygon. NaN sentinels in `hatch.boundary` split the path into multiple
/// rings so islands and holes render correctly under the even-odd rule.
/// Mirrors `scene::paper_canvas::draw_hatch`: solid → fill, pattern → outline,
/// gradient → solid fill of the averaged colour.
#[cfg(not(target_arch = "wasm32"))]
fn emit_hatch(ops: &mut Vec<Op>, hatch: &HatchModel, ox: f32, oy: f32) {
    if hatch.boundary.is_empty() {
        return;
    }
    let [r, g, b, a] = hatch.color;
    if a < 0.01 {
        return;
    }
    let world_ox = hatch.world_origin[0] as f32;
    let world_oy = hatch.world_origin[1] as f32;

    // Split the boundary into rings on every NaN-NaN separator.
    let mut rings: Vec<PolygonRing> = Vec::new();
    let mut current: Vec<LinePoint> = Vec::new();
    for &[bx, by] in hatch.boundary.iter() {
        if bx.is_nan() || by.is_nan() {
            if current.len() >= 3 {
                rings.push(PolygonRing { points: std::mem::take(&mut current) });
            } else {
                current.clear();
            }
            continue;
        }
        current.push(LinePoint {
            p: Point::new(Mm(bx + world_ox + ox), Mm(by + world_oy + oy)),
            bezier: false,
        });
    }
    if current.len() >= 3 {
        rings.push(PolygonRing { points: current });
    }
    if rings.is_empty() {
        return;
    }

    let (paint_mode, fill_color) = match &hatch.pattern {
        HatchPattern::Solid => (PaintMode::Fill, [r, g, b]),
        HatchPattern::Pattern(_) => {
            // Pattern fills are emitted as raster line segments below; the
            // outline polygon path itself is skipped because pattern
            // hatches in real DXF do not draw their boundary as part of
            // the fill.
            (PaintMode::Clip, [r, g, b]) // sentinel — handled below
        }
        HatchPattern::Gradient { color2, .. } => {
            // PDF gradients are stored in resource dictionaries; for the
            // fast path we average the two colours, matching paper_canvas.
            let avg = [
                (r + color2[0]) * 0.5,
                (g + color2[1]) * 0.5,
                (b + color2[2]) * 0.5,
            ];
            (PaintMode::Fill, avg)
        }
    };

    // Pattern hatches: rasterise the family lines clipped to the boundary
    // and emit each as a stroked line. Skips the polygon outline entirely.
    if matches!(hatch.pattern, HatchPattern::Pattern(_)) {
        let segments = hatch.pattern_segments();
        if segments.is_empty() {
            return;
        }
        ops.push(Op::SetOutlineColor {
            col: Color::Rgb(Rgb {
                r,
                g,
                b,
                icc_profile: None,
            }),
        });
        for [a, b_pt] in segments {
            ops.push(Op::DrawLine {
                line: Line {
                    points: vec![
                        LinePoint {
                            p: Point::new(Mm(a[0] + ox), Mm(a[1] + oy)),
                            bezier: false,
                        },
                        LinePoint {
                            p: Point::new(Mm(b_pt[0] + ox), Mm(b_pt[1] + oy)),
                            bezier: false,
                        },
                    ],
                    is_closed: false,
                },
            });
        }
        return;
    }

    // Solid / gradient: filled polygon path.
    if matches!(paint_mode, PaintMode::Fill | PaintMode::FillStroke) {
        ops.push(Op::SetFillColor {
            col: Color::Rgb(Rgb {
                r: fill_color[0],
                g: fill_color[1],
                b: fill_color[2],
                icc_profile: None,
            }),
        });
    }
    ops.push(Op::DrawPolygon {
        polygon: Polygon {
            rings,
            mode: paint_mode,
            winding_order: WindingOrder::EvenOdd,
        },
    });
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn clip_and_scale_emit_pdf_bytes() {
        let w = WireModel::solid(
            "test".into(),
            vec![[0.0, 0.0, 0.0], [50.0, 50.0, 0.0]],
            WireModel::WHITE,
            false,
        );
        let bytes = build_pdf(
            &[w],
            &[],
            &[],
            210.0,
            297.0,
            0.0,
            0.0,
            0,
            2.0,
            Some((10.0, 10.0, 100.0, 100.0)),
            None,
        );
        // A valid PDF is produced (starts with the PDF header) and is non-trivial.
        assert!(bytes.starts_with(b"%PDF"), "not a PDF");
        assert!(bytes.len() > 200, "suspiciously small: {}", bytes.len());
    }
}
