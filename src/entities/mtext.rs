use acadrust::entities::{AttachmentPoint, DrawingDirection, MText};
use acadrust::types::aci_table::aci_to_rgb;
use glam::Vec3;

use crate::command::EntityTransform;
use crate::entities::common::{edit_prop as edit, ro_prop as ro, square_grip, triangle_grip};
use crate::entities::text_support::{
    parse_mtext_paragraphs, resolve_text_style, InlineColor, MTextRunKind, ParagraphAlign,
    RunState, TabStop,
};
use crate::entities::traits::{Grippable, PropertyEditable, Transformable, TruckConvertible};
use crate::scene::acad_to_truck::{TextStroke, TruckEntity, TruckObject};
use crate::scene::cxf;
use crate::scene::object::{GripApply, GripDef, PropSection, PropValue, Property};
use crate::scene::wire_model::SnapHint;

fn attachment_str(a: &AttachmentPoint) -> &'static str {
    match a {
        AttachmentPoint::TopLeft => "Top Left",
        AttachmentPoint::TopCenter => "Top Center",
        AttachmentPoint::TopRight => "Top Right",
        AttachmentPoint::MiddleLeft => "Middle Left",
        AttachmentPoint::MiddleCenter => "Middle Center",
        AttachmentPoint::MiddleRight => "Middle Right",
        AttachmentPoint::BottomLeft => "Bottom Left",
        AttachmentPoint::BottomCenter => "Bottom Center",
        AttachmentPoint::BottomRight => "Bottom Right",
    }
}

fn mtext_halign_str(a: &AttachmentPoint) -> &'static str {
    match a {
        AttachmentPoint::TopLeft | AttachmentPoint::MiddleLeft | AttachmentPoint::BottomLeft => {
            "Left"
        }
        AttachmentPoint::TopCenter
        | AttachmentPoint::MiddleCenter
        | AttachmentPoint::BottomCenter => "Center",
        AttachmentPoint::TopRight | AttachmentPoint::MiddleRight | AttachmentPoint::BottomRight => {
            "Right"
        }
    }
}

fn mtext_valign_str(a: &AttachmentPoint) -> &'static str {
    match a {
        AttachmentPoint::TopLeft | AttachmentPoint::TopCenter | AttachmentPoint::TopRight => "Top",
        AttachmentPoint::MiddleLeft
        | AttachmentPoint::MiddleCenter
        | AttachmentPoint::MiddleRight => "Middle",
        AttachmentPoint::BottomLeft
        | AttachmentPoint::BottomCenter
        | AttachmentPoint::BottomRight => "Bottom",
    }
}

fn mtext_attachment_from_align(h: &str, v: &str) -> Option<AttachmentPoint> {
    Some(match (h, v) {
        ("Left", "Top") => AttachmentPoint::TopLeft,
        ("Center", "Top") => AttachmentPoint::TopCenter,
        ("Right", "Top") => AttachmentPoint::TopRight,
        ("Left", "Middle") => AttachmentPoint::MiddleLeft,
        ("Center", "Middle") => AttachmentPoint::MiddleCenter,
        ("Right", "Middle") => AttachmentPoint::MiddleRight,
        ("Left", "Bottom") => AttachmentPoint::BottomLeft,
        ("Center", "Bottom") => AttachmentPoint::BottomCenter,
        ("Right", "Bottom") => AttachmentPoint::BottomRight,
        _ => return None,
    })
}

fn drawing_dir_str(d: &DrawingDirection) -> &'static str {
    match d {
        DrawingDirection::LeftToRight => "Left to Right",
        DrawingDirection::TopToBottom => "Top to Bottom",
        DrawingDirection::ByStyle => "By Style",
    }
}

