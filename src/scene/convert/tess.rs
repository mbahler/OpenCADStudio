// Auto-split from scene/mod.rs. Pure text-move; behaviour unchanged.
use super::super::*;

/// Dim an already-bg-adapted colour toward the background when the entity's
/// layer is locked, so locked objects read as non-editable (they stay visible
/// and snappable). No-op for unlocked layers.
fn fade_if_locked(
    document: &acadrust::CadDocument,
    e: &EntityType,
    color: [f32; 4],
    bg: [f32; 4],
) -> [f32; 4] {
    if view::render::layer_locked(document, e) {
        crate::scene::cache::block_cache::fade_toward_bg(color, bg)
    } else {
        color
    }
}

// ── Parallel tessellation free function ──────────────────────────────────────
//
// Takes only the `Send + Sync` data needed for tessellation so that
// `wires_for_block` can dispatch work across rayon's thread pool without
// requiring `Scene` (which contains `Rc<RefCell<...>>` and is `!Send`) to
// cross thread boundaries.


/// Tessellate a synthesised dimension-text entity through `tessellate_entity`
/// so it picks up the standard text LOD ladder (baseline / greek / full),
/// then re-color the returned wires with the dimension's resolved text colour
/// (so DIMCLRT / DIMSTYLE colours win over the synthetic Text's defaults).
pub(crate) fn tessellate_entity_dim_text(
    document: &acadrust::CadDocument,
    selected: &HashSet<Handle>,
    active_viewport: Option<Handle>,
    bg_color: [f32; 4],
    anno_scale: f32,
    e: &EntityType,
    view_aabb: Option<[f32; 4]>,
    world_per_pixel: Option<f32>,
    text_color: [f32; 4],
) -> Vec<WireModel> {
    let mut wires = tessellate_entity(
        document, selected, active_viewport, bg_color,
        anno_scale, e, None, view_aabb, world_per_pixel,
    );
    for w in &mut wires {
        // Synth dim text carries no real entity colour — paint everything
        // (including greek-LOD fill tris which read `wire.color`) with the
        // dim's text colour. Selection highlight already baked in by
        // tessellate_entity, so leave that alone.
        if !w.selected {
            w.color = text_color;
        }
    }
    wires
}

