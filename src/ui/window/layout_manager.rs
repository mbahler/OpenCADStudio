//! Layout Manager window — fills the entire OS window.

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
const WARN: Color = Color {
    r: 0.80,
    g: 0.35,
    b: 0.25,
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
    layouts: Vec<String>,
    selected: &'a str,
    rename_buf: &'a str,
    current: String,
) -> Element<'a, Message> {
    let is_model = selected == "Model";

    // ── Toolbar ───────────────────────────────────────────────────────────
    let toolbar = container(
        row![
            button(text("New Layout").size(11))
                .on_press(Message::LayoutManagerNew)
                .style(btn_s(false))
                .padding([4, 10]),
            button(text("Delete").size(11))
                .on_press(Message::LayoutManagerDelete)
                .style(move |_: &Theme, st| button::Style {
                    background: Some(Background::Color(match st {
                        button::Status::Hovered | button::Status::Pressed => Color {
                            r: 0.60,
                            g: 0.20,
                            b: 0.18,
                            a: 1.0
                        },
                        _ => Color {
                            r: 0.22,
                            g: 0.22,
                            b: 0.22,
                            a: 1.0
                        },
                    })),
                    text_color: if is_model { DIM } else { WARN },
                    border: Border {
                        color: BORDER,
                        width: 1.0,
                        radius: 4.0.into()
                    },
                    ..Default::default()
                })
                .padding([4, 10]),
            Space::new().width(Fill),
            button(
                row![
                    crate::ui::icons::tinted(crate::ui::icons::TRI_LEFT_B, 9.0, Color::WHITE),
                    text("Move Left").size(11),
                ]
                .spacing(4)
                .align_y(iced::Center),
            )
            .on_press(Message::LayoutManagerMoveLeft)
            .style(btn_s(false))
            .padding([4, 8]),
            button(
                row![
                    text("Move Right").size(11),
                    crate::ui::icons::arrow_right(9.0, Color::WHITE),
                ]
                .spacing(4)
                .align_y(iced::Center),
            )
            .on_press(Message::LayoutManagerMoveRight)
            .style(btn_s(false))
            .padding([4, 8]),
            button(text("Set Current").size(11))
                .on_press(Message::LayoutManagerSetCurrent)
                .style(btn_s(true))
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

    // ── Left: Layout list ─────────────────────────────────────────────────
    let list_items: Vec<Element<'_, Message>> = layouts
        .iter()
        .map(|name| {
            let is_sel = name.as_str() == selected;
            let is_cur = name.as_str() == current.as_str();
            let mut item_row = row![text(name.clone()).size(12)]
                .spacing(5)
                .align_y(iced::Center);
            if is_cur {
                item_row = item_row.push(crate::ui::icons::tinted(
                    crate::ui::icons::TRI_LEFT_B,
                    8.0,
                    Color::WHITE,
                ));
            }
            button(item_row)
                .on_press(Message::LayoutManagerSelect(name.clone()))
                .style(list_item(is_sel))
                .padding([5, 10])
                .width(Fill)
                .into()
        })
        .collect();

    let layout_list = container(
        column![
            text("Layouts").size(10).color(DIM),
            container(scrollable(column(list_items).spacing(2)).height(Fill))
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
    .width(220)
    .height(Fill)
    .padding(iced::Padding {
        top: 12.0,
        right: 8.0,
        bottom: 12.0,
        left: 12.0,
    });

    // ── Right: Details + rename ───────────────────────────────────────────
    let details = container(
        column![
            text(if is_model {
                "Model Space"
            } else {
                "Paper Space Layout"
            })
            .size(13)
            .color(TEXT),
            Space::new().height(8),
            row![
                text("Name:").size(11).color(DIM).width(80),
                text(selected).size(11),
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                text("Status:").size(11).color(DIM).width(80),
                text(if selected == current.as_str() {
                    "Active"
                } else {
                    "Inactive"
                })
                .size(11)
                .color(if selected == current.as_str() {
                    ACCENT
                } else {
                    DIM
                }),
            ]
            .spacing(8)
            .align_y(iced::Center),
            Space::new().height(16),
            text("Rename").size(10).color(DIM),
            row![
                text_input("New name…", rename_buf)
                    .on_input(Message::LayoutManagerRenameBuf)
                    .on_submit(Message::LayoutManagerRenameCommit)
                    .style(field_style)
                    .size(11)
                    .padding([4, 8]),
                button(text("OK").size(11))
                    .on_press(Message::LayoutManagerRenameCommit)
                    .style(btn_s(true))
                    .padding([4, 10]),
            ]
            .spacing(6)
            .align_y(iced::Center),
        ]
        .spacing(8),
    )
    .width(Fill)
    .padding([12, 12]);

    let body = row![layout_list, vsep(), details].height(Fill);

    container(column![toolbar, hdivider(), body].spacing(0))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(BG)),
            ..Default::default()
        })
        .width(Fill)
        .height(Fill)
        .into()
}
