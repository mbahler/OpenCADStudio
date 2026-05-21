use acadrust::CadDocument;

use crate::scene::cxf;

pub struct ResolvedTextStyle {
    pub font_name: String,
    pub width_factor: f32,
    pub oblique_angle: f32,
    pub is_backward: bool,
    pub is_upside_down: bool,
}

pub fn resolve_text_style(style_name: &str, document: &CadDocument) -> ResolvedTextStyle {
    let style = document.text_styles.iter().find(|entry| {
        entry.name.eq_ignore_ascii_case(style_name)
            || (style_name.trim().is_empty() && entry.name.eq_ignore_ascii_case("Standard"))
    });

    let font_name = if let Some(style) = style {
        if !style.font_file.trim().is_empty() {
            let file = style.font_file.trim();
            let basename = file.rsplit(['/', '\\']).next().unwrap_or(file);
            let stem = basename.split('.').next().unwrap_or(basename).trim();
            if !stem.is_empty() {
                stem.to_string()
            } else if !style.true_type_font.trim().is_empty() {
                style.true_type_font.trim().to_string()
            } else if !style.name.trim().is_empty() {
                style.name.trim().to_string()
            } else {
                "Standard".to_string()
            }
        } else if !style.true_type_font.trim().is_empty() {
            style.true_type_font.trim().to_string()
        } else if !style.name.trim().is_empty() {
            style.name.trim().to_string()
        } else {
            "Standard".to_string()
        }
    } else if style_name.trim().is_empty() {
        "Standard".to_string()
    } else {
        style_name.trim().to_string()
    };

    ResolvedTextStyle {
        font_name,
        width_factor: style.map(|s| s.width_factor as f32).unwrap_or(1.0),
        oblique_angle: style.map(|s| s.oblique_angle as f32).unwrap_or(0.0),
        is_backward: style.map(|s| s.is_backward()).unwrap_or(false),
        is_upside_down: style.map(|s| s.is_upside_down()).unwrap_or(false),
    }
}

pub fn text_local_bounds(
    font_name: &str,
    text: &str,
    height: f32,
    width_factor: f32,
    oblique_angle: f32,
) -> Option<([f32; 2], [f32; 2])> {
    if text.is_empty() || height <= 0.0 {
        return None;
    }

    let font = cxf::get_font(font_name);
    let scale = height / 9.0;
    let wf = width_factor.abs().clamp(0.01, 100.0);
    let ob = oblique_angle.tan();
    let mut cursor_x = 0.0_f32;
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for ch in text.chars() {
        if ch == ' ' {
            cursor_x += font.word_spacing;
            continue;
        }
        match font.glyph(ch) {
            Some(glyph) => {
                for stroke in &glyph.strokes {
                    for &[gx, gy] in stroke {
                        let sx = (cursor_x + gx) * scale * wf + gy * scale * ob;
                        let sy = gy * scale;
                        min_x = min_x.min(sx);
                        max_x = max_x.max(sx);
                        min_y = min_y.min(sy);
                        max_y = max_y.max(sy);
                    }
                }
                cursor_x += glyph.advance + font.letter_spacing;
            }
            None => {
                cursor_x += 6.0 + font.letter_spacing;
            }
        }
    }

    if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
        Some(([min_x, min_y], [max_x, max_y]))
    } else {
        None
    }
}

/// Expand DXF `%%x` special-character sequences that appear in both TEXT and MTEXT values:
/// - `%%d` / `%%D` → `°`
/// - `%%p` / `%%P` → `±`
/// - `%%c` / `%%C` → `⌀`
/// - `%%u` / `%%U` → underline toggle (stripped — not renderable with stroke fonts)
/// - `%%o` / `%%O` → overline toggle (stripped)
/// - `%%%%` → `%`
/// - `%%nnn` (3 decimal digits) → Unicode scalar `nnn`
/// Any unrecognised `%%x` is passed through unchanged.
pub fn resolve_dxf_special_chars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '%' || chars.peek() != Some(&'%') {
            out.push(c);
            continue;
        }
        chars.next(); // consume second '%'
        match chars.peek().map(|c| c.to_ascii_lowercase()) {
            Some('d') => {
                chars.next();
                out.push('°');
            }
            Some('p') => {
                chars.next();
                out.push('±');
            }
            Some('c') => {
                chars.next();
                out.push('⌀');
            }
            Some('u') | Some('o') => {
                chars.next();
            } // toggle codes — strip silently
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some(d) if d.is_ascii_digit() => {
                let mut digits = String::with_capacity(3);
                for _ in 0..3 {
                    match chars.peek() {
                        Some(&ch) if ch.is_ascii_digit() => {
                            digits.push(chars.next().unwrap());
                        }
                        _ => break,
                    }
                }
                if digits.len() == 3 {
                    if let Ok(n) = digits.parse::<u32>() {
                        if let Some(ch) = char::from_u32(n) {
                            out.push(ch);
                            continue;
                        }
                    }
                }
                out.push('%');
                out.push('%');
                out.push_str(&digits);
            }
            _ => {
                out.push('%');
                out.push('%');
            }
        }
    }

    out
}