// ── Run-based MTEXT layout ──────────────────────────────────────────────────
//
// Layout proceeds in three passes:
//   1. Atomise — turn each MTextLine.runs into a flat sequence of atoms
//      (Word / Space / Tab) so the wrapper can operate at break boundaries
//      while keeping per-character formatting state.
//   2. Wrap   — accumulate atoms into sub-lines using paragraph indents
//      (`indent_first` for the first sub-line, `indent_left` for
//      continuations, `indent_right` shrinks the right edge). Each Tab
//      jumps the cursor to the next tab stop (or the next 4-em default
//      stop when none is defined).
//   3. Render — for every sub-line: pick paragraph alignment + indent,
//      walk atoms left → right, emit one TextStroke per Word using the
//      atom's RunState (height / width / oblique / tracking / font /
//      colour / decorations / valign).

#[derive(Clone)]
enum AtomKind {
    Word(String),
    Space,
    Tab,
}

#[derive(Clone)]
struct LayoutAtom {
    kind: AtomKind,
    state: RunState,
}

fn run_scale(state: &RunState, entity_h: f32, base_wf: f32) -> f32 {
    (state.height_mul * entity_h / 9.0) * (state.width_mul * base_wf.abs())
}

fn resolve_font<'a>(state: &'a RunState, base: &'a str) -> &'a str {
    state.font.as_deref().unwrap_or(base)
}

fn measure_word(text: &str, state: &RunState, entity_h: f32, base_wf: f32, base_font: &str) -> f32 {
    let scale = run_scale(state, entity_h, base_wf);
    let font_name = resolve_font(state, base_font);
    let font = cxf::get_font(font_name);
    let mut w = 0.0_f32;
    for ch in text.chars() {
        w += match font.glyph(ch) {
            Some(g) => (g.advance + font.letter_spacing * state.tracking) * scale,
            None => (6.0 + font.letter_spacing * state.tracking) * scale,
        };
    }
    w
}

fn measure_space(state: &RunState, entity_h: f32, base_wf: f32, base_font: &str) -> f32 {
    let scale = run_scale(state, entity_h, base_wf);
    let font_name = resolve_font(state, base_font);
    cxf::get_font(font_name).word_spacing * scale
}

fn atom_width(atom: &LayoutAtom, entity_h: f32, base_wf: f32, base_font: &str) -> f32 {
    match &atom.kind {
        AtomKind::Word(t) => measure_word(t, &atom.state, entity_h, base_wf, base_font),
        AtomKind::Space => measure_space(&atom.state, entity_h, base_wf, base_font),
        AtomKind::Tab => 0.0, // tabs jump the cursor explicitly during wrap / render
    }
}

/// Cursor position after a `\t` atom: advance to the next user-defined tab
/// stop that lies past `cur_x`, falling back to a 4-em default grid when no
/// stop is reached.
fn next_tab_position(cur_x: f32, tab_stops: &[TabStop], indent_left: f32, entity_h: f32) -> f32 {
    let local = cur_x - indent_left; // tabs are measured from the content area's left edge
    for ts in tab_stops {
        if ts.position > local + 1e-4 {
            return indent_left + ts.position;
        }
    }
    let default_interval = entity_h * 4.0; // 4 em fallback, matches AutoCAD's "no tab stops"
    let n = (local / default_interval).floor() + 1.0;
    indent_left + n * default_interval
}

