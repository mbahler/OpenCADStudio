use super::super::{Message, OpenCADStudio};
use iced::widget::{
    button, column, container, mouse_area, pick_list, row, text, text_input,
    Space,
};
use iced::{Background, Border, Color, Element, Fill, Theme};

impl OpenCADStudio {
    /// Build the currently-open modal dialog's content (Plan B), or `None`.
    /// Each former pop-up window is constructed here and given a bounded size
    /// (About shrinks to its content). Rendered as an overlay by `view_main`.
    pub(super) fn modal_content(&self) -> Option<Element<'_, Message>> {
        fn sized<'a>(e: Element<'a, Message>, w: u16, h: u16) -> Element<'a, Message> {
            iced::widget::container(e)
                .width(iced::Length::Fixed(w as f32))
                .height(iced::Length::Fixed(h as f32))
                .into()
        }
        Some(match self.active_modal? {
            super::super::ModalKind::About => crate::ui::about::view_window(),
            super::super::ModalKind::Shortcuts => {
                sized(crate::ui::shortcuts::view_window(&self.shortcut_overrides), 720, 520)
            }
            super::super::ModalKind::PluginManager => sized(
                crate::ui::plugin_manager::view_window(
                    &self.disabled_plugins,
                    &self.external_plugins,
                    &self.loaded_plugin_ids,
                    crate::ui::plugin_manager::MarketView {
                        registry: &self.plugin_registry,
                        input: &self.plugin_repo_input,
                        repos: &self.plugin_repos,
                        release_tags: &self.repo_release_tags,
                        selected_tag: &self.repo_selected_tag,
                        status: &self.marketplace_status,
                    },
                ),
                520,
                460,
            ),
            super::super::ModalKind::UpdateNotice => {
                let latest = self.update_notice_version.as_deref().unwrap_or("?");
                let body = self.update_notice_body.as_deref().unwrap_or("");
                sized(crate::ui::update_notice::view_window(latest, body), 560, 460)
            }
            super::super::ModalKind::Layers => {
                let tab = &self.tabs[self.active_tab];
                sized(tab.layers.view_window(), 900, 360)
            }
            super::super::ModalKind::PageSetup => sized(
                crate::ui::page_setup::view_window(
                    &self.page_setup_w,
                    &self.page_setup_h,
                    &self.page_setup_plot_area,
                    self.page_setup_center,
                    &self.page_setup_offset_x,
                    &self.page_setup_offset_y,
                    &self.page_setup_rotation,
                    &self.page_setup_scale,
                ),
                520,
                460,
            ),
            super::super::ModalKind::LayoutManager => {
                let i = self.active_tab;
                let layouts = self.tabs[i].scene.layout_names();
                let current = self.tabs[i].scene.current_layout.clone();
                sized(
                    crate::ui::layout_manager::view_window(
                        layouts,
                        &self.layout_manager_selected,
                        &self.layout_manager_rename_buf,
                        current,
                    ),
                    640,
                    320,
                )
            }
            super::super::ModalKind::Plotstyle => sized(
                crate::ui::style::plotstyle::view_window(
                    self.active_plot_style.as_ref(),
                    self.plotstyle_panel_aci,
                    &self.ps_color_buf,
                    &self.ps_lineweight_buf,
                    &self.ps_screening_buf,
                ),
                780,
                540,
            ),
            super::super::ModalKind::TextStyle => {
                let tab = &self.tabs[self.active_tab];
                let styles: Vec<String> = tab
                    .scene
                    .document
                    .text_styles
                    .iter()
                    .map(|s| s.name.clone())
                    .collect();
                let (backward, upside_down, annotative) = tab
                    .scene
                    .document
                    .text_styles
                    .get(&self.textstyle_selected)
                    .map(|s| (s.flags.backward, s.flags.upside_down, s.annotative))
                    .unwrap_or((false, false, false));
                sized(
                    crate::ui::style::textstyle::view_window(crate::ui::style::textstyle::TextStyleView {
                        styles,
                        selected: &self.textstyle_selected,
                        current: &tab.scene.document.header.current_text_style_name,
                        font_buf: &self.textstyle_font,
                        width_buf: &self.textstyle_width,
                        oblique_buf: &self.textstyle_oblique,
                        height_buf: &self.textstyle_height,
                        bigfont_buf: &self.textstyle_bigfont,
                        ttf_buf: &self.textstyle_ttf,
                        backward,
                        upside_down,
                        annotative,
                        rename_active: self.style_rename.as_deref(),
                        rename_buf: &self.style_rename_buf,
                    }),
                    // Wider than the old 620 window: the TTF system-font panel
                    // (Plan B / web fonts) added a column.
                    860,
                    480,
                )
            }
            super::super::ModalKind::MlStyle => {
                use acadrust::objects::ObjectType;
                let tab = &self.tabs[self.active_tab];
                let styles: Vec<String> = tab
                    .scene
                    .document
                    .objects
                    .values()
                    .filter_map(|o| match o {
                        ObjectType::MLineStyle(s) => Some(s.name.clone()),
                        _ => None,
                    })
                    .collect();
                let selected_style = tab.scene.document.objects.values().find_map(|o| match o {
                    ObjectType::MLineStyle(s) if s.name == self.mlstyle_selected => Some(s),
                    _ => None,
                });
                sized(
                    crate::ui::style::mlstyle::view_window(
                        styles,
                        &self.mlstyle_selected,
                        selected_style,
                        tab.scene.document.header.multiline_style.clone(),
                        self.style_rename.as_deref(),
                        &self.style_rename_buf,
                    ),
                    620,
                    420,
                )
            }
            super::super::ModalKind::TableStyle => {
                use acadrust::objects::ObjectType;
                let tab = &self.tabs[self.active_tab];
                let styles: Vec<String> = tab
                    .scene
                    .document
                    .objects
                    .values()
                    .filter_map(|o| match o {
                        ObjectType::TableStyle(s) => Some(s.name.clone()),
                        _ => None,
                    })
                    .collect();
                let selected_style = tab.scene.document.objects.values().find_map(|o| match o {
                    ObjectType::TableStyle(s) if s.name == self.tablestyle_selected => Some(s),
                    _ => None,
                });
                sized(
                    crate::ui::style::tablestyle::view_window(
                        styles,
                        &self.tablestyle_selected,
                        &self.ribbon.active_table_style,
                        selected_style,
                        &self.ts_hmargin,
                        &self.ts_vmargin,
                        &self.ts_description,
                        &self.ts_cell_textstyle,
                        &self.ts_cell_height,
                        &self.ts_cell_textcolor,
                        &self.ts_cell_fillcolor,
                        &self.ts_cell_datatype,
                        &self.ts_cell_unittype,
                        &self.ts_cell_format,
                        &self.ts_border_lw,
                        &self.ts_border_color,
                        &self.ts_border_spacing,
                        self.style_rename.as_deref(),
                        &self.style_rename_buf,
                        self.ts_color_open,
                    ),
                    620,
                    420,
                )
            }
            super::super::ModalKind::MLeaderStyle => {
                use acadrust::objects::ObjectType;
                let tab = &self.tabs[self.active_tab];
                let styles: Vec<String> = tab
                    .scene
                    .document
                    .objects
                    .values()
                    .filter_map(|o| match o {
                        ObjectType::MultiLeaderStyle(s) => Some(s.name.clone()),
                        _ => None,
                    })
                    .collect();
                let selected_style = tab.scene.document.objects.values().find_map(|o| match o {
                    ObjectType::MultiLeaderStyle(s) if s.name == self.mleaderstyle_selected => {
                        Some(s)
                    }
                    _ => None,
                });
                let doc = &tab.scene.document;
                let mut block_opts: Vec<String> = vec!["None".to_string()];
                block_opts.extend(doc.block_records.iter().map(|b| b.name.clone()));
                let mut lt_opts: Vec<String> = vec!["None".to_string()];
                lt_opts.extend(doc.line_types.iter().map(|lt| lt.name.clone()));
                let mut textstyle_opts: Vec<String> = vec!["None".to_string()];
                textstyle_opts.extend(doc.text_styles.iter().map(|t| t.name.clone()));
                let opt_block = |h: Option<acadrust::types::Handle>| -> String {
                    match h {
                        Some(h) => doc
                            .block_records
                            .iter()
                            .find(|b| b.handle == h)
                            .map(|b| b.name.clone())
                            .unwrap_or_else(|| "None".to_string()),
                        None => "None".to_string(),
                    }
                };
                let opt_lt = |h: Option<acadrust::types::Handle>| -> String {
                    match h {
                        Some(h) => doc
                            .line_types
                            .iter()
                            .find(|lt| lt.handle == h)
                            .map(|lt| lt.name.clone())
                            .unwrap_or_else(|| "None".to_string()),
                        None => "None".to_string(),
                    }
                };
                let opt_ts = |h: Option<acadrust::types::Handle>| -> String {
                    match h {
                        Some(h) => doc
                            .text_styles
                            .iter()
                            .find(|t| t.handle == h)
                            .map(|t| t.name.clone())
                            .unwrap_or_else(|| "None".to_string()),
                        None => "None".to_string(),
                    }
                };
                let (line_type_name, arrowhead_name, text_style_name, block_content_name) =
                    match selected_style {
                        Some(s) => (
                            opt_lt(s.line_type_handle),
                            opt_block(s.arrowhead_handle),
                            opt_ts(s.text_style_handle),
                            opt_block(s.block_content_handle),
                        ),
                        None => Default::default(),
                    };
                sized(
                    crate::ui::style::mleaderstyle::view_window(crate::ui::style::mleaderstyle::MLeaderStyleView {
                        styles,
                        selected: &self.mleaderstyle_selected,
                        style: selected_style,
                        current: tab.active_mleader_style.clone(),
                        landing_distance: &self.mls_landing_distance,
                        landing_gap: &self.mls_landing_gap,
                        arrowhead_size: &self.mls_arrowhead_size,
                        text_height: &self.mls_text_height,
                        scale_factor: &self.mls_scale_factor,
                        break_gap: &self.mls_break_gap,
                        first_seg_angle: &self.mls_first_seg_angle,
                        second_seg_angle: &self.mls_second_seg_angle,
                        max_points: &self.mls_max_points,
                        default_text: &self.mls_default_text,
                        line_color: &self.mls_line_color,
                        text_color: &self.mls_text_color,
                        description: &self.mls_description,
                        line_weight: &self.mls_line_weight,
                        align_space: &self.mls_align_space,
                        block_color: &self.mls_block_color,
                        block_rotation: &self.mls_block_rotation,
                        block_scale_x: &self.mls_block_scale_x,
                        block_scale_y: &self.mls_block_scale_y,
                        block_scale_z: &self.mls_block_scale_z,
                        block_opts,
                        lt_opts,
                        textstyle_opts,
                        line_type_name,
                        arrowhead_name,
                        text_style_name,
                        block_content_name,
                        rename_active: self.style_rename.as_deref(),
                        rename_buf: &self.style_rename_buf,
                        color_open: self.mls_color_open,
                    }),
                    560,
                    560,
                )
            }
            super::super::ModalKind::DimStyle => {

            let tab = &self.tabs[self.active_tab];
            let styles: Vec<String> = tab
                .scene
                .document
                .dim_styles
                .iter()
                .map(|s| s.name.clone())
                .collect();
            let doc = &tab.scene.document;
            // Dropdown options (names must match the records exactly so the
            // selection can be resolved back to a handle on the update side).
            let mut block_opts: Vec<String> = vec!["Default".to_string()];
            block_opts.extend(doc.block_records.iter().map(|b| b.name.clone()));
            let mut lt_opts: Vec<String> = vec!["ByBlock".to_string()];
            lt_opts.extend(doc.line_types.iter().map(|lt| lt.name.clone()));
            let blk_name = |h: acadrust::types::Handle| -> String {
                if h.is_null() {
                    "Default".to_string()
                } else {
                    doc.block_records
                        .iter()
                        .find(|b| b.handle == h)
                        .map(|b| b.name.clone())
                        .unwrap_or_else(|| "Default".to_string())
                }
            };
            let lt_name = |h: acadrust::types::Handle| -> String {
                if h.is_null() {
                    "ByBlock".to_string()
                } else {
                    doc.line_types
                        .iter()
                        .find(|lt| lt.handle == h)
                        .map(|lt| lt.name.clone())
                        .unwrap_or_else(|| "ByBlock".to_string())
                }
            };
            let ds_sel = doc.dim_styles.get(&self.dimstyle_selected);
            let (
                dimblk_name,
                dimblk1_name,
                dimblk2_name,
                dimldrblk_name,
                dimltex_name,
                dimltex1_name,
                dimltex2_name,
            ) = match ds_sel {
                Some(d) => (
                    blk_name(d.dimblk),
                    blk_name(d.dimblk1),
                    blk_name(d.dimblk2),
                    blk_name(d.dimldrblk),
                    lt_name(d.dimltex_handle),
                    lt_name(d.dimltex1_handle),
                    lt_name(d.dimltex2_handle),
                ),
                None => Default::default(),
            };
            sized(crate::ui::style::dimstyle::view_window(
                styles,
                &self.dimstyle_selected,
                &self.tabs[self.active_tab]
                    .scene
                    .document
                    .header
                    .current_dimstyle_name,
                self.dimstyle_tab,
                crate::ui::style::dimstyle::DimStyleValues {
                    dimdle: &self.ds_dimdle,
                    dimdli: &self.ds_dimdli,
                    dimgap: &self.ds_dimgap,
                    dimexe: &self.ds_dimexe,
                    dimexo: &self.ds_dimexo,
                    dimsd1: self.ds_dimsd1,
                    dimsd2: self.ds_dimsd2,
                    dimse1: self.ds_dimse1,
                    dimse2: self.ds_dimse2,
                    dimasz: &self.ds_dimasz,
                    dimcen: &self.ds_dimcen,
                    dimtsz: &self.ds_dimtsz,
                    dimtxt: &self.ds_dimtxt,
                    dimtxsty: &self.ds_dimtxsty,
                    dimtad: &self.ds_dimtad,
                    dimtih: self.ds_dimtih,
                    dimtoh: self.ds_dimtoh,
                    dimscale: &self.ds_dimscale,
                    dimlfac: &self.ds_dimlfac,
                    dimlunit: &self.ds_dimlunit,
                    dimdec: &self.ds_dimdec,
                    dimpost: &self.ds_dimpost,
                    dimtol: self.ds_dimtol,
                    dimlim: self.ds_dimlim,
                    dimtp: &self.ds_dimtp,
                    dimtm: &self.ds_dimtm,
                    dimtdec: &self.ds_dimtdec,
                    dimtfac: &self.ds_dimtfac,
                    annotative: self.ds_annotative,
                    dimclrd: &self.ds_dimclrd,
                    dimlwd: &self.ds_dimlwd,
                    dimclre: &self.ds_dimclre,
                    dimlwe: &self.ds_dimlwe,
                    dimfxl: &self.ds_dimfxl,
                    dimfxlon: self.ds_dimfxlon,
                    dimsah: self.ds_dimsah,
                    dimarcsym: &self.ds_dimarcsym,
                    dimjogang: &self.ds_dimjogang,
                    dimclrt: &self.ds_dimclrt,
                    dimjust: &self.ds_dimjust,
                    dimtvp: &self.ds_dimtvp,
                    dimtfill: &self.ds_dimtfill,
                    dimtfillclr: &self.ds_dimtfillclr,
                    dimtxtdirection: self.ds_dimtxtdirection,
                    dimatfit: &self.ds_dimatfit,
                    dimtix: self.ds_dimtix,
                    dimsoxd: self.ds_dimsoxd,
                    dimtmove: &self.ds_dimtmove,
                    dimupt: self.ds_dimupt,
                    dimtofl: self.ds_dimtofl,
                    dimfit: &self.ds_dimfit,
                    dimdsep: &self.ds_dimdsep,
                    dimrnd: &self.ds_dimrnd,
                    dimzin: &self.ds_dimzin,
                    dimfrac: &self.ds_dimfrac,
                    dimaunit: &self.ds_dimaunit,
                    dimadec: &self.ds_dimadec,
                    dimunit: &self.ds_dimunit,
                    dimazin: &self.ds_dimazin,
                    dimalt: self.ds_dimalt,
                    dimaltf: &self.ds_dimaltf,
                    dimaltd: &self.ds_dimaltd,
                    dimaltu: &self.ds_dimaltu,
                    dimalttd: &self.ds_dimalttd,
                    dimaltrnd: &self.ds_dimaltrnd,
                    dimapost: &self.ds_dimapost,
                    dimaltz: &self.ds_dimaltz,
                    dimalttz: &self.ds_dimalttz,
                    dimtolj: &self.ds_dimtolj,
                    dimtzin: &self.ds_dimtzin,
                    dimblk_name,
                    dimblk1_name,
                    dimblk2_name,
                    dimldrblk_name,
                    dimltex_name,
                    dimltex1_name,
                    dimltex2_name,
                    block_opts,
                    lt_opts,
                    color_open: self.ds_color_open.clone(),
                },
                self.style_rename.as_deref(),
                &self.style_rename_buf,
            ), 720, 560)
            }
            super::super::ModalKind::AssocPrompt => sized(default_assoc_dialog_window(), 440, 210),
            super::super::ModalKind::Unsaved => {
                let tab_name = match &self.pending_close {
                    Some(super::super::PendingClose::Tab(idx)) => self
                        .tabs
                        .get(*idx)
                        .map(|t| t.tab_display_name())
                        .unwrap_or_default(),
                    Some(super::super::PendingClose::Quit) => self
                        .tabs
                        .iter()
                        .find(|t| t.dirty)
                        .map(|t| t.tab_display_name())
                        .unwrap_or_default(),
                    None => String::new(),
                };
                sized(unsaved_changes_dialog_window(&tab_name), 420, 160)
            }
            super::super::ModalKind::PointStyle => sized(
                crate::ui::style::point_style::view_window(
                    self.tabs[self.active_tab].scene.document.header.point_display_mode,
                    self.point_size_relative,
                    &self.point_size_buf,
                ),
                360,
                470,
            ),
            super::super::ModalKind::SaveDialog => sized(
                save_as_dialog_window(
                    &self.save_dialog_filename,
                    &self.save_dialog_folder,
                    &self.save_dialog_entries,
                    &self.save_dialog_format,
                ),
                560,
                480,
            ),
        })
    }
}