// ──────────────────────────────────────────────────────────────────────────
// Rich MTEXT parser — full inline format-code coverage
//
// Recognised codes (DXF MTEXT inline):
//   Escapes:  \\  \{  \}  \~  \t  \P  \n  \N  \U+XXXX  \u+XXXX
//   Toggles:  \L\l  \O\o  \K\k  (underline / overline / strike)
//   State:    \H<v>[x];  \W<v>[x];  \Q<v>;  \T<v>[x];  \A<n>;
//             \C<aci>;   \c<rgb>;
//             \f<name>|b<0/1>|i<0/1>|c<n>|p<n>;   \F<file>;
//             \M+<n>;    \X   \S<u><sep><l>;
//   Paragraph: \p[xi<v>,l<v>,r<v>,q[lcrjd],t<positions>,s<v>...];
//   Scope:    { ... }   push/pop full state
// ──────────────────────────────────────────────────────────────────────────

/// Paragraph alignment encoded inline via `\p...q[lcrjd]...;`.
/// `Justify` / `Distribute` render as `Left` (full inter-word redistribution
/// is not implemented in the stroke renderer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParagraphAlign {
    Left,
    Center,
    Right,
    Justify,
    Distribute,
}

/// Inline colour override (`\C` ACI or `\c` 24-bit true colour). Resolved to
/// linear RGB at render time via the document's ACI table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InlineColor {
    Aci(u8),
    True([f32; 3]),
}

/// Tab-stop alignment kind (from `\pt<L|C|R><pos>` entries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabStop {
    pub position: f32,
    pub kind: TabKind,
}

/// Per-run formatting state. All fields are multipliers / overrides relative
/// to the entity-level defaults; the renderer composes them with the resolved
/// text style at draw time.
#[derive(Clone, Debug, PartialEq)]
pub struct RunState {
    /// Multiplier on entity text height (`\H<v>x;` → ×v; `\H<v>;` → v / entity_h)
    pub height_mul: f32,
    /// Multiplier on the (signed) style width-factor (`\W<v>;` → set, `\Wx;` → ×)
    pub width_mul: f32,
    /// Absolute oblique angle override in radians (`\Q<deg>;`)
    pub oblique_rad: f32,
    /// Tracking multiplier on `font.letter_spacing` (`\T<v>;`)
    pub tracking: f32,
    /// Vertical alignment of the run within its line box (0=baseline / 1=center
    /// / 2=top). Mainly used for fractions and superscript-like layout (`\A`).
    pub valign: u8,
    /// Font-name override, `None` ⇒ inherit the resolved style font.
    pub font: Option<String>,
    /// Colour override, `None` ⇒ inherit entity colour.
    pub color: Option<InlineColor>,
    pub underline: bool,
    pub overline: bool,
    pub strike: bool,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            height_mul: 1.0,
            width_mul: 1.0,
            oblique_rad: 0.0,
            tracking: 1.0,
            valign: 0,
            font: None,
            color: None,
            underline: false,
            overline: false,
            strike: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MTextRunKind {
    /// Renderable glyph text (DXF specials resolved, decoration markers stripped).
    Glyphs(String),
    /// `\t` — jump the cursor to the next tab stop (or default tab interval).
    Tab,
}

#[derive(Clone, Debug)]
pub struct MTextRun {
    pub kind: MTextRunKind,
    pub state: RunState,
}

/// One paragraph of MTEXT after parsing. Each line is a sequence of runs that
/// share text content + a snapshot of formatting state, plus paragraph-level
/// layout (alignment, indents, tab stops). `\P` / `\n` / `\N` start a new
/// line; paragraph properties carry forward until the next `\p...;` block.
#[derive(Clone, Debug, Default)]
pub struct MTextLine {
    pub runs: Vec<MTextRun>,
    pub align: Option<ParagraphAlign>,
    pub indent_first: f32,
    pub indent_left: f32,
    pub indent_right: f32,
    pub tab_stops: Vec<TabStop>,
}

impl MTextLine {
    /// Concatenated glyph text across all runs (decorations / tabs ignored).
    /// Useful for callers that only need an unstyled string (search, hit-test
    /// previews, fallback paths).
    pub fn plain_text(&self) -> String {
        let mut s = String::new();
        for r in &self.runs {
            if let MTextRunKind::Glyphs(t) = &r.kind {
                s.push_str(t);
            }
        }
        s
    }