/// Break a flat MText paragraph atom stream into wrap-fit sub-lines.
fn wrap_paragraph(
    atoms: Vec<LayoutAtom>,
    rect_w: f32,
    indent_first: f32,
    indent_left: f32,
    indent_right: f32,
    tab_stops: &[TabStop],
    entity_h: f32,
    base_wf: f32,
    base_font: &str,
) -> Vec<Vec<LayoutAtom>> {
    if rect_w <= 0.0 {
        return vec![atoms];
    }
    let mut sublines: Vec<Vec<LayoutAtom>> = Vec::new();
    let mut cur: Vec<LayoutAtom> = Vec::new();
    let mut cur_w = 0.0_f32;
    let mut subline_idx: usize = 0;
    let line_start_x = |idx: usize| if idx == 0 { indent_first } else { indent_left };
    let line_max_w = |idx: usize| (rect_w - indent_right - line_start_x(idx)).max(0.0);

    for atom in atoms {
        match &atom.kind {
            AtomKind::Word(_) => {
                let w = atom_width(&atom, entity_h, base_wf, base_font);
                let max_w = line_max_w(subline_idx);
                if !cur.is_empty() && cur_w + w > max_w {
                    while matches!(cur.last().map(|a| &a.kind), Some(AtomKind::Space)) {
                        cur.pop();
                    }
                    sublines.push(std::mem::take(&mut cur));
                    cur_w = 0.0;
                    subline_idx += 1;
                }
                cur.push(atom);
                cur_w += w;
            }
            AtomKind::Space => {
                if cur.is_empty() {
                    continue;
                }
                cur_w += atom_width(&atom, entity_h, base_wf, base_font);
                cur.push(atom);
            }
            AtomKind::Tab => {
                let start_x = line_start_x(subline_idx);
                let new_w = next_tab_position(cur_w + start_x, tab_stops, indent_left, entity_h)
                    - start_x;
                let max_w = line_max_w(subline_idx);
                if new_w > max_w && !cur.is_empty() {
                    sublines.push(std::mem::take(&mut cur));
                    cur_w = 0.0;
                    subline_idx += 1;
                } else {
                    cur.push(atom);
                    cur_w = new_w.min(max_w);
                }
            }
        }
    }
    if !cur.is_empty() {
        sublines.push(cur);
    }
    if sublines.is_empty() {
        sublines.push(Vec::new());
    }
    sublines
}

fn line_total_width(
    atoms: &[LayoutAtom],
    entity_h: f32,
    base_wf: f32,
    base_font: &str,
    line_start_x: f32,
    indent_left: f32,
    tab_stops: &[TabStop],
) -> f32 {
    let mut x = line_start_x;
    for atom in atoms {
        match atom.kind {
            AtomKind::Tab => {
                x = next_tab_position(x, tab_stops, indent_left, entity_h);
            }
            _ => x += atom_width(atom, entity_h, base_wf, base_font),
        }
    }
    x - line_start_x
}