const SAVE_FORMAT_OPTIONS: &[&str] = &[
    "DWG 2018", "DWG 2013", "DWG 2010", "DWG 2007", "DWG 2004", "DWG 2000", "DWG R14", "DXF 2018",
    "DXF 2013", "DXF 2010", "DXF 2007", "DXF 2004", "DXF 2000", "DXF R14",
];

fn save_as_dialog_window<'a>(
    filename: &'a str,
    folder: &'a std::path::Path,
    entries: &'a [(String, bool, std::path::PathBuf)],
    format: &'a str,
) -> Element<'a, Message> {
    const BG: Color = Color {
        r: 0.15,
        g: 0.15,
        b: 0.17,
        a: 1.0,
    };
    const LIST_BG: Color = Color {
        r: 0.11,
        g: 0.11,
        b: 0.13,
        a: 1.0,
    };
    const BORDER: Color = Color {
        r: 0.32,
        g: 0.32,
        b: 0.36,
        a: 1.0,
    };
    const TEXT: Color = Color {
        r: 0.90,
        g: 0.90,
        b: 0.90,
        a: 1.0,
    };
    const DIM: Color = Color {
        r: 0.58,
        g: 0.58,
        b: 0.62,
        a: 1.0,
    };
    const INPUT_BG: Color = Color {
        r: 0.10,
        g: 0.10,
        b: 0.12,
        a: 1.0,
    };
    const BTN_OK: Color = Color {
        r: 0.20,
        g: 0.46,
        b: 0.80,
        a: 1.0,
    };
    const BTN_HOV: Color = Color {
        r: 0.26,
        g: 0.55,
        b: 0.92,
        a: 1.0,
    };
    const BTN_GREY: Color = Color {
        r: 0.26,
        g: 0.26,
        b: 0.29,
        a: 1.0,
    };
    const BTN_GHOV: Color = Color {
        r: 0.34,
        g: 0.34,
        b: 0.38,
        a: 1.0,
    };
    const DIR_COL: Color = Color {
        r: 0.75,
        g: 0.85,
        b: 1.00,
        a: 1.0,
    };
    const FILE_COL: Color = TEXT;
    const ROW_HOV: Color = Color {
        r: 0.22,
        g: 0.24,
        b: 0.28,
        a: 1.0,
    };

    let input_sty =
        |_: &Theme, _: iced::widget::text_input::Status| iced::widget::text_input::Style {
            background: Background::Color(INPUT_BG),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            icon: TEXT,
            placeholder: DIM,
            value: TEXT,
            selection: Color {
                r: 0.20,
                g: 0.46,
                b: 0.80,
                a: 0.45,
            },
        };

    let btn = |lbl: &'static str, msg: Message, base: Color, hov: Color| {
        button(text(lbl).size(12).color(TEXT))
            .on_press(msg)
            .style(move |_: &Theme, st| button::Style {
                background: Some(Background::Color(
                    if matches!(st, button::Status::Hovered | button::Status::Pressed) {
                        hov
                    } else {
                        base
                    },
                )),
                text_color: TEXT,
                border: Border {
                    color: BORDER,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .padding([4, 12])
    };

    // ── Path bar ─────────────────────────────────────────────────────────
    let path_str = if crate::io::is_drives_root(folder) {
        crate::io::drives_root_label().to_string()
    } else {
        folder.to_string_lossy().into_owned()
    };
    let up_path = crate::io::parent_folder(folder);
    let path_bar = row![
        {
            let up_msg = up_path.map(Message::SaveDialogNavigate);
            let b = button(crate::ui::icons::tinted(crate::ui::icons::UP, 14.0, TEXT))
                .style(|_: &Theme, st| button::Style {
                    background: Some(Background::Color(
                        if matches!(st, button::Status::Hovered | button::Status::Pressed) {
                            BTN_GHOV
                        } else {
                            BTN_GREY
                        },
                    )),
                    text_color: TEXT,
                    border: Border {
                        color: BORDER,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
                .padding([3, 10]);
            if let Some(msg) = up_msg {
                b.on_press(msg)
            } else {
                b
            }
        },
        Space::new().width(8),
        container(text(path_str.clone()).size(12).color(DIM))
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(INPUT_BG)),
                border: Border {
                    color: BORDER,
                    width: 1.0,
                    radius: 4.0.into()
                },
                ..Default::default()
            })
            .padding([4, 8])
            .width(Fill),
    ]
    .align_y(iced::Alignment::Center);

    // ── File list ─────────────────────────────────────────────────────────
    let file_list: Element<'_, Message> = {
        let rows: Vec<Element<'_, Message>> = entries
            .iter()
            .map(|(name, is_dir, path)| {
                let icon_bytes = if *is_dir {
                    crate::ui::icons::FOLDER
                } else {
                    crate::ui::icons::DOC
                };
                let color = if *is_dir { DIR_COL } else { FILE_COL };
                let p = path.clone();
                let d = *is_dir;
                mouse_area(
                    container(
                        row![
                            crate::ui::icons::tinted(icon_bytes, 13.0, color),
                            Space::new().width(6),
                            text(crate::ui::text_util::elide(name.as_str(), 48))
                                .size(13)
                                .color(color),
                        ]
                        .align_y(iced::Alignment::Center),
                    )
                    .style(|_: &Theme| container::Style {
                        ..Default::default()
                    })
                    .padding([3, 8])
                    .width(Fill),
                )
                .on_press(Message::SaveDialogEntryClicked(p, d))
                .into()
            })
            .collect();

        container(iced::widget::scrollable(
            column(rows).spacing(1).width(Fill),
        ))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(LIST_BG)),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .width(Fill)
        .height(Fill)
        .into()
    };
    let _ = ROW_HOV; // used conceptually, suppress warning

    let sel_fmt = SAVE_FORMAT_OPTIONS.iter().copied().find(|&s| s == format);
    let label = |s: &'static str| text(s).size(11).color(DIM);

    // ── Bottom controls ───────────────────────────────────────────────────
    let bottom = column![
        row![
            label("File name:").width(90),
            text_input("drawing.dwg", filename)
                .on_input(Message::SaveDialogFilenameChanged)
                .style(input_sty)
                .size(13)
                .padding([5, 8])
                .width(Fill),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(6),
        Space::new().height(6),
        row![
            label("Format:").width(90),
            pick_list(SAVE_FORMAT_OPTIONS, sel_fmt, |s: &str| {
                Message::SaveDialogFormatChanged(s.to_string())
            })
            .width(Fill),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(6),
        Space::new().height(12),
        row![
            Space::new().width(Fill),
            btn("Save", Message::SaveDialogConfirm, BTN_OK, BTN_HOV),
            Space::new().width(8),
            btn("Cancel", Message::SaveDialogCancel, BTN_GREY, BTN_GHOV),
        ],
    ]
    .spacing(0);

    container(
        column![
            path_bar,
            Space::new().height(8),
            file_list,
            Space::new().height(10),
            bottom,
        ]
        .spacing(0),
    )
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(BG)),
        ..Default::default()
    })
    .padding([14, 16])
    .width(Fill)
    .height(Fill)
    .into()
}