    pub fn is_blank(&self) -> bool {
        self.runs.iter().all(|r| match &r.kind {
            MTextRunKind::Glyphs(t) => t.trim().is_empty(),
            MTextRunKind::Tab => false,
        })
    }
}

#[derive(Clone, Debug, Default)]
struct ParagraphProps {
    align: Option<ParagraphAlign>,
    indent_first: f32,
    indent_left: f32,
    indent_right: f32,
    tab_stops: Vec<TabStop>,
}

/// Parse a `\p...;` body. Comma-separated tokens, each with a single-letter
/// kind prefix. Unknown tokens are skipped — anything we don't understand is
/// silently dropped rather than poisoning the rest of the paragraph.
fn parse_paragraph_block(body: &str, props: &mut ParagraphProps) {
    // The legacy AutoCAD writer prefixes the block with a redundant `x`; skip
    // it so it doesn't confuse the kind matcher.
    let body = body.strip_prefix('x').unwrap_or(body);
    for token in body.split(',') {
        let token = token.trim();
        let mut chars = token.chars();
        let Some(kind) = chars.next() else { continue };
        let rest: String = chars.collect();
        match kind {
            'q' | 'Q' => {
                let sel = rest.chars().next().map(|c| c.to_ascii_lowercase());
                props.align = match sel {
                    Some('l') => Some(ParagraphAlign::Left),
                    Some('c') => Some(ParagraphAlign::Center),
                    Some('r') => Some(ParagraphAlign::Right),
                    Some('j') => Some(ParagraphAlign::Justify),
                    Some('d') => Some(ParagraphAlign::Distribute),
                    _ => props.align,
                };
            }
            'i' => {
                if let Ok(v) = rest.parse::<f32>() {
                    props.indent_first = v;
                }
            }
            'l' => {
                if let Ok(v) = rest.parse::<f32>() {
                    props.indent_left = v;
                }
            }
            'r' => {
                if let Ok(v) = rest.parse::<f32>() {
                    props.indent_right = v;
                }
            }
            't' => {
                // Tab list. Each entry may be prefixed with L / C / R to
                // pick the tab kind; default is Left. The remainder is the
                // position in drawing units.
                props.tab_stops.clear();
                for entry in rest.split(',') {
                    let entry = entry.trim();
                    if entry.is_empty() {
                        continue;
                    }
                    let (kind, num_str) = match entry.chars().next() {
                        Some(c @ ('L' | 'l')) => (TabKind::Left, &entry[c.len_utf8()..]),
                        Some(c @ ('C' | 'c')) => (TabKind::Center, &entry[c.len_utf8()..]),
                        Some(c @ ('R' | 'r')) => (TabKind::Right, &entry[c.len_utf8()..]),
                        _ => (TabKind::Left, entry),
                    };
                    if let Ok(p) = num_str.parse::<f32>() {
                        props.tab_stops.push(TabStop { position: p, kind });
                    }
                }
            }
            's' => {} // space-before — affects line spacing; ignored for now
            _ => {}
        }
    }
}

/// Parse an unsigned 24-bit true-colour value into linear-ish [r,g,b] floats.
/// AutoCAD packs `\c` as a 24-bit decimal: high byte = R, mid = G, low = B.
fn parse_true_color(s: &str) -> Option<InlineColor> {
    let n: u32 = s.trim().parse().ok()?;
    let r = ((n >> 16) & 0xFF) as f32 / 255.0;
    let g = ((n >> 8) & 0xFF) as f32 / 255.0;
    let b = (n & 0xFF) as f32 / 255.0;
    Some(InlineColor::True([r, g, b]))
}

/// Parse `\H` / `\W` / `\T` value. Returns `(value, is_relative)`; a trailing
/// `x` marks a multiplier on the current state, otherwise the value is the
/// absolute target (interpreted by the caller against entity defaults).
fn parse_scalar_with_x_suffix(body: &str) -> Option<(f32, bool)> {
    let body = body.trim();
    let (num, is_rel) = if let Some(stripped) = body.strip_suffix('x').or_else(|| body.strip_suffix('X')) {
        (stripped, true)
    } else {
        (body, false)
    };
    Some((num.trim().parse::<f32>().ok()?, is_rel))
}

/// Flush the glyph buffer as a `Glyphs` run on `line` using a snapshot of
/// `state`. Resolves DXF `%%x` specials (degree / diameter / `%%nnn` literal
/// unicode) so the renderer sees fully decoded text.
fn flush_glyph_buf(line: &mut MTextLine, buf: &mut String, state: &RunState) {
    if buf.is_empty() {
        return;
    }
    let text = resolve_dxf_special_chars(&std::mem::take(buf));
    line.runs.push(MTextRun {
        kind: MTextRunKind::Glyphs(text),
        state: state.clone(),
    });
}

/// Walk the MTEXT value string and produce one [`MTextLine`] per visible
/// paragraph (after stripping leading / trailing blank lines, as the legacy
/// `split_mtext_lines` does). Every inline format code listed in the module
/// header is recognised; unknown semicolon-terminated codes are stripped
/// silently so future / vendor-specific extensions don't pollute the text.
///
/// `entity_height` is needed to translate absolute `\H<v>;` declarations into
/// the height-multiplier representation carried in [`RunState`]; pass the
/// MTEXT entity's `height` field.
pub fn parse_mtext_paragraphs(s: &str, entity_height: f32) -> Vec<MTextLine> {
    let mut lines: Vec<MTextLine> = Vec::new();
    let mut current = MTextLine::default();
    let mut buf = String::new();
    let mut state = RunState::default();
    let mut state_stack: Vec<RunState> = Vec::new();
    let mut props = ParagraphProps::default();
    // No props_stack — paragraph props persist across braces by design.
    let entity_height = entity_height.max(1e-6);

    let mut chars = s.chars().peekable();

    let read_until_semi = |chars: &mut std::iter::Peekable<std::str::Chars>| -> String {
        let mut out = String::new();
        for c in chars.by_ref() {
            if c == ';' {
                break;
            }
            out.push(c);
        }
        out
    };

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.peek().copied() {
                // ── Line / paragraph break ────────────────────────────────
                Some('P') | Some('n') | Some('N') => {
                    chars.next();
                    flush_glyph_buf(&mut current, &mut buf, &state);
                    current.align = props.align;
                    current.indent_first = props.indent_first;
                    current.indent_left = props.indent_left;
                    current.indent_right = props.indent_right;
                    current.tab_stops = props.tab_stops.clone();
                    lines.push(std::mem::take(&mut current));
                }
                // ── Whitespace literals ───────────────────────────────────
                Some('~') => {
                    chars.next();
                    buf.push('\u{00A0}'); // nbsp — treated as a regular space by the wrap pass
                }
                Some('t') => {
                    chars.next();
                    flush_glyph_buf(&mut current, &mut buf, &state);
                    current.runs.push(MTextRun {
                        kind: MTextRunKind::Tab,
                        state: state.clone(),
                    });
                }
                // ── Unicode by hex code point ────────────────────────────
                Some('U') | Some('u') => {
                    chars.next();
                    if chars.peek() == Some(&'+') {
                        chars.next();
                        let mut hex = String::with_capacity(6);
                        for _ in 0..6 {
                            match chars.peek() {
                                Some(&c) if c.is_ascii_hexdigit() => {
                                    hex.push(chars.next().unwrap());
                                }
                                _ => break,
                            }
                        }
                        if chars.peek() == Some(&';') {
                            chars.next();
                        }
                        if let Ok(n) = u32::from_str_radix(&hex, 16) {
                            if let Some(c) = char::from_u32(n) {
                                buf.push(c);
                                continue;
                            }
                        }
                    } else {
                        // Bare `\U` / `\u` — strip to next `;`
                        let _ = read_until_semi(&mut chars);
                    }
                }
                // ── Stacked text \S<u><sep><l>; ──────────────────────────
                Some('S') | Some('s') => {
                    chars.next();
                    let mut upper = String::new();
                    let mut lower = String::new();
                    let mut sep = '/';
                    let mut in_lower = false;
                    for c in chars.by_ref() {
                        if c == ';' {
                            break;
                        }
                        if !in_lower && (c == '/' || c == '^' || c == '#') {
                            sep = c;
                            in_lower = true;
                        } else if in_lower {
                            lower.push(c);
                        } else {
                            upper.push(c);
                        }
                    }
                    buf.push_str(&upper);
                    if !lower.is_empty() {
                        buf.push(if sep == '#' { '/' } else { sep });
                        buf.push_str(&lower);
                    }
                }
                // ── Decoration toggles (state, not markers) ──────────────
                Some('L') => { chars.next(); flush_glyph_buf(&mut current, &mut buf, &state); state.underline = true; }
                Some('l') => { chars.next(); flush_glyph_buf(&mut current, &mut buf, &state); state.underline = false; }
                Some('O') => { chars.next(); flush_glyph_buf(&mut current, &mut buf, &state); state.overline = true; }
                Some('o') => { chars.next(); flush_glyph_buf(&mut current, &mut buf, &state); state.overline = false; }
                Some('K') => { chars.next(); flush_glyph_buf(&mut current, &mut buf, &state); state.strike = true; }
                Some('k') => { chars.next(); flush_glyph_buf(&mut current, &mut buf, &state); state.strike = false; }
                // ── Literal backslash / braces ───────────────────────────
                Some('\\') => { chars.next(); buf.push('\\'); }
                Some('{') | Some('}') => { buf.push(chars.next().unwrap()); }
                // ── Paragraph props ──────────────────────────────────────
                Some('p') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    parse_paragraph_block(&body, &mut props);
                }
                // ── Height ───────────────────────────────────────────────
                Some('H') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    if let Some((v, is_rel)) = parse_scalar_with_x_suffix(&body) {
                        flush_glyph_buf(&mut current, &mut buf, &state);
                        if is_rel {
                            state.height_mul *= v;
                        } else {
                            state.height_mul = v / entity_height;
                        }
                    }
                }
                // ── Width factor ─────────────────────────────────────────
                Some('W') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    if let Some((v, is_rel)) = parse_scalar_with_x_suffix(&body) {
                        flush_glyph_buf(&mut current, &mut buf, &state);
                        if is_rel {
                            state.width_mul *= v;
                        } else {
                            state.width_mul = v;
                        }
                    }
                }
                // ── Oblique angle (degrees → radians) ────────────────────
                Some('Q') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    if let Ok(deg) = body.trim().parse::<f32>() {
                        flush_glyph_buf(&mut current, &mut buf, &state);
                        state.oblique_rad = deg.to_radians();
                    }
                }
                // ── Tracking ─────────────────────────────────────────────
                Some('T') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    if let Some((v, is_rel)) = parse_scalar_with_x_suffix(&body) {
                        flush_glyph_buf(&mut current, &mut buf, &state);
                        if is_rel {
                            state.tracking *= v;
                        } else {
                            state.tracking = v;
                        }
                    }
                }
                // ── Vertical alignment ───────────────────────────────────
                Some('A') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    if let Ok(n) = body.trim().parse::<u8>() {
                        flush_glyph_buf(&mut current, &mut buf, &state);
                        state.valign = n.min(2);
                    }
                }
                // ── ACI colour ───────────────────────────────────────────
                Some('C') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    if let Ok(n) = body.trim().parse::<u32>() {
                        flush_glyph_buf(&mut current, &mut buf, &state);
                        state.color = Some(InlineColor::Aci(n.min(255) as u8));
                    }
                }
                // ── True colour ──────────────────────────────────────────
                Some('c') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    if let Some(col) = parse_true_color(&body) {
                        flush_glyph_buf(&mut current, &mut buf, &state);
                        state.color = Some(col);
                    }
                }
                // ── Font (name + b/i/c/p flags) ──────────────────────────
                Some('f') | Some('F') => {
                    chars.next();
                    let body = read_until_semi(&mut chars);
                    // First `|`-separated field is the font name / file stem.
                    let name = body.split('|').next().unwrap_or("").trim();
                    if !name.is_empty() {
                        flush_glyph_buf(&mut current, &mut buf, &state);
                        // Strip extension if `\F` passed a file path.
                        let stem = name
                            .rsplit(['/', '\\'])
                            .next()
                            .unwrap_or(name)
                            .split('.')
                            .next()
                            .unwrap_or(name);
                        state.font = Some(stem.to_string());
                    }
                }
                // ── Multibyte / codepage marker — strip silently ─────────
                Some('M') => {
                    chars.next();
                    let _ = read_until_semi(&mut chars);
                }
                // ── Dimension MTEXT paragraph-end marker — strip silently
                Some('X') => {
                    chars.next();
                }
                // ── Unknown single-letter escape ─────────────────────────
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            // Scope push / pop — braces scope *character* state (font,
            // colour, height, decorations, …). Paragraph properties
            // (`\p...;`) are intentionally NOT scoped: AutoCAD treats them
            // as paragraph-level layout that persists across braces, and
            // real-world files routinely wrap inline `\pxqc;` inside a
            // `{\fArial;…}` block while expecting the alignment to apply
            // to the whole paragraph.
            '{' => {
                state_stack.push(state.clone());
            }
            '}' => {
                if let Some(prev) = state_stack.pop() {
                    flush_glyph_buf(&mut current, &mut buf, &state);
                    state = prev;
                }
            }
            '\r' => {}
            '\n' => {
                flush_glyph_buf(&mut current, &mut buf, &state);
                current.align = props.align;
                current.indent_first = props.indent_first;
                current.indent_left = props.indent_left;
                current.indent_right = props.indent_right;
                current.tab_stops = props.tab_stops.clone();
                lines.push(std::mem::take(&mut current));
            }
            other => buf.push(other),
        }
    }
    flush_glyph_buf(&mut current, &mut buf, &state);
    current.align = props.align;
    current.indent_first = props.indent_first;
    current.indent_left = props.indent_left;
    current.indent_right = props.indent_right;
    current.tab_stops = props.tab_stops.clone();
    lines.push(current);

    let start = lines.iter().position(|l| !l.is_blank()).unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|l| !l.is_blank())
        .map(|i| i + 1)
        .unwrap_or(0);
    lines[start..end].to_vec()
}