pub(crate) fn tessellate_entity(
    document: &acadrust::CadDocument,
    selected: &HashSet<Handle>,
    active_viewport: Option<Handle>,
    bg_color: [f32; 4],
    anno_scale: f32,
    e: &EntityType,
    block_cache: Option<&cache::block_cache::BlockCache>,
    // World-space XY view AABB (post `world_offset` subtraction). When
    // `Some`, entities whose AABB doesn't intersect this rect are skipped.
    view_aabb: Option<[f32; 4]>,
    // World units per screen pixel for LOD culling. `None` = no LOD.
    world_per_pixel: Option<f32>,
) -> Vec<WireModel> {
    let h = e.common().handle;
    let sel = selected.contains(&h);

    // Frustum + LOD cull for non-Insert, non-Viewport entities. Insert is
    // handled separately (its WCS bbox depends on the block defn AABB ×
    // Insert transform — done inside expand_insert). Viewports always emit
    // so the viewport frame stays visible regardless of zoom.
    let needs_cull = view_aabb.is_some() || world_per_pixel.is_some();
    if needs_cull {
        match e {
            EntityType::Viewport(_) | EntityType::Insert(_) => {}
            _ => {
                let ab = entity_aabb(e);
                if ab != WireModel::UNBOUNDED_AABB {
                    if let Some(view) = view_aabb {
                        if cache::block_cache::aabb_disjoint_xy(ab, view) {
                            return vec![];
                        }
                    }
                    if let Some(wpp) = world_per_pixel {
                        let w_px = (ab[2] - ab[0]).abs();
                        let h_px = (ab[3] - ab[1]).abs();
                        // Keep in sync with `cache::block_cache::MIN_PIXEL_SIZE`.
                        // Text entities render as SDF glyph quads (crisp at every
                        // zoom, no LOD), so they must reach the full path even at
                        // sub-5 px — never substitute the stub.
                        let is_text = matches!(
                            e,
                            EntityType::Text(_)
                                | EntityType::MText(_)
                                | EntityType::AttributeDefinition(_)
                                | EntityType::AttributeEntity(_)
                                | EntityType::Tolerance(_)
                        );
                        // Face3D is exempt from the sub-pixel stub: it is trivially
                        // cheap to tessellate (4 corners → 2 tris), so there is no
                        // cost to draw it full at any zoom, and the cube-stub
                        // otherwise pops/coarsens flat faces across the threshold.
                        let is_face3d = matches!(e, EntityType::Face3D(_));
                        let is_3d_entity = matches!(
                            e,
                            EntityType::Solid3D(_)
                                | EntityType::Mesh(_)
                                | EntityType::PolyfaceMesh(_)
                                | EntityType::PolygonMesh(_)
                                | EntityType::Body(_)
                                | EntityType::Region(_)
                                | EntityType::Surface(_)
                        );
                        if !is_text && !is_face3d && w_px.max(h_px) / wpp < 5.0 {
                            // Sub-pixel entity: emit a stub instead of
                            // nothing so it stays visible / selectable /
                            // hit-test'able at any zoom. 2-D entities
                            // get the cheap diagonal segment; 3-D
                            // entities get an AABB cube so their
                            // footprint doesn't drift when the camera
                            // crosses the LOD threshold. See #19.
                            let (entity_color, _, _, _, aci_idx) =
                                view::render::render_style_for(document, e);
                            let entity_color = view::render::adapt_to_bg(entity_color, bg_color);
                            let entity_color = fade_if_locked(document, e, entity_color, bg_color);
                            if is_3d_entity {
                                // `ab` is already in the local frame
                                // (entity_aabb subtracted world_offset
                                // XY). The bbox z fields are still in
                                // WCS, so subtract `world_offset[2]` to
                                // match — otherwise the stub sits at a
                                // different z than the full tessellation
                                // and the geometry visibly shifts when
                                // the camera crosses the LOD threshold.
                                let bbox = e.as_entity().bounding_box();
                                let oz = 0.0_f64;
                                let z_min = (bbox.min.z - oz) as f32;
                                let z_max = (bbox.max.z - oz) as f32;
                                return vec![lod_stub_wire_3d(
                                    h.value().to_string(),
                                    entity_color,
                                    sel,
                                    aci_idx,
                                    ab,
                                    z_min,
                                    z_max,
                                )];
                            }
                            return vec![lod_stub_wire(
                                h.value().to_string(),
                                entity_color,
                                sel,
                                aci_idx,
                                ab,
                                0.0,
                                0.0,
                            )];
                        }
                    }
                }
            }
        }
    }

    if let EntityType::Viewport(vp) = e {
        // The sheet viewport (overall/id=1) is never shown — it represents the
        // paper boundary, not a user-defined content window.
        if !Scene::is_content_viewport(vp) {
            return vec![];
        }
        let is_active = active_viewport == Some(h);
        let is_locked = vp.status.locked;
        let color = if sel {
            [1.0, 1.0, 1.0, 1.0]
        } else if is_active {
            [1.0, 0.90, 0.20, 1.0]
        } else if is_locked {
            [0.90, 0.55, 0.10, 1.0]
        } else {
            [0.0, 0.75, 0.75, 1.0]
        };
        let (pattern_length, pattern) = if is_active {
            (1.5_f32, [0.8, -0.4, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0_f32])
        } else {
            (0.0_f32, [0.0f32; 8])
        };
        let mut wires = convert::tessellate::tessellate(
            document,
            h,
            e,
            sel,
            color,
            pattern_length,
            pattern,
            1.5,
            1.0,
            world_per_pixel,
            bg_color,
            false,
        );
        let ab = entity_aabb(e);
        for w in &mut wires {
            w.aabb = ab;
        }
        return wires;
    }

    let (entity_color, pattern_length, pattern, line_weight_px, aci) =
        view::render::render_style_for(document, e);
    let entity_color = view::render::adapt_to_bg(entity_color, bg_color);
    let entity_color = fade_if_locked(document, e, entity_color, bg_color);
    let lt_scale = document.header.linetype_scale as f32 * e.common().linetype_scale as f32;
    let lt_name = view::render::linetype_name_for(document, e);
    // PSLTSCALE: scale linetype dashes by viewport anno_scale so they appear uniform in paper space.
    let pslt_factor = if document.header.paper_space_linetype_scaling {
        anno_scale
    } else {
        1.0
    };
    let pattern_length = pattern_length * pslt_factor;
    let pattern = pattern.map(|v| v * pslt_factor);

    // ── Dimension baked-block fast path ─────────────────────────────────────
    //
    // AutoCAD bakes each dimension's final geometry (extension lines, dim
    // line, arrows, text MText) into a per-instance block — usually
    // `*D<n>`, but custom names like `DIMBLOCK###-4NP` also occur. When the
    // block exists we render its contents through `tessellate_entity` so
    // sub-Text/MText get the standard baseline/greek/full LOD ladder, and
    // DIMTXT × DIMSCALE isn't re-applied on already-baked geometry.
    if let EntityType::Dimension(dim) = e {
        let block_name = &dim.base().block_name;
        if !block_name.trim().is_empty() {
            if let Some(br) = document
                .block_records
                .iter()
                .find(|br| br.name.eq_ignore_ascii_case(block_name))
            {
                if !br.entity_handles.is_empty() {
                    let mut wires: Vec<WireModel> =
                        Vec::with_capacity(br.entity_handles.len());
                    // The Dimension's own layer style — layer-0 inheritance
                    // target for baked sub-entities on layer "0" (#221).
                    let dim_l0_color = view::render::adapt_to_bg(
                        view::render::layer_render_style(document, &e.common().layer).color,
                        bg_color,
                    );
                    let dim_l0_aci = document
                        .layers
                        .get(&e.common().layer)
                        .map(|l| match &l.color {
                            acadrust::types::Color::Index(i) => *i,
                            _ => 0,
                        })
                        .unwrap_or(0);
                    for &eh in &br.entity_handles {
                        let Some(sub) = document.get_entity(eh) else { continue };
                        // Sub-entities inside *D### / DIMBLOCK## blocks
                        // typically use ByBlock color/linetype/lineweight —
                        // they should inherit from the Dimension entity.
                        let sub_color_is_byblock =
                            sub.common().color == acadrust::types::Color::ByBlock;
                        let sub_is_l0_bylayer = sub.common().layer == "0"
                            && sub.common().color == acadrust::types::Color::ByLayer;
                        let sub_wires = tessellate_entity(
                            document, selected, active_viewport, bg_color,
                            // Block contents are baked at the final WCS size —
                            // don't let downstream paths re-apply anno_scale.
                            1.0, sub, block_cache, view_aabb, world_per_pixel,
                        );
                        for mut w in sub_wires {
                            w.name = h.value().to_string();
                            // Override ByBlock colour with the dim's resolved
                            // colour so text matches `DIMCLRT`-style behaviour
                            // (or layer colour) instead of the raw ByBlock
                            // fallback that render_style_for produces. A layer-0
                            // sub inherits the dim's layer colour instead.
                            if sub_color_is_byblock {
                                w.color = if sel { WireModel::SELECTED } else { entity_color };
                                w.aci = aci;
                            } else if sub_is_l0_bylayer && !sel {
                                w.color = dim_l0_color;
                                w.aci = dim_l0_aci;
                            }
                            wires.push(w);
                        }
                    }
                    if !wires.is_empty() {
                        let aabb = entity_aabb(e);
                        for w in &mut wires {
                            // Empty SDF-text cells keep their tight glyph-box
                            // AABB; only stroke/fill wires take the whole-block
                            // box as a broad-phase pick hint.
                            if !w.points.is_empty() || !w.fill_tris.is_empty() {
                                w.aabb = aabb;
                            }
                        }
                        return wires;
                    }
                }
            }
        }
        // Fall through to the synthesis path below when no block is attached.
    }

    if let EntityType::Dimension(dim) = e {
        let aabb = entity_aabb(e);
        use crate::entities::dimension::DimensionTess;
        let mut wires = dim.tessellate(
            document,
            h,
            sel,
            entity_color,
            line_weight_px,
            anno_scale,
            selected,
            active_viewport,
            bg_color,
            view_aabb,
            world_per_pixel,
        );
        for w in &mut wires {
            w.aci = aci;
            // The whole-dimension box is a broad-phase hint for stroke/fill
            // wires (picked by proximity). An empty SDF-text wire instead
            // keeps its own tight glyph-box AABB so the text pick box hugs the
            // text — clicking empty space inside the dimension selects nothing,
            // only the lines or the text do.
            if !w.points.is_empty() || !w.fill_tris.is_empty() {
                w.aabb = aabb;
            }
        }
        return wires;
    }

    if let EntityType::MultiLeader(ml) = e {
        let aabb = entity_aabb(e);
        use crate::entities::multileader::MultiLeaderTess;
        let mut wires = ml.tessellate(
            document,
            h,
            sel,
            entity_color,
            line_weight_px,
            anno_scale,
            world_per_pixel,
            bg_color,
        );
        for w in &mut wires {
            w.aci = aci;
            // As with dimensions: keep the whole-leader box only on stroke/fill
            // wires; empty SDF-text wires keep their tight glyph-box AABB.
            if !w.points.is_empty() || !w.fill_tris.is_empty() {
                w.aabb = aabb;
            }
        }
        return wires;
    }

    // ── Table baked-block fast path ─────────────────────────────────────────
    //
    // AutoCAD bakes a Table's final rendered geometry (cell text, gridlines,
    // fill) into a per-instance block (usually `*T###`) referenced through
    // `table.block_record_handle`. The block's text uses the *displayed*
    // height; synthesising cells from `self.rows + TableStyle` instead would
    // re-apply the table's scale factor on top of already-baked geometry.
    // When the block exists we render it directly. Same pattern as
    // Dimension's `block_name`.
    if let EntityType::Table(tab) = e {
        if let Some(br_h) = tab.block_record_handle {
            if let Some(br) = document
                .block_records
                .iter()
                .find(|br| br.handle == br_h)
            {
                if !br.entity_handles.is_empty() {
                    let mut wires: Vec<WireModel> =
                        Vec::with_capacity(br.entity_handles.len());
                    // The Table's own layer style — layer-0 inheritance target
                    // for baked sub-entities on layer "0" (#221).
                    let tab_l0_color = view::render::adapt_to_bg(
                        view::render::layer_render_style(document, &e.common().layer).color,
                        bg_color,
                    );
                    let tab_l0_aci = document
                        .layers
                        .get(&e.common().layer)
                        .map(|l| match &l.color {
                            acadrust::types::Color::Index(i) => *i,
                            _ => 0,
                        })
                        .unwrap_or(0);
                    for &eh in &br.entity_handles {
                        let Some(sub) = document.get_entity(eh) else { continue };
                        let sub_color_is_byblock =
                            sub.common().color == acadrust::types::Color::ByBlock;
                        let sub_is_l0_bylayer = sub.common().layer == "0"
                            && sub.common().color == acadrust::types::Color::ByLayer;
                        let sub_wires = tessellate_entity(
                            document, selected, active_viewport, bg_color,
                            anno_scale, sub, block_cache, view_aabb, world_per_pixel,
                        );
                        for mut w in sub_wires {
                            w.name = h.value().to_string();
                            if sub_color_is_byblock {
                                w.color = if sel { WireModel::SELECTED } else { entity_color };
                                w.aci = aci;
                            } else if sub_is_l0_bylayer && !sel {
                                w.color = tab_l0_color;
                                w.aci = tab_l0_aci;
                            }
                            wires.push(w);
                        }
                    }
                    if !wires.is_empty() {
                        let aabb = entity_aabb(e);
                        for w in &mut wires {
                            // Empty SDF-text cells keep their tight glyph-box
                            // AABB; only stroke/fill wires take the whole-block
                            // box as a broad-phase pick hint.
                            if !w.points.is_empty() || !w.fill_tris.is_empty() {
                                w.aabb = aabb;
                            }
                        }
                        return wires;
                    }
                }
            }
        }
        // No baked block (e.g. a table created in-app) — synthesise coloured
        // geometry from the rows + TableStyle so fills/colours/borders/margins
        // are honoured instead of the monochrome fallback.
        // Annotative tables scale with the current annotation scale (their
        // stored geometry is at paper size); non-annotative tables are already
        // model-size, so pass 1.0.
        let table_anno = if crate::scene::annotative::is_annotative(document, e) {
            anno_scale
        } else {
            1.0
        };
        let mut wires = crate::entities::table::tessellate_table(
            tab, document, sel, entity_color, line_weight_px, table_anno,
        );
        if !wires.is_empty() {
            let aabb = entity_aabb(e);
            for w in &mut wires {
                w.aci = aci;
                // Empty SDF-text cells keep their tight glyph-box AABB; only
                // stroke/fill wires take the whole-table box as a broad-phase
                // pick hint (matches the dim / mleader / baked-block paths).
                if !w.points.is_empty() || !w.fill_tris.is_empty() {
                    w.aabb = aabb;
                }
            }
            return wires;
        }
    }

    if let EntityType::Insert(ins) = e {
        // Resolve the INSERT's own style so ByBlock sub-entities can inherit it.
        let (ins_color, ins_pat_len, ins_pat, ins_lw_px, _) = view::render::render_style_for(document, e);
        let ins_color = view::render::adapt_to_bg(ins_color, bg_color);
        // Resolve the INSERT's *layer* style — the layer-0 inheritance target
        // for sub-entities on layer "0" with ByLayer properties (#221).
        let ins_layer = {
            let mut s = view::render::layer_render_style(document, &ins.common.layer);
            s.color = view::render::adapt_to_bg(s.color, bg_color);
            s
        };
        let ip = glam::Vec3::new(
            (ins.insert_point.x) as f32,
            (ins.insert_point.y) as f32,
            (ins.insert_point.z) as f32,
        );
        let marker = WireModel {
            text_verts: Vec::new(),
            name: h.value().to_string(),
            points: vec![],
            points_low: Vec::new(),
            color: entity_color,
            selected: sel,
            aci: 0,
            pattern_length: 0.0,
            pattern: [0.0; 8],
            line_weight_px: 1.0,
            snap_pts: vec![(ip.as_dvec3(), model::wire_model::SnapHint::Insertion)],
            tangent_geoms: vec![],
            key_vertices: vec![],
            aabb: WireModel::UNBOUNDED_AABB,
            plinegen: true,
            vp_scissor: None,
            fill_tris: vec![],
            fill_tris_low: Vec::new(),
        };

        if let Some(cache) = block_cache {
            // Xrefs render with the same hue but faded toward `bg_color` so
            // the user can recognise external-reference geometry at a glance.
            let is_xref = document
                .block_records
                .get(&ins.block_name)
                .map(|br| br.flags.is_xref || br.flags.is_xref_overlay)
                .unwrap_or(false);
            if let Some(mut wires) = cache::block_cache::expand_insert(
                cache,
                ins,
                h,
                ins_color,
                ins_pat_len,
                ins_pat,
                ins_lw_px,
                ins_layer,
                sel,
                pslt_factor,
                view_aabb,
                world_per_pixel,
                is_xref,
                bg_color,
                anno_scale,
            ) {
                // XCLIP: if this INSERT carries an enabled spatial filter,
                // clip the expanded block geometry to the boundary polygon so
                // only the portion inside the clip is drawn.
                if let Some(sf) = pick::xclip::insert_spatial_filter(document, ins) {
                    let poly = pick::xclip::world_clip_polygon_f64(sf, ins);
                    pick::xclip::clip_wires(&mut wires, &poly);
                }

                // Per-INSERT attribute values. The block defn carries the
                // AttributeDefinitions (templates) which expand_insert skips;
                // the AttributeEntity instances live on the Insert itself in
                // WCS and need their own tessellation so the user sees the
                // values they actually filled in. See #20.
                crate::entities::insert::append_insert_attribute_wires(
                    &mut wires,
                    document,
                    ins,
                    h,
                    sel,
                    ins_color,
                    ins_pat_len,
                    ins_pat,
                    ins_lw_px,
                    ins_layer,
                    bg_color,
                    is_xref,
                    pslt_factor,
                    anno_scale,
                );
                wires.push(marker);
                return wires;
            }
        }

        // Cache miss / unavailable: fall back to the original explode path.
        // The block_cache primary path covers all typical Inserts; this
        // branch only fires for pathological cache failures.
        let br = document.block_records.get(&ins.block_name);
        let is_xref = br
            .map(|br| br.flags.is_xref || br.flags.is_xref_overlay)
            .unwrap_or(false);
        let mut wires: Vec<WireModel> = ins
            .explode_from_document(document)
            .iter()
            .cloned()
            .map(crate::modules::draw::modify::explode::normalize_insert_entity)
            .flat_map(|sub| {
                let (sub_color, sub_pattern_length, sub_pattern, sub_line_weight_px, sub_aci) =
                    view::render::render_style_for_block_sub(
                        document,
                        &sub,
                        ins_color,
                        ins_pat_len,
                        ins_pat,
                        ins_lw_px,
                        ins_layer,
                    );
                let sub_color = view::render::adapt_to_bg(sub_color, bg_color);
                let sub_color = if is_xref && !sel {
                    cache::block_cache::fade_toward_bg(sub_color, bg_color)
                } else {
                    sub_color
                };
                let sub_aabb = entity_aabb(&sub);
                let sub_pattern_length = sub_pattern_length * pslt_factor;
                let sub_pattern = sub_pattern.map(|v| v * pslt_factor);
                let mut wires = convert::tessellate::tessellate(
                    document,
                    h,
                    &sub,
                    sel,
                    sub_color,
                    sub_pattern_length,
                    sub_pattern,
                    sub_line_weight_px,
                    anno_scale,
                    world_per_pixel,
                    bg_color,
                    false,
                );
                for w in &mut wires {
                    w.name = h.value().to_string();
                    w.aci = sub_aci;
                    // Keep the glyph-bounds AABB tessellate set on SDF text wires
                    // (their geometry is in `text_verts`, not `points`); clobbering
                    // it here left block text with an UNBOUNDED box. For every
                    // other wire use the sub-entity box, falling back to the wire's
                    // own world points when that box is degenerate/unimplemented
                    // (UNBOUNDED) — otherwise it never culls and stalls snapping on
                    // block-heavy drawings.
                    if w.text_verts.is_empty() {
                        w.aabb = if sub_aabb == WireModel::UNBOUNDED_AABB {
                            wire_points_aabb(w)
                        } else {
                            sub_aabb
                        };
                    }
                }
                wires
            })
            .collect();
        crate::entities::insert::append_insert_attribute_wires(
            &mut wires,
            document,
            ins,
            h,
            sel,
            ins_color,
            ins_pat_len,
            ins_pat,
            ins_lw_px,
            ins_layer,
            bg_color,
            is_xref,
            pslt_factor,
            anno_scale,
        );
        wires.push(marker);
        return wires;
    }

    let aabb = entity_aabb(e);

    // TEXT / MTEXT / ATTDEF / ATTRIB / Tolerance all render as SDF glyph quads
    // (crisp at every zoom), so there is no text LOD ladder — they fall through
    // to the full tessellation path below.

    let mut bases = convert::tessellate::tessellate(
        document,
        h,
        e,
        sel,
        entity_color,
        pattern_length,
        pattern,
        line_weight_px,
        anno_scale,
        world_per_pixel,
        bg_color,
        false,
    );
    for b in &mut bases {
        b.aci = aci;
        // SDF text wires carry a glyph-bounds AABB (the true text extent) set
        // in the Text arm; entity_aabb would mis-place the pick box for MTEXT,
        // so don't clobber it. Every other wire takes the entity AABB.
        if b.text_verts.is_empty() {
            b.aabb = aabb;
        }
    }

    // Complex linetypes (with embedded shapes / text) expand the *base*
    // polyline along its tangent. Text-type entities never have a complex
    // linetype assigned, so we only consult the first wire here — multi-wire
    // returns come exclusively from MTEXT colour splits which can't trigger
    // this path.
    if let Some(clt) = crate::io::linetypes::resolve_complex_lt(document, lt_name) {
        if let Some(base) = bases.first() {
            let mut wires = text::complex_lt::apply_along(
                &base.name,
                &base.points,
                &clt,
                (lt_scale * pslt_factor).max(1e-4),
                entity_color,
                sel,
                base.line_weight_px,
            );
            if !wires.is_empty() {
                for w in &mut wires {
                    w.aabb = aabb;
                }
                return wires;
            }
        }
    }

    bases
}