fn unsaved_changes_dialog_window(name: &str) -> Element<'static, Message> {
    const BG: Color = Color {
        r: 0.18,
        g: 0.18,
        b: 0.20,
        a: 1.0,
    };
    const BORDER_COL: Color = Color {
        r: 0.38,
        g: 0.38,
        b: 0.42,
        a: 1.0,
    };
    const TEXT_COL: Color = Color {
        r: 0.90,
        g: 0.90,
        b: 0.90,
        a: 1.0,
    };
    const BTN_SAVE: Color = Color {
        r: 0.20,
        g: 0.46,
        b: 0.80,
        a: 1.0,
    };
    const BTN_HOVER: Color = Color {
        r: 0.26,
        g: 0.55,
        b: 0.92,
        a: 1.0,
    };
    const BTN_DISC: Color = Color {
        r: 0.28,
        g: 0.28,
        b: 0.30,
        a: 1.0,
    };
    const BTN_DHOV: Color = Color {
        r: 0.36,
        g: 0.36,
        b: 0.40,
        a: 1.0,
    };

    let body_text = format!("Do you want to save changes to \"{}\"?", name);

    let btn = |label: &'static str, msg: Message, base: Color, hov: Color| {
        button(text(label).size(13).color(TEXT_COL))
            .on_press(msg)
            .style(move |_: &Theme, status| button::Style {
                background: Some(Background::Color(match status {
                    button::Status::Hovered | button::Status::Pressed => hov,
                    _ => base,
                })),
                text_color: TEXT_COL,
                border: Border {
                    color: BORDER_COL,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            })
            .padding([6, 18])
    };

    container(
        column![
            text(body_text).size(13).color(TEXT_COL),
            iced::widget::Space::new().height(20),
            row![
                btn("Save", Message::UnsavedDialogSave, BTN_SAVE, BTN_HOVER),
                iced::widget::Space::new().width(8),
                btn("Discard", Message::UnsavedDialogDiscard, BTN_DISC, BTN_DHOV),
                iced::widget::Space::new().width(8),
                btn("Cancel", Message::UnsavedDialogCancel, BTN_DISC, BTN_DHOV),
            ],
        ]
        .spacing(0),
    )
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(BG)),
        ..Default::default()
    })
    .center(Fill)
    .padding([24, 28])
    .into()
}