pub fn strip_mtext_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.peek().copied() {
                // Paragraph / line breaks
                Some('P') | Some('n') | Some('N') => {
                    chars.next();
                    out.push('\n');
                }
                // Non-breaking space → regular space (preserves spacing, simpler for word-wrap)
                Some('~') => {
                    chars.next();
                    out.push(' ');
                }
                // Tab
                Some('t') => {
                    chars.next();
                    out.push_str("    ");
                }
                // \U+XXXX or \u+XXXX — Unicode character by hex code point.
                // Up to 6 hex digits; optional trailing semicolon.
                // NOTE: 'U'/'u' were previously in the "strip until ;" list — that was wrong.
                Some('U') | Some('u') => {
                    chars.next();
                    if chars.peek() == Some(&'+') {
                        chars.next(); // consume '+'
                        let mut hex = String::with_capacity(6);
                        for _ in 0..6 {
                            match chars.peek() {
                                Some(&c) if c.is_ascii_hexdigit() => {
                                    hex.push(chars.next().unwrap());
                                }
                                _ => break,
                            }
                        }
                        if chars.peek() == Some(&';') {
                            chars.next(); // consume optional trailing semicolon
                        }
                        if let Ok(n) = u32::from_str_radix(&hex, 16) {
                            if let Some(decoded) = char::from_u32(n) {
                                out.push(decoded);
                                continue;
                            }
                        }
                        // Undecodable — silently drop
                    } else {
                        // \U or \u without '+' — strip until semicolon
                        for c in chars.by_ref() {
                            if c == ';' {
                                break;
                            }
                        }
                    }
                }
                // \S<upper><sep><lower>; — stacked text (fraction / tolerance).
                // sep is '/' (diagonal fraction), '^' (stacked with bar), '#' (horizontal bar).
                // Render as "upper/lower" or "upper^lower" since stroke fonts can't stack.
                Some('S') | Some('s') => {
                    chars.next();
                    let mut upper = String::new();
                    let mut lower = String::new();
                    let mut sep = '/';
                    let mut in_lower = false;
                    for c in chars.by_ref() {
                        if c == ';' {
                            break;
                        }
                        if !in_lower && (c == '/' || c == '^' || c == '#') {
                            sep = c;
                            in_lower = true;
                        } else if in_lower {
                            lower.push(c);
                        } else {
                            upper.push(c);
                        }
                    }
                    out.push_str(&upper);
                    if !lower.is_empty() {
                        out.push(if sep == '#' { '/' } else { sep });
                        out.push_str(&lower);
                    }
                }
                // Decoration toggles — keep as \X markers so the tessellator can
                // emit underline / overline / strikethrough strokes.
                Some('L') | Some('l') | Some('O') | Some('o') | Some('K') | Some('k') => {
                    out.push('\\');
                    out.push(chars.next().unwrap());
                }
                // Literal backslash
                Some('\\') => {
                    chars.next();
                    out.push('\\');
                }
                // Literal braces
                Some('{') | Some('}') => {
                    out.push(chars.next().unwrap());
                }
                // In-line codes with semicolon-terminated arguments — strip entirely.
                //   p = paragraph format   H = height       W = width factor
                //   Q = oblique angle      T = tracking     A = alignment
                //   C = ACI color          c = true color   f/F = font change
                //   M = DBCS multibyte     X = paragraph-align end (arg-less but safe to strip)
                Some(c) if "pHWQTACcfFMX".contains(c) => {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == ';' {
                            break;
                        }
                    }
                }
                // Unknown escape — consume and silently discard the code character so
                // it does not appear as a literal in the output.
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            // Strip brace grouping markers (scope delimiters for in-line formatting)
            '{' | '}' => {}
            '\r' => {}
            other => out.push(other),
        }
    }

    resolve_dxf_special_chars(&out)
}

