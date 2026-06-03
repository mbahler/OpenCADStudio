//! Table Style Manager window — fills the entire OS window.

use crate::app::Message;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, Space};
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

fn list_item(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_: &Theme, st| button::Style {
        background: Some(Background::Color(match (active, st) {
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
    styles: Vec<String>,
    selected: &'a str,
    selected_style: Option<&'a acadrust::objects::TableStyle>,
) -> Element<'a, Message> {
    // ── Toolbar ───────────────────────────────────────────────────────────
    let toolbar = container(
        row![
            button(text("New").size(11))
                .on_press(Message::TableStyleDialogNew)
                .style(btn_s(true))
                .padding([4, 10]),
            button(text("Delete").size(11))
                .on_press(Message::TableStyleDialogDelete)
                .style(btn_s(false))
                .padding([4, 10]),
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

    // ── Left: Style list ──────────────────────────────────────────────────
    let style_items: Vec<Element<'_, Message>> = styles
        .iter()
        .map(|name| {
            let is_sel = name.as_str() == selected;
            button(text(name.clone()).size(11))
                .on_press(Message::TableStyleDialogSelect(name.clone()))
                .style(list_item(is_sel))
                .padding([4, 8])
                .width(Fill)
                .into()
        })
        .collect();

    let style_list = container(
        column![
            text("Styles").size(10).color(DIM),
            container(scrollable(column(style_items).spacing(2)).height(Fill))
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
    .width(200)
    .height(Fill)
    .padding(iced::Padding {
        top: 12.0,
        right: 8.0,
        bottom: 12.0,
        left: 12.0,
    });

    // ── Right: Details panel ──────────────────────────────────────────────
    let info_row = |label: &'static str, val: String| -> Element<'_, Message> {
        row![
            text(label).size(11).color(DIM).width(160),
            text(val).size(11),
        ]
        .spacing(8)
        .align_y(iced::Center)
        .into()
    };

    let row_info =
        |row_label: &'static str, rs: &acadrust::objects::RowCellStyle| -> Element<'_, Message> {
            column![
                text(row_label).size(11).color(ACCENT),
                info_row("  Text Style:", rs.text_style_name.clone()),
                info_row("  Text Height:", format!("{:.4}", rs.text_height)),
                info_row("  Alignment:", format!("{:?}", rs.alignment)),
            ]
            .spacing(3)
            .into()
        };

    let details: Element<'_, Message> = if let Some(s) = selected_style {
        scrollable(
            column![
                info_row("Name:", s.name.clone()),
                checkbox(s.annotative)
                    .label("Annotative")
                    .on_toggle(|_| Message::TableStyleToggleAnnotative)
                    .size(14)
                    .text_size(11),
                info_row("H Margin:", format!("{:.4}", s.horizontal_margin)),
                info_row("V Margin:", format!("{:.4}", s.vertical_margin)),
                info_row("Title Suppressed:", s.title_suppressed.to_string()),
                info_row("Header Suppressed:", s.header_suppressed.to_string()),
                row_info("Data Row:", &s.data_row_style),
                row_info("Header Row:", &s.header_row_style),
                row_info("Title Row:", &s.title_row_style),
            ]
            .spacing(6)
            .padding([12, 12]),
        )
        .height(Fill)
        .into()
    } else {
        container(text("Select a style to view details.").size(11).color(DIM))
            .padding([12, 12])
            .into()
    };

    let right_panel = container(details).width(Fill).height(Fill);

    let body = row![style_list, vsep(), right_panel].height(Fill);

    container(column![toolbar, hdivider(), body].spacing(0))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(BG)),
            ..Default::default()
        })
        .width(Fill)
        .height(Fill)
        .into()
}
