//! Plot Style Table Editor window — fills the entire OS window.

use crate::app::Message;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Background, Border, Color, Element, Fill, Theme};

const TB: Color = Color {
    r: 0.13,
    g: 0.13,
    b: 0.13,
    a: 1.0,
};
const BG: Color = Color {
    r: 0.15,
    g: 0.15,
    b: 0.15,
    a: 1.0,
};
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
const LIST: Color = Color {
    r: 0.12,
    g: 0.12,
    b: 0.12,
    a: 1.0,
};

fn btn_s(accent: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_: &Theme, st| button::Style {
        background: Some(Background::Color(match (accent, st) {
            (true, button::Status::Hovered | button::Status::Pressed) => Color {
                r: 0.20,
                g: 0.42,
                b: 0.72,
                a: 1.0,
            },
            (false, button::Status::Hovered | button::Status::Pressed) => Color {
                r: 0.28,
                g: 0.28,
                b: 0.28,
                a: 1.0,
            },
            (true, _) => ACCENT,
            _ => Color {
                r: 0.22,
                g: 0.22,
                b: 0.22,
                a: 1.0,
            },
        })),
        text_color: TEXT,
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 4.0.into(),
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

fn vsep<'a>() -> Element<'a, Message> {
    container(Space::new().width(1).height(Fill))
        .width(1)
        .height(Fill)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(BORDER)),
            ..Default::default()
        })
        .into()
}

pub fn view_window<'a>(
    table: Option<&'a crate::io::plot_style::PlotStyleTable>,
    selected_aci: u8,
    color_buf: &'a str,
    lw_buf: &'a str,
    screen_buf: &'a str,
) -> Element<'a, Message> {
    let table_name = table
        .map(|t| t.name.as_str())
        .unwrap_or("(no table loaded)");

    // ── Toolbar ───────────────────────────────────────────────────────────
    let toolbar = container(
        row![
            button(text("Load CTB/STB").size(11))
                .on_press(Message::PlotStyleLoad)
                .style(btn_s(false))
                .padding([4, 10]),
            button(text("Save As…").size(11))
                .on_press(Message::PlotStylePanelSave)
                .style(btn_s(false))
                .padding([4, 10]),
            button(text("Clear Table").size(11))
                .on_press(Message::PlotStyleClear)
                .style(btn_s(false))
                .padding([4, 10]),
            Space::new().width(Fill),
            text(table_name).size(10).color(DIM),
        ]
        .spacing(4)
        .align_y(iced::Center),
    )
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(TB)),
        ..Default::default()
    })
    .width(Fill)
    .padding([5, 8]);

    // ── Left: ACI list ────────────────────────────────────────────────────
    let aci_items: Vec<Element<'_, Message>> = (1u8..=255)
        .map(|aci| {
            let is_sel = aci == selected_aci;
            let has_override = table
                .and_then(|t| t.aci_entries.get(aci as usize))
                .map(|e| e.color.is_some() || e.lineweight != 255 || e.screening != 100)
                .unwrap_or(false);
            let lw_str = table
                .and_then(|t| t.aci_entries.get(aci as usize))
                .and_then(|e| {
                    if e.lineweight != 255 {
                        crate::io::plot_style::LW_TABLE
                            .get(e.lineweight as usize)
                            .map(|lw| format!("{lw:.2}mm"))
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            let color_str = table
                .and_then(|t| t.aci_entries.get(aci as usize))
                .and_then(|e| e.color.map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}")))
                .unwrap_or_default();
            let label = if has_override {
                format!("{aci:>3}  {color_str:<9} {lw_str}")
            } else {
                format!("{aci:>3}  (default)")
            };
            button(text(label).size(10).font(iced::Font::MONOSPACE))
                .on_press(Message::PlotStylePanelSelectAci(aci))
                .style(move |_: &Theme, st| button::Style {
                    background: Some(Background::Color(match (is_sel, st) {
                        (true, _) => ACTIVE,
                        (false, button::Status::Hovered | button::Status::Pressed) => Color {
                            r: 0.26,
                            g: 0.26,
                            b: 0.26,
                            a: 1.0,
                        },
                        _ => Color::TRANSPARENT,
                    })),
                    text_color: TEXT,
                    ..Default::default()
                })
                .padding([2, 8])
                .width(Fill)
                .into()
        })
        .collect();

    let aci_list = container(
        column![
            text("ACI Color Index").size(10).color(DIM),
            container(scrollable(column(aci_items).spacing(1)).height(Fill))
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(LIST)),
                    border: Border {
                        color: BORDER,
                        width: 1.0,
                        radius: 3.0.into()
                    },
                    ..Default::default()
                })
                .width(Fill)
                .height(Fill)
                .padding(2),
        ]
        .spacing(4)
        .height(Fill),
    )
    .width(280)
    .height(Fill)
    .padding(iced::Padding {
        top: 12.0,
        right: 8.0,
        bottom: 12.0,
        left: 12.0,
    });

    // ── Right: Edit panel ─────────────────────────────────────────────────
    let entry = table.and_then(|t| t.aci_entries.get(selected_aci as usize));
    let cur_color = entry
        .and_then(|e| e.color.map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}")))
        .unwrap_or_else(|| "(none)".into());
    let cur_lw = entry
        .map(|e| {
            if e.lineweight == 255 {
                "object".into()
            } else {
                crate::io::plot_style::LW_TABLE
                    .get(e.lineweight as usize)
                    .map(|lw| format!("{lw:.2}mm (idx {})", e.lineweight))
                    .unwrap_or_else(|| format!("idx {}", e.lineweight))
            }
        })
        .unwrap_or_else(|| "—".into());
    let cur_scr = entry
        .map(|e| format!("{}%", e.screening))
        .unwrap_or_else(|| "—".into());

    let lbl = |s: &'static str| text(s).size(11).color(DIM);

    let edit_panel = container(
        column![
            row![
                text("ACI:").size(11).color(DIM).width(100),
                text(format!("{selected_aci}")).size(11),
            ]
            .spacing(8)
            .align_y(iced::Center),
            lbl("Color override (#RRGGBB):"),
            text_input("#RRGGBB or blank", color_buf)
                .on_input(Message::PlotStylePanelColorBuf)
                .style(field_style)
                .size(11)
                .padding([4, 8]),
            lbl("Lineweight index (0-24, 255=obj):"),
            text_input("255", lw_buf)
                .on_input(Message::PlotStylePanelLwBuf)
                .style(field_style)
                .size(11)
                .padding([4, 8]),
            lbl("Screening (0-100):"),
            text_input("100", screen_buf)
                .on_input(Message::PlotStylePanelScreenBuf)
                .style(field_style)
                .size(11)
                .padding([4, 8]),
            Space::new().height(8),
            text("Current values:").size(10).color(DIM),
            text(format!("  Color: {cur_color}")).size(10),
            text(format!("  Lineweight: {cur_lw}")).size(10),
            text(format!("  Screening: {cur_scr}")).size(10),
            Space::new().height(Fill),
            button(text("Apply to ACI").size(11))
                .on_press(Message::PlotStylePanelApply)
                .style(btn_s(true))
                .padding([5, 10]),
        ]
        .spacing(8)
        .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .padding([12, 12]);

    let body = row![aci_list, vsep(), edit_panel].height(Fill);

    container(column![toolbar, hdivider(), body].spacing(0))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(BG)),
            ..Default::default()
        })
        .width(Fill)
        .height(Fill)
        .into()
}