pub fn split_mtext_lines(s: &str) -> Vec<String> {
    let lines: Vec<String> = s.split('\n').map(|l| l.trim().to_string()).collect();
    // Drop leading and trailing blank lines, but preserve blank lines in the
    // middle — they are intentional paragraph separators (\\P\\P in MTEXT).
    let start = lines.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);
    lines[start..end].to_vec()
}

/// Measure the advance width of an MText line (after strip_mtext_codes), correctly
/// skipping decoration toggle markers (`\L`, `\l`, `\O`, `\o`, `\K`, `\k`).
pub fn measure_mtext_chars(text: &str, scale: f32, font: &cxf::CxfFile) -> f32 {
    let mut width = 0.0_f32;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && matches!(chars.peek(), Some('L' | 'l' | 'O' | 'o' | 'K' | 'k')) {
            chars.next();
            continue;
        }
        if c == ' ' {
            width += font.word_spacing * scale;
        } else {
            width += font
                .glyph(c)
                .map(|g| (g.advance + font.letter_spacing) * scale)
                .unwrap_or(scale * 6.0);
        }
    }
    width
}

/// Total number of visible lines an MText renders to — explicit `\P` /
/// `\n` / `\N` breaks plus word-wrap induced sublines when
/// `rectangle_width > 0`. Mirrors the line-splitting `entities/mtext.rs`
/// and `tessellate_multileader` perform; lets renderers split the OBB
/// into per-row LOD primitives without re-running the wrap measurement.
pub fn mtext_line_count(
    m: &acadrust::entities::MText,
    document: &CadDocument,
    anno_scale: f32,
) -> usize {
    let resolved = resolve_text_style(&m.style, document);
    let font = cxf::get_font(&resolved.font_name);
    let width_factor = resolved.width_factor.max(0.01);
    let height = m.height as f32 * anno_scale;
    let plain = strip_mtext_codes(&m.value);
    let explicit = split_mtext_lines(&plain);
    let total: usize = if m.rectangle_width > 0.0 && height > 0.0 {
        let scale = height / 9.0 * width_factor;
        let max_w = m.rectangle_width as f32 * anno_scale;
        explicit
            .iter()
            .map(|l| word_wrap(l, max_w, scale, font).len())
            .sum()
    } else {
        explicit.len()
    };
    total.max(1)
}

pub fn word_wrap(text: &str, max_w: f32, scale: f32, font: &'static cxf::CxfFile) -> Vec<String> {
    if max_w <= 0.0 || text.is_empty() {
        return vec![text.to_string()];
    }

    let space_w = font.word_spacing * scale;
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0.0_f32;

    for word in text.split(' ') {
        let word_w = measure_mtext_chars(word, scale, font);
        let gap = if current.is_empty() { 0.0 } else { space_w };
        if !current.is_empty() && current_w + gap + word_w > max_w {
            lines.push(std::mem::take(&mut current));
            current_w = 0.0;
        }
        if !current.is_empty() {
            current.push(' ');
            current_w += space_w;
        }
        current.push_str(word);
        current_w += word_w;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
