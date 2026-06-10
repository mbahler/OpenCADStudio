//! Dimension Style Manager window — fills the entire OS window.

use crate::app::{DsField, Message};
use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_input, Space,
};
use iced::{Background, Border, Color, Element, Fill, Theme};

const BORDER: Color = Color {
    r: 0.35,
    g: 0.35,
    b: 0.35,
    a: 1.0,
};
const TEXT: Color = Color {
    r: 0.88,
    g: 0.88,
    b: 0.88,
    a: 1.0,
};
const DIM: Color = Color {
    r: 0.55,
    g: 0.55,
    b: 0.55,
    a: 1.0,
};
const ACCENT: Color = Color {
    r: 0.25,
    g: 0.50,
    b: 0.85,
    a: 1.0,
};
const ACTIVE: Color = Color {
    r: 0.20,
    g: 0.40,
    b: 0.70,
    a: 1.0,
};
const FIELD: Color = Color {
    r: 0.10,
    g: 0.10,
    b: 0.10,
    a: 1.0,
};

/// All DimStyle field values needed by the view.
pub struct DimStyleValues<'a> {
    pub dimdle: &'a str,
    pub dimdli: &'a str,
    pub dimgap: &'a str,
    pub dimexe: &'a str,
    pub dimexo: &'a str,
    pub dimsd1: bool,
    pub dimsd2: bool,
    pub dimse1: bool,
    pub dimse2: bool,
    pub dimasz: &'a str,
    pub dimcen: &'a str,
    pub dimtsz: &'a str,
    pub dimtxt: &'a str,
    pub dimtxsty: &'a str,
    pub dimtad: &'a str,
    pub dimtih: bool,
    pub dimtoh: bool,
    pub dimscale: &'a str,
    pub dimlfac: &'a str,
    pub dimlunit: &'a str,
    pub dimdec: &'a str,
    pub dimpost: &'a str,
    pub dimtol: bool,
    pub dimlim: bool,
    pub dimtp: &'a str,
    pub dimtm: &'a str,
    pub dimtdec: &'a str,
    pub dimtfac: &'a str,
    pub annotative: bool,
    pub dimclrd: &'a str,
    pub dimlwd: &'a str,
    pub dimclre: &'a str,
    pub dimlwe: &'a str,
    pub dimfxl: &'a str,
    pub dimfxlon: bool,
    pub dimsah: bool,
    pub dimarcsym: &'a str,
    pub dimjogang: &'a str,
    pub dimclrt: &'a str,
    pub dimjust: &'a str,
    pub dimtvp: &'a str,
    pub dimtfill: &'a str,
    pub dimtfillclr: &'a str,
    pub dimtxtdirection: bool,
    pub dimatfit: &'a str,
    pub dimtix: bool,
    pub dimsoxd: bool,
    pub dimtmove: &'a str,
    pub dimupt: bool,
    pub dimtofl: bool,
    pub dimfit: &'a str,
    pub dimdsep: &'a str,
    pub dimrnd: &'a str,
    pub dimzin: &'a str,
    pub dimfrac: &'a str,
    pub dimaunit: &'a str,
    pub dimadec: &'a str,
    pub dimunit: &'a str,
    pub dimazin: &'a str,
    pub dimalt: bool,
    pub dimaltf: &'a str,
    pub dimaltd: &'a str,
    pub dimaltu: &'a str,
    pub dimalttd: &'a str,
    pub dimaltrnd: &'a str,
    pub dimapost: &'a str,
    pub dimaltz: &'a str,
    pub dimalttz: &'a str,
    pub dimtolj: &'a str,
    pub dimtzin: &'a str,
    // Resolved selected names for the block/linetype Handle fields.
    pub dimblk_name: String,
    pub dimblk1_name: String,
    pub dimblk2_name: String,
    pub dimldrblk_name: String,
    pub dimltex_name: String,
    pub dimltex1_name: String,
    pub dimltex2_name: String,
    // Dropdown option lists shared by the arrowhead / linetype fields.
    pub block_opts: Vec<String>,
    pub lt_opts: Vec<String>,
}