/// First-launch prompt offering to register Open CAD Studio as the default
/// handler for .dwg / .dxf. "Yes" runs the platform association call; "Not now"
/// just dismisses. Either answer flips the persisted `default_assoc_prompted`
/// flag so the dialog never reappears.
fn default_assoc_dialog_window() -> Element<'static, Message> {
    const BG: Color = Color {
        r: 0.18,
        g: 0.18,
        b: 0.20,
        a: 1.0,
    };
    const BORDER_COL: Color = Color {
        r: 0.38,
        g: 0.38,
        b: 0.42,
        a: 1.0,
    };
    const TEXT_COL: Color = Color {
        r: 0.90,
        g: 0.90,
        b: 0.90,
        a: 1.0,
    };
    const DIM_COL: Color = Color {
        r: 0.62,
        g: 0.62,
        b: 0.66,
        a: 1.0,
    };
    const BTN_YES: Color = Color {
        r: 0.20,
        g: 0.46,
        b: 0.80,
        a: 1.0,
    };
    const BTN_YHOV: Color = Color {
        r: 0.26,
        g: 0.55,
        b: 0.92,
        a: 1.0,
    };
    const BTN_NO: Color = Color {
        r: 0.28,
        g: 0.28,
        b: 0.30,
        a: 1.0,
    };
    const BTN_NHOV: Color = Color {
        r: 0.36,
        g: 0.36,
        b: 0.40,
        a: 1.0,
    };

    let btn = |label: &'static str, msg: Message, base: Color, hov: Color| {
        button(text(label).size(13).color(TEXT_COL))
            .on_press(msg)
            .style(move |_: &Theme, status| button::Style {
                background: Some(Background::Color(match status {
                    button::Status::Hovered | button::Status::Pressed => hov,
                    _ => base,
                })),
                text_color: TEXT_COL,
                border: Border {
                    color: BORDER_COL,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            })
            .padding([6, 18])
    };

    container(
        column![
            text("Make Open CAD Studio your default CAD app?")
                .size(15)
                .color(TEXT_COL),
            iced::widget::Space::new().height(10),
            text("Open .dwg and .dxf drawings in Open CAD Studio by default. You can change this later in your system settings.")
                .size(12)
                .color(DIM_COL),
            iced::widget::Space::new().height(22),
            row![
                iced::widget::Space::new().width(Fill),
                btn("Not now", Message::AssocPromptNo, BTN_NO, BTN_NHOV),
                iced::widget::Space::new().width(8),
                btn("Yes, set as default", Message::AssocPromptYes, BTN_YES, BTN_YHOV),
            ]
            .align_y(iced::Center),
        ]
        .spacing(0),
    )
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(BG)),
        ..Default::default()
    })
    .center(Fill)
    .padding([24, 28])
    .into()
}