fn resolve_inline_color(c: &InlineColor) -> Option<[f32; 3]> {
    match c {
        InlineColor::Aci(idx) => {
            aci_to_rgb(*idx).map(|(r, g, b)| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
        }
        InlineColor::True(rgb) => Some(*rgb),
    }
}

/// Wrap a run's glyph text with MTEXT decoration markers so cxf's
/// `tessellate_text_run` emits the underline / overline / strikethrough
/// strokes for us — keeps decoration geometry in one place rather than
/// duplicating the y-position constants.
fn decorated(text: &str, state: &RunState) -> String {
    if !(state.underline || state.overline || state.strike) {
        return text.to_string();
    }
    let mut s = String::with_capacity(text.len() + 6);
    if state.underline {
        s.push_str("\\L");
    }
    if state.overline {
        s.push_str("\\O");
    }
    if state.strike {
        s.push_str("\\K");
    }
    s.push_str(text);
    if state.underline {
        s.push_str("\\l");
    }
    if state.overline {
        s.push_str("\\o");
    }
    if state.strike {
        s.push_str("\\k");
    }
    s
}

fn to_truck(t: &MText, document: &acadrust::CadDocument) -> TruckEntity {
    let resolved_style = resolve_text_style(&t.style, document);
    let base_font_name = resolved_style.font_name.clone();
    let base_font = cxf::get_font(&base_font_name);
    let base_wf_abs = resolved_style.width_factor.max(0.01);
    let base_wf = if resolved_style.is_backward { -base_wf_abs } else { base_wf_abs };
    let base_oblique = resolved_style.oblique_angle;
    let entity_h = t.height as f32;
    let rect_w = t.rectangle_width as f32;

    // ── 1. Parse ────────────────────────────────────────────────────────────
    let paragraphs = parse_mtext_paragraphs(&t.value, entity_h);

    // ── 2. Atomise + wrap each paragraph into sub-lines ─────────────────────
    struct SubLine {
        atoms: Vec<LayoutAtom>,
        align: Option<ParagraphAlign>,
        indent_first: f32,
        indent_left: f32,
        indent_right: f32,
        tab_stops: Vec<TabStop>,
        is_first_in_paragraph: bool,
    }

    let mut sub_lines: Vec<SubLine> = Vec::new();
    for para in &paragraphs {
        // Flatten runs into atoms.
        let mut atoms: Vec<LayoutAtom> = Vec::new();
        for run in &para.runs {
            match &run.kind {
                MTextRunKind::Glyphs(text) => {
                    let mut word = String::new();
                    for ch in text.chars() {
                        if ch == ' ' || ch == '\u{00A0}' {
                            if !word.is_empty() {
                                atoms.push(LayoutAtom {
                                    kind: AtomKind::Word(std::mem::take(&mut word)),
                                    state: run.state.clone(),
                                });
                            }
                            atoms.push(LayoutAtom {
                                kind: AtomKind::Space,
                                state: run.state.clone(),
                            });
                        } else {
                            word.push(ch);
                        }
                    }
                    if !word.is_empty() {
                        atoms.push(LayoutAtom {
                            kind: AtomKind::Word(word),
                            state: run.state.clone(),
                        });
                    }
                }
                MTextRunKind::Tab => {
                    atoms.push(LayoutAtom {
                        kind: AtomKind::Tab,
                        state: run.state.clone(),
                    });
                }
            }
        }

        // Trim leading + trailing Space atoms — the legacy renderer dropped
        // edge whitespace via `String::trim()` after `strip_mtext_codes`, so
        // line_w / cursor_start agree on what counts as the paragraph's
        // visible content. Without this, a paragraph that ends with a stray
        // space measures wider than it draws and centering / right-alignment
        // appears off by half a space-width.
        let first_word = atoms
            .iter()
            .position(|a| !matches!(a.kind, AtomKind::Space))
            .unwrap_or(atoms.len());
        atoms.drain(..first_word);
        while matches!(atoms.last().map(|a| &a.kind), Some(AtomKind::Space)) {
            atoms.pop();
        }

        let wrapped = wrap_paragraph(
            atoms,
            rect_w,
            para.indent_first,
            para.indent_left,
            para.indent_right,
            &para.tab_stops,
            entity_h,
            base_wf,
            &base_font_name,
        );
        for (idx, atoms) in wrapped.into_iter().enumerate() {
            sub_lines.push(SubLine {
                atoms,
                align: para.align,
                indent_first: para.indent_first,
                indent_left: para.indent_left,
                indent_right: para.indent_right,
                tab_stops: para.tab_stops.clone(),
                is_first_in_paragraph: idx == 0,
            });
        }
    }
    if sub_lines.is_empty() {
        sub_lines.push(SubLine {
            atoms: Vec::new(),
            align: None,
            indent_first: 0.0,
            indent_left: 0.0,
            indent_right: 0.0,
            tab_stops: Vec::new(),
            is_first_in_paragraph: true,
        });
    }

    // ── 3. Block geometry (line spacing, attachment, rotation) ──────────────
    let n_lines = sub_lines.len().max(1) as f32;
    let ls_factor = if t.line_spacing_factor > 0.0 {
        t.line_spacing_factor as f32
    } else {
        1.0
    };
    // DXF code 44 — multiplier on the *default* baseline-to-baseline gap,
    // which AutoCAD defines as 5/3 × text height (≈ 1.667). 1.0 → single
    // spacing, 2.0 → double, etc.
    let line_h = entity_h * ls_factor * (5.0 / 3.0) * base_font.line_spacing;
    let h = entity_h;
    // CXF glyphs sit on the baseline (y=0) and extend UP by `h`. v_offset
    // is the Y of the first sub-line's baseline relative to the insertion
    // point; pick it from the attachment-point's vertical anchor:
    //   Top    → block top    at insertion → v_offset = −h
    //   Bottom → block bottom at insertion → v_offset = (n−1) · line_h
    //   Middle → midpoint of the two above
    let v_offset = match t.attachment_point {
        AttachmentPoint::TopLeft | AttachmentPoint::TopCenter | AttachmentPoint::TopRight => -h,
        AttachmentPoint::MiddleLeft
        | AttachmentPoint::MiddleCenter
        | AttachmentPoint::MiddleRight => ((n_lines - 1.0) * line_h - h) * 0.5,
        AttachmentPoint::BottomLeft
        | AttachmentPoint::BottomCenter
        | AttachmentPoint::BottomRight => (n_lines - 1.0) * line_h,
    };
    let attach_h_anchor: f32 = match t.attachment_point {
        AttachmentPoint::TopCenter
        | AttachmentPoint::MiddleCenter
        | AttachmentPoint::BottomCenter => 0.5,
        AttachmentPoint::TopRight | AttachmentPoint::MiddleRight | AttachmentPoint::BottomRight => {
            1.0
        }
        _ => 0.0,
    };
    let box_left = -attach_h_anchor * rect_w;
    let vertical_text = matches!(t.drawing_direction, DrawingDirection::TopToBottom);
    let rot = if resolved_style.is_upside_down {
        t.rotation as f32 + std::f32::consts::PI
    } else {
        t.rotation as f32
    };
    let (cos_r, sin_r) = (rot.cos(), rot.sin());
    let ins_x = t.insertion_point.x;
    let ins_y = t.insertion_point.y;
    let insertion = Vec3::new(ins_x as f32, ins_y as f32, t.insertion_point.z as f32);
    let mut all_strokes: Vec<TextStroke> = Vec::new();

    // ── 4. Render each sub-line ─────────────────────────────────────────────
    for (i, sub) in sub_lines.iter().enumerate() {
        let li = i as f32;
        let (line_base_x, line_base_y) = if vertical_text {
            let col_offset = li * entity_h * 1.2;
            (
                col_offset * cos_r + v_offset * (-sin_r),
                col_offset * sin_r + v_offset * cos_r,
            )
        } else {
            let ly = -(li * line_h) + v_offset;
            (ly * (-sin_r), ly * cos_r)
        };

        // Paragraph-content area: [content_left, content_right] relative to
        // insertion point. With rect_w == 0 we have no box, so fall back to
        // anchoring at the insertion point (legacy behaviour).
        let content_left = if rect_w > 0.0 {
            box_left + if sub.is_first_in_paragraph { sub.indent_first } else { sub.indent_left }
        } else {
            0.0
        };
        let content_right = if rect_w > 0.0 {
            box_left + rect_w - sub.indent_right
        } else {
            0.0
        };

        // Effective horizontal anchor for this sub-line: paragraph alignment
        // (when explicitly set inline) wins, otherwise inherit from the
        // entity attachment.
        let line_anchor: f32 = match sub.align {
            Some(ParagraphAlign::Left)
            | Some(ParagraphAlign::Justify)
            | Some(ParagraphAlign::Distribute) => 0.0,
            Some(ParagraphAlign::Center) => 0.5,
            Some(ParagraphAlign::Right) => 1.0,
            None => attach_h_anchor,
        };

        let line_w = line_total_width(
            &sub.atoms,
            entity_h,
            base_wf,
            &base_font_name,
            0.0,
            sub.indent_left,
            &sub.tab_stops,
        );

        // Cursor X (relative to insertion point, pre-rotation) where the
        // first atom starts. With a box: lay the line inside the paragraph
        // content area at `line_anchor`. Without a box: anchor the line at
        // the insertion point using the entity's attachment.
        let cursor_start = if rect_w > 0.0 {
            let content_w = (content_right - content_left).max(0.0);
            content_left + (content_w - line_w) * line_anchor
        } else if line_anchor > 0.0 {
            -line_w * line_anchor
        } else {
            0.0
        };

        // Line's tallest run height (for valign offsets).
        let line_max_h = sub
            .atoms
            .iter()
            .map(|a| a.state.height_mul * entity_h)
            .fold(entity_h, f32::max);

        // Walk atoms left → right, emitting one TextStroke per Word atom.
        let mut cursor_x = cursor_start;
        for atom in &sub.atoms {
            match &atom.kind {
                AtomKind::Word(text) => {
                    // Per-atom render parameters (composed with style baseline).
                    let run_h = atom.state.height_mul * entity_h;
                    let signed_wf = base_wf.signum() * atom.state.width_mul * base_wf.abs();
                    let oblique = base_oblique + atom.state.oblique_rad;
                    let font_name = resolve_font(&atom.state, &base_font_name);
                    let tracking = atom.state.tracking;
                    let valign_dy = match atom.state.valign {
                        1 => (line_max_h - run_h) * 0.5,
                        2 => line_max_h - run_h,
                        _ => 0.0,
                    };
                    let color = atom.state.color.as_ref().and_then(resolve_inline_color);
                    let body = decorated(text, &atom.state);

                    // Translate (cursor_x, valign_dy) into world space at this
                    // sub-line's baseline.
                    let lx = cursor_x;
                    let ly = valign_dy;
                    let world_dx = lx * cos_r - ly * sin_r;
                    let world_dy = lx * sin_r + ly * cos_r;
                    let origin: [f64; 2] = [
                        ins_x + (line_base_x + world_dx) as f64,
                        ins_y + (line_base_y + world_dy) as f64,
                    ];
                    let strokes = cxf::tessellate_text_run(
                        [0.0, 0.0],
                        run_h,
                        rot,
                        signed_wf,
                        oblique,
                        tracking,
                        font_name,
                        &body,
                    );
                    all_strokes.push(TextStroke {
                        strokes,
                        origin,
                        color,
                    });
                    cursor_x += measure_word(text, &atom.state, entity_h, base_wf, &base_font_name);
                }
                AtomKind::Space => {
                    cursor_x += measure_space(&atom.state, entity_h, base_wf, &base_font_name);
                }
                AtomKind::Tab => {
                    cursor_x = next_tab_position(
                        cursor_x,
                        &sub.tab_stops,
                        sub.indent_left,
                        entity_h,
                    );
                }
            }
        }
    }

    TruckEntity {
        object: TruckObject::Text(all_strokes),
        snap_pts: vec![(insertion, SnapHint::Insertion)],
        tangent_geoms: vec![],
        key_vertices: vec![],
        fill_tris: vec![],
    }
}

fn grips(t: &MText) -> Vec<GripDef> {
    let p = Vec3::new(
        t.insertion_point.x as f32,
        t.insertion_point.y as f32,
        t.insertion_point.z as f32,
    );
    let dir = Vec3::new((t.rotation as f32).cos(), (t.rotation as f32).sin(), 0.0);
    let width_grip = p + dir * t.rectangle_width.max(0.0) as f32;
    vec![square_grip(0, p), triangle_grip(1, width_grip)]
}

fn properties(t: &MText, text_style_names: &[String]) -> PropSection {
    PropSection {
        title: "Geometry".into(),
        props: vec![
            edit("Insert X", "ins_x", t.insertion_point.x),
            edit("Insert Y", "ins_y", t.insertion_point.y),
            edit("Insert Z", "ins_z", t.insertion_point.z),
            edit("Height", "height", t.height),
            edit("Width", "rect_w", t.rectangle_width),
            edit("Rect Height", "rect_h", t.rectangle_height.unwrap_or(0.0)),
            edit("Rotation", "rotation", t.rotation.to_degrees()),
            edit("Line Spacing", "line_spacing", t.line_spacing_factor),
            Property {
                label: "H-Align".into(),
                field: "h_align",
                value: PropValue::Choice {
                    selected: mtext_halign_str(&t.attachment_point).to_string(),
                    options: ["Left", "Center", "Right"]
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                },
            },
            Property {
                label: "V-Align".into(),
                field: "v_align",
                value: PropValue::Choice {
                    selected: mtext_valign_str(&t.attachment_point).to_string(),
                    options: ["Top", "Middle", "Bottom"]
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                },
            },
            ro(
                "Attachment",
                "attachment",
                attachment_str(&t.attachment_point).to_string(),
            ),
            ro(
                "Direction",
                "direction",
                drawing_dir_str(&t.drawing_direction).to_string(),
            ),
            Property {
                label: "Content".into(),
                field: "content",
                value: PropValue::EditText(t.value.clone()),
            },
            Property {
                label: "Style".into(),
                field: "style",
                value: PropValue::Choice {
                    selected: if t.style.trim().is_empty() {
                        "Standard".into()
                    } else {
                        t.style.clone()
                    },
                    options: text_style_names.to_vec(),
                },
            },
        ],
    }
}

fn apply_geom_prop(t: &mut MText, field: &str, value: &str) {
    match field {
        "content" => {
            t.value = value.to_string();
            return;
        }
        "style" => {
            t.style = value.to_string();
            return;
        }
        "h_align" => {
            if let Some(next) =
                mtext_attachment_from_align(value, mtext_valign_str(&t.attachment_point))
            {
                t.attachment_point = next;
            }
            return;
        }
        "v_align" => {
            if let Some(next) =
                mtext_attachment_from_align(mtext_halign_str(&t.attachment_point), value)
            {
                t.attachment_point = next;
            }
            return;
        }
        _ => {}
    }
    let Some(v) = crate::entities::common::parse_f64(value) else {
        return;
    };
    match field {
        "ins_x" => t.insertion_point.x = v,
        "ins_y" => t.insertion_point.y = v,
        "ins_z" => t.insertion_point.z = v,
        "height" if v > 0.0 => t.height = v,
        "rect_w" if v > 0.0 => t.rectangle_width = v,
        "rect_h" if v > 0.0 => t.rectangle_height = Some(v),
        "rotation" => t.rotation = v.to_radians(),
        "line_spacing" if v > 0.0 => t.line_spacing_factor = v,
        _ => {}
    }
}

fn apply_grip(t: &mut MText, grip_id: usize, apply: GripApply) {
    match (grip_id, apply) {
        (0, GripApply::Absolute(p)) => {
            t.insertion_point.x = p.x as f64;
            t.insertion_point.y = p.y as f64;
            t.insertion_point.z = p.z as f64;
        }
        (0, GripApply::Translate(d)) => {
            t.insertion_point.x += d.x as f64;
            t.insertion_point.y += d.y as f64;
            t.insertion_point.z += d.z as f64;
        }
        (1, GripApply::Absolute(p)) => {
            let dir_x = t.rotation.cos();
            let dir_y = t.rotation.sin();
            let dx = p.x as f64 - t.insertion_point.x;
            let dy = p.y as f64 - t.insertion_point.y;
            let projected = dx * dir_x + dy * dir_y;
            t.rectangle_width = projected.max(0.01);
        }
        _ => {}
    }
}

fn apply_transform(t: &mut MText, tr: &EntityTransform) {
    crate::scene::transform::apply_standard_entity_transform(t, tr, |entity, p1, p2| {
        crate::scene::transform::reflect_xy_point(
            &mut entity.insertion_point.x,
            &mut entity.insertion_point.y,
            p1,
            p2,
        );
        let dx = (p2.x - p1.x) as f64;
        let dy = (p2.y - p1.y) as f64;
        let line_angle = dy.atan2(dx);
        entity.rotation = 2.0 * line_angle - entity.rotation;
    });
}

impl TruckConvertible for MText {
    fn to_truck(&self, document: &acadrust::CadDocument) -> Option<TruckEntity> {
        Some(to_truck(self, document))
    }
}

impl Grippable for MText {
    fn grips(&self) -> Vec<GripDef> {
        grips(self)
    }

    fn apply_grip(&mut self, grip_id: usize, apply: GripApply) {
        apply_grip(self, grip_id, apply);
    }
}

impl PropertyEditable for MText {
    fn geometry_properties(&self, text_style_names: &[String]) -> PropSection {
        properties(self, text_style_names)
    }

    fn apply_geom_prop(&mut self, field: &str, value: &str) {
        apply_geom_prop(self, field, value);
    }
}

impl Transformable for MText {
    fn apply_transform(&mut self, t: &EntityTransform) {
        apply_transform(self, t);
    }
}