fn tab_btn_style(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_: &Theme, st| button::Style {
        background: Some(Background::Color(match (active, st) {
            (true, _) => ACTIVE,
            (false, button::Status::Hovered | button::Status::Pressed) => Color {
                r: 0.28,
                g: 0.28,
                b: 0.28,
                a: 1.0,
            },
            _ => Color {
                r: 0.20,
                g: 0.20,
                b: 0.20,
                a: 1.0,
            },
        })),
        text_color: TEXT,
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}

fn field_style(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(FIELD),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 3.0.into(),
        },
        icon: TEXT,
        placeholder: DIM,
        value: TEXT,
        selection: ACCENT,
    }
}

fn hdivider<'a>() -> Element<'a, Message> {
    container(Space::new().width(Fill).height(1))
        .width(Fill)
        .height(1)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(BORDER)),
            ..Default::default()
        })
        .into()
}

pub fn view_window<'a>(
    styles: Vec<String>,
    selected: &'a str,
    current: &'a str,
    tab: u8,
    vals: DimStyleValues<'a>,
    rename_active: Option<&'a str>,
    rename_buf: &'a str,
) -> Element<'a, Message> {
    // ── Tab bar ───────────────────────────────────────────────────────────
    let tabs = row![
        button(text("Lines").size(11))
            .on_press(Message::DimStyleDialogTab(0))
            .style(tab_btn_style(tab == 0))
            .padding([4, 10]),
        button(text("Arrows").size(11))
            .on_press(Message::DimStyleDialogTab(1))
            .style(tab_btn_style(tab == 1))
            .padding([4, 10]),
        button(text("Text").size(11))
            .on_press(Message::DimStyleDialogTab(2))
            .style(tab_btn_style(tab == 2))
            .padding([4, 10]),
        button(text("Scale/Units").size(11))
            .on_press(Message::DimStyleDialogTab(3))
            .style(tab_btn_style(tab == 3))
            .padding([4, 10]),
        button(text("Tolerances").size(11))
            .on_press(Message::DimStyleDialogTab(4))
            .style(tab_btn_style(tab == 4))
            .padding([4, 10]),
        button(text("Alternate").size(11))
            .on_press(Message::DimStyleDialogTab(5))
            .style(tab_btn_style(tab == 5))
            .padding([4, 10]),
    ]
    .spacing(2);

    let lbl = |s: &'static str| text(s).size(11).color(DIM).width(180);

    let mk_field = |fld: DsField, val: &'a str| -> Element<'a, Message> {
        text_input("", val)
            .on_input(move |s| Message::DsEdit(fld.clone(), s))
            .style(field_style)
            .size(11)
            .width(100)
            .into()
    };

    let chk = |label: &'static str, val: bool, fld: DsField| -> Element<'a, Message> {
        checkbox(val)
            .label(label)
            .on_toggle(move |_| Message::DsToggle(fld.clone()))
            .size(14)
            .text_size(11)
            .into()
    };

    // Block / linetype Handle dropdown: pick a block-record (arrowheads) or a
    // linetype by name from the available records.
    let hrow = move |label: &'static str,
                     options: Vec<String>,
                     selected: String,
                     field: &'static str|
          -> Element<'a, Message> {
        row![
            lbl(label),
            pick_list(options, Some(selected), move |value| {
                Message::DsSetHandle { field, value }
            })
            .text_size(11)
            .width(150),
        ]
        .spacing(8)
        .align_y(iced::Center)
        .into()
    };

    let tab_content: Element<'_, Message> = match tab {
        0 => column![
            text("Dimension Line").size(11).color(ACCENT),
            row![
                lbl("Extension (DIMDLE)"),
                mk_field(DsField::Dimdle, vals.dimdle)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Spacing (DIMDLI)"),
                mk_field(DsField::Dimdli, vals.dimdli)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Text gap (DIMGAP)"),
                mk_field(DsField::Dimgap, vals.dimgap)
            ]
            .spacing(8)
            .align_y(iced::Center),
            chk("Suppress 1st line (DIMSD1)", vals.dimsd1, DsField::Dimsd1),
            chk("Suppress 2nd line (DIMSD2)", vals.dimsd2, DsField::Dimsd2),
            text("Extension Line").size(11).color(ACCENT),
            row![
                lbl("Extension (DIMEXE)"),
                mk_field(DsField::Dimexe, vals.dimexe)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Offset (DIMEXO)"),
                mk_field(DsField::Dimexo, vals.dimexo)
            ]
            .spacing(8)
            .align_y(iced::Center),
            chk("Suppress 1st line (DIMSE1)", vals.dimse1, DsField::Dimse1),
            chk("Suppress 2nd line (DIMSE2)", vals.dimse2, DsField::Dimse2),
            row![
                lbl("Dim line color ACI (DIMCLRD)"),
                mk_field(DsField::Dimclrd, vals.dimclrd)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Dim line weight (DIMLWD)"),
                mk_field(DsField::Dimlwd, vals.dimlwd)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Ext line color ACI (DIMCLRE)"),
                mk_field(DsField::Dimclre, vals.dimclre)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Ext line weight (DIMLWE)"),
                mk_field(DsField::Dimlwe, vals.dimlwe)
            ]
            .spacing(8)
            .align_y(iced::Center),
            chk(
                "Fixed-length ext lines (DIMFXLON)",
                vals.dimfxlon,
                DsField::Dimfxlon
            ),
            row![
                lbl("Fixed length (DIMFXL)"),
                mk_field(DsField::Dimfxl, vals.dimfxl)
            ]
            .spacing(8)
            .align_y(iced::Center),
            hrow(
                "Dim line linetype (DIMLTYPE)",
                vals.lt_opts.clone(),
                vals.dimltex_name.clone(),
                "dimltex_handle"
            ),
            hrow(
                "Ext line 1 linetype (DIMLTEX1)",
                vals.lt_opts.clone(),
                vals.dimltex1_name.clone(),
                "dimltex1_handle"
            ),
            hrow(
                "Ext line 2 linetype (DIMLTEX2)",
                vals.lt_opts.clone(),
                vals.dimltex2_name.clone(),
                "dimltex2_handle"
            ),
        ]
        .spacing(7)
        .into(),
        1 => column![
            text("Arrows").size(11).color(ACCENT),
            hrow(
                "Arrowhead (DIMBLK)",
                vals.block_opts.clone(),
                vals.dimblk_name.clone(),
                "dimblk"
            ),
            hrow(
                "1st arrowhead (DIMBLK1)",
                vals.block_opts.clone(),
                vals.dimblk1_name.clone(),
                "dimblk1"
            ),
            hrow(
                "2nd arrowhead (DIMBLK2)",
                vals.block_opts.clone(),
                vals.dimblk2_name.clone(),
                "dimblk2"
            ),
            hrow(
                "Leader arrowhead (DIMLDRBLK)",
                vals.block_opts.clone(),
                vals.dimldrblk_name.clone(),
                "dimldrblk"
            ),
            row![
                lbl("Arrow size (DIMASZ)"),
                mk_field(DsField::Dimasz, vals.dimasz)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Center mark (DIMCEN)"),
                mk_field(DsField::Dimcen, vals.dimcen)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Tick size (DIMTSZ)"),
                mk_field(DsField::Dimtsz, vals.dimtsz)
            ]
            .spacing(8)
            .align_y(iced::Center),
            chk(
                "Separate arrow blocks (DIMSAH)",
                vals.dimsah,
                DsField::Dimsah
            ),
            row![
                lbl("Arc length symbol (DIMARCSYM)"),
                mk_field(DsField::Dimarcsym, vals.dimarcsym)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Jog angle ° (DIMJOGANG)"),
                mk_field(DsField::Dimjogang, vals.dimjogang)
            ]
            .spacing(8)
            .align_y(iced::Center),
        ]
        .spacing(7)
        .into(),
        2 => column![
            text("Text").size(11).color(ACCENT),
            row![
                lbl("Height (DIMTXT)"),
                mk_field(DsField::Dimtxt, vals.dimtxt)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Style (DIMTXSTY)"),
                mk_field(DsField::Dimtxsty, vals.dimtxsty)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Vertical pos (DIMTAD)"),
                mk_field(DsField::Dimtad, vals.dimtad)
            ]
            .spacing(8)
            .align_y(iced::Center),
            chk("Horizontal inside (DIMTIH)", vals.dimtih, DsField::Dimtih),
            chk("Horizontal outside (DIMTOH)", vals.dimtoh, DsField::Dimtoh),
            row![
                lbl("Text color ACI (DIMCLRT)"),
                mk_field(DsField::Dimclrt, vals.dimclrt)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Horizontal just (DIMJUST)"),
                mk_field(DsField::Dimjust, vals.dimjust)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Vertical pos (DIMTVP)"),
                mk_field(DsField::Dimtvp, vals.dimtvp)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Text fill mode (DIMTFILL)"),
                mk_field(DsField::Dimtfill, vals.dimtfill)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Fill color ACI (DIMTFILLCLR)"),
                mk_field(DsField::Dimtfillclr, vals.dimtfillclr)
            ]
            .spacing(8)
            .align_y(iced::Center),
            chk(
                "Left-to-right (DIMTXTDIRECTION)",
                vals.dimtxtdirection,
                DsField::Dimtxtdirection
            ),
        ]
        .spacing(7)
        .into(),
        3 => column![
            text("Scale").size(11).color(ACCENT),
            chk("Annotative", vals.annotative, DsField::Annotative),
            row![
                lbl("Overall scale (DIMSCALE)"),
                mk_field(DsField::Dimscale, vals.dimscale)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Linear factor (DIMLFAC)"),
                mk_field(DsField::Dimlfac, vals.dimlfac)
            ]
            .spacing(8)
            .align_y(iced::Center),
            text("Units").size(11).color(ACCENT),
            row![
                lbl("Format (DIMLUNIT)"),
                mk_field(DsField::Dimlunit, vals.dimlunit)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Decimals (DIMDEC)"),
                mk_field(DsField::Dimdec, vals.dimdec)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Suffix (DIMPOST)"),
                mk_field(DsField::Dimpost, vals.dimpost)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Decimal sep ASCII (DIMDSEP)"),
                mk_field(DsField::Dimdsep, vals.dimdsep)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Round off (DIMRND)"),
                mk_field(DsField::Dimrnd, vals.dimrnd)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Zero suppress (DIMZIN)"),
                mk_field(DsField::Dimzin, vals.dimzin)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Fraction format (DIMFRAC)"),
                mk_field(DsField::Dimfrac, vals.dimfrac)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Angular unit (DIMAUNIT)"),
                mk_field(DsField::Dimaunit, vals.dimaunit)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Angular decimals (DIMADEC)"),
                mk_field(DsField::Dimadec, vals.dimadec)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Unit format (DIMUNIT)"),
                mk_field(DsField::Dimunit, vals.dimunit)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Angular zero supp (DIMAZIN)"),
                mk_field(DsField::Dimazin, vals.dimazin)
            ]
            .spacing(8)
            .align_y(iced::Center),
            text("Fit").size(11).color(ACCENT),
            row![
                lbl("Fit (DIMATFIT)"),
                mk_field(DsField::Dimatfit, vals.dimatfit)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Text movement (DIMTMOVE)"),
                mk_field(DsField::Dimtmove, vals.dimtmove)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Fit (legacy DIMFIT)"),
                mk_field(DsField::Dimfit, vals.dimfit)
            ]
            .spacing(8)
            .align_y(iced::Center),
            chk("Force text inside (DIMTIX)", vals.dimtix, DsField::Dimtix),
            chk(
                "Suppress outside arrows (DIMSOXD)",
                vals.dimsoxd,
                DsField::Dimsoxd
            ),
            chk("Place text manually (DIMUPT)", vals.dimupt, DsField::Dimupt),
            chk(
                "Dim line between ext (DIMTOFL)",
                vals.dimtofl,
                DsField::Dimtofl
            ),
        ]
        .spacing(7)
        .into(),
        5 => column![
            text("Alternate Units").size(11).color(ACCENT),
            chk(
                "Enable alternate units (DIMALT)",
                vals.dimalt,
                DsField::Dimalt
            ),
            row![
                lbl("Multiplier (DIMALTF)"),
                mk_field(DsField::Dimaltf, vals.dimaltf)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Decimals (DIMALTD)"),
                mk_field(DsField::Dimaltd, vals.dimaltd)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Unit format (DIMALTU)"),
                mk_field(DsField::Dimaltu, vals.dimaltu)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Tol decimals (DIMALTTD)"),
                mk_field(DsField::Dimalttd, vals.dimalttd)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Round off (DIMALTRND)"),
                mk_field(DsField::Dimaltrnd, vals.dimaltrnd)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Suffix (DIMAPOST)"),
                mk_field(DsField::Dimapost, vals.dimapost)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Zero suppress (DIMALTZ)"),
                mk_field(DsField::Dimaltz, vals.dimaltz)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Tol zero supp (DIMALTTZ)"),
                mk_field(DsField::Dimalttz, vals.dimalttz)
            ]
            .spacing(8)
            .align_y(iced::Center),
        ]
        .spacing(7)
        .into(),
        _ => column![
            text("Tolerances").size(11).color(ACCENT),
            chk("Generate tolerances (DIMTOL)", vals.dimtol, DsField::Dimtol),
            chk("Limits generation (DIMLIM)", vals.dimlim, DsField::Dimlim),
            row![
                lbl("Plus tolerance (DIMTP)"),
                mk_field(DsField::Dimtp, vals.dimtp)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Minus tolerance (DIMTM)"),
                mk_field(DsField::Dimtm, vals.dimtm)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Tol. decimals (DIMTDEC)"),
                mk_field(DsField::Dimtdec, vals.dimtdec)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Tol. scale (DIMTFAC)"),
                mk_field(DsField::Dimtfac, vals.dimtfac)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Tol. vert just (DIMTOLJ)"),
                mk_field(DsField::Dimtolj, vals.dimtolj)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                lbl("Tol. zero supp (DIMTZIN)"),
                mk_field(DsField::Dimtzin, vals.dimtzin)
            ]
            .spacing(8)
            .align_y(iced::Center),
        ]
        .spacing(7)
        .into(),
    };

    // ── Right panel: tabs + scrollable content ────────────────────────────
    // The scrollable itself fills the full width so its scrollbar sits flush
    // against the window's right edge; horizontal insets live on the inner
    // content container instead of the panel padding.
    let right_panel = container(
        column![
            text(format!("Editing: {selected}")).size(11).color(DIM),
            tabs,
            hdivider(),
            scrollable(container(tab_content).padding([12, 12]).width(Fill))
                .width(Fill)
                .height(Fill),
        ]
        .spacing(6)
        .height(Fill),
    )
    .height(Fill)
    .width(Fill)
    .padding(iced::Padding {
        top: 12.0,
        right: 0.0,
        bottom: 12.0,
        left: 0.0,
    });

    crate::ui::style_manager::view(crate::ui::style_manager::Scaffold {
        kind: crate::app::StyleKind::Dim,
        styles: &styles,
        selected,
        current: Some(current),
        rename_active,
        rename_buf,
        on_new: Message::DimStyleDialogNew,
        on_copy: Message::DimStyleDialogCopy,
        on_delete: Message::DimStyleDialogDelete,
        on_select: Message::DimStyleDialogSelect,
        on_set_current: Message::DimStyleDialogSetCurrent,
        on_apply: Message::DimStyleDialogApply,
        editor: right_panel.into(),
    })
}