/// Build the 4 OBB corners (CCW: bl, br, tr, tl) of a Text / MText entity
/// in its **native frame** — for top-level entities this is world coords,
/// for block-defn subs it's block-local. No offset/transform applied.
/// Width is approximated from glyph height × character count (TEXT) or
/// from `rectangle_width` (MTEXT). Returns `None` for non-text entities.
///
/// `mtext_lines_override` lets the caller plug in a wrap-aware line count
/// (from `text_support::mtext_line_count`). Without it, MText's OBB
/// height collapses to a single line when the file omits `rectangle_height`,
/// which makes downstream per-line LOD math degenerate.

/// Build a "low-LOD stub" wire for an entity that would otherwise be culled
/// to nothing — the entity's AABB diagonal as a 2-point segment, plus the
/// AABB itself so window / crossing selection picks the entity up. The
/// stored `selected` flag tracks across zoom levels so highlight visuals
/// don't disappear when the LOD level changes. See #19.
fn lod_stub_wire(
    name: String,
    color: [f32; 4],
    selected: bool,
    aci: u8,
    aabb: [f32; 4],
    z_min: f32,
    z_max: f32,
) -> WireModel {
    let [ax, ay, bx, by] = aabb;
    let cx = (ax + bx) * 0.5;
    let cy = (ay + by) * 0.5;
    let cz = (z_min + z_max) * 0.5;
    // Mirror what tessellate.rs does for the non-stub paths: bake the
    // selection-highlight colour into the wire so a re-tessellate triggered
    // by a zoom-induced LOD change keeps the entity highlighted. Without
    // this swap the wire's `selected` flag is true but its colour stays at
    // the entity's own hue, so the user sees the highlight vanish at the
    // LOD boundary. #19.
    let stored_color = if selected { WireModel::SELECTED } else { color };
    WireModel {
            text_verts: Vec::new(),
        name,
        // Diagonal of the entity's 3D AABB so depth tests against
        // shaded / hidden-line geometry are correct — the stub doesn't
        // flatten to z=0 and pop in front of objects that sit at a
        // different elevation. 2D entities (text fallbacks) pass
        // z_min = z_max = 0 to keep the historical behaviour.
        points: vec![[ax, ay, z_min], [bx, by, z_max]],
        points_low: Vec::new(),
        color: stored_color,
        selected,
        aci,
        pattern_length: 0.0,
        pattern: [0.0; 8],
        line_weight_px: 1.0,
        snap_pts: vec![],
        tangent_geoms: vec![],
        key_vertices: vec![[cx as f64, cy as f64, cz as f64]],
        aabb,
        plinegen: true,
        vp_scissor: None,
        fill_tris: vec![],
        fill_tris_low: Vec::new(),
    }
}

/// Sub-pixel LOD stub for 3D entities. Emits the entity's 3D AABB as a
/// 12-edge cube so the geometry occupies the same screen footprint and
/// depth range as the full tessellation, just with a tiny constant cost
/// (12 line segments). Without this, the diagonal stub used by
/// `lod_stub_wire` cuts off at two opposite bbox corners and drifts
/// visibly when the camera crosses the LOD threshold.
fn lod_stub_wire_3d(
    name: String,
    color: [f32; 4],
    selected: bool,
    aci: u8,
    aabb: [f32; 4],
    z_min: f32,
    z_max: f32,
) -> WireModel {
    let [x0, y0, x1, y1] = aabb;
    let (z0, z1) = if z_min <= z_max { (z_min, z_max) } else { (z_max, z_min) };
    let p = [
        [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
        [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1],
    ];
    // 12 edges = 4 bottom-face + 4 top-face + 4 vertical connectors.
    const EDGES: [(usize, usize); 12] = [
        (0, 1), (1, 2), (2, 3), (3, 0),
        (4, 5), (5, 6), (6, 7), (7, 4),
        (0, 4), (1, 5), (2, 6), (3, 7),
    ];
    let mut points: Vec<[f32; 3]> = Vec::with_capacity(EDGES.len() * 3);
    for (a, b) in EDGES {
        if !points.is_empty() {
            points.push([f32::NAN; 3]);
        }
        points.push(p[a]);
        points.push(p[b]);
    }
    let stored_color = if selected { WireModel::SELECTED } else { color };
    WireModel {
            text_verts: Vec::new(),
        name,
        points,
        points_low: Vec::new(),
        color: stored_color,
        selected,
        aci,
        pattern_length: 0.0,
        pattern: [0.0; 8],
        line_weight_px: 1.0,
        snap_pts: vec![],
        tangent_geoms: vec![],
        // No `key_vertices` — Face3DGpu requires 4 corners to emit a
        // fill quad, and we don't want this stub painted as a solid
        // face. The wire pass still draws its 12 edges.
        key_vertices: vec![],
        aabb,
        plinegen: true,
        vp_scissor: None,
        fill_tris: vec![],
        fill_tris_low: Vec::new(),
    }
}

/// Tessellate each visible AttributeEntity attached to an Insert and append
/// the resulting wires. AttributeEntity positions are already in WCS — the
/// INSERT only stamps the geometry once, attribute text sits at the world
/// position recorded on each ATTRIB. See #20.
#[allow(clippy::too_many_arguments)]
/// World-space XY AABB of a wire computed from its own points (double-single
/// `points + points_low` sum = absolute world, matching `wire_in_range`'s
/// `cursor_world`). Used as a fallback when an entity's `bounding_box()` is
/// degenerate/unimplemented (`entity_aabb` → `UNBOUNDED`), so the wire still
/// gets a real, cullable box instead of never being pre-rejected during snap.
pub(crate) fn wire_points_aabb(w: &WireModel) -> [f32; 4] {
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    let mut any = false;
    for (i, p) in w.points.iter().enumerate() {
        let lo = w.points_low.get(i).copied().unwrap_or([0.0; 3]);
        let (x, y) = (p[0] + lo[0], p[1] + lo[1]);
        if x.is_finite() && y.is_finite() {
            min[0] = min[0].min(x);
            min[1] = min[1].min(y);
            max[0] = max[0].max(x);
            max[1] = max[1].max(y);
            any = true;
        }
    }
    if any {
        [min[0], min[1], max[0], max[1]]
    } else {
        WireModel::UNBOUNDED_AABB
    }
}

pub(crate) fn entity_aabb(e: &acadrust::EntityType) -> [f32; 4] {
    let bbox = e.as_entity().bounding_box();
    let min_x = (bbox.min.x) as f32;
    let min_y = (bbox.min.y) as f32;
    let max_x = (bbox.max.x) as f32;
    let max_y = (bbox.max.y) as f32;
    // The all-zero box is bounding_box()'s Default — returned by entities with
    // no usable box (unimplemented) — so treat it as UNBOUNDED (never
    // pre-rejected). A genuinely zero-size box *away* from the origin (e.g. a
    // POINT) is a valid, cullable position: keep it, otherwise point-heavy
    // drawings fill the always-checked set and stall hit-testing.
    if min_x == 0.0 && min_y == 0.0 && max_x == 0.0 && max_y == 0.0 {
        return WireModel::UNBOUNDED_AABB;
    }
    [min_x, min_y, max_x, max_y]
}

/// AABB of `e` in WCS f64 (no world_offset subtraction). `None` for
/// entities whose `bounding_box()` returned the degenerate default
/// (which `entity_aabb` collapses to `UNBOUNDED_AABB`). Quadtree
/// indexing uses this so changing `world_offset` doesn't invalidate
/// the index.
pub(crate) fn entity_world_aabb_f64(e: &acadrust::EntityType) -> Option<[f64; 4]> {
    let bbox = e.as_entity().bounding_box();
    let (xmin, ymin, xmax, ymax) = (bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y);
    if xmin == xmax && ymin == ymax {
        return None;
    }
    if !xmin.is_finite() || !ymin.is_finite() || !xmax.is_finite() || !ymax.is_finite() {
        return None;
    }
    Some([xmin, ymin, xmax, ymax])
}

/// True if `e` is a type the quadtree should skip. `Insert` and
/// `Viewport` are sized only after extra transformation; tessellation
/// already handles them via dedicated code paths. `Block`/`BlockEnd`
/// are block-defn sentinels with no geometry.
pub(crate) fn is_unindexable_entity(e: &acadrust::EntityType) -> bool {
    use acadrust::EntityType as E;
    matches!(
        e,
        E::Insert(_) | E::Viewport(_) | E::Block(_) | E::BlockEnd(_)
    )
}
