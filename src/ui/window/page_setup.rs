//! Page Setup window — fills the entire OS window.

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

fn btn(accent: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
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
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

fn pill(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
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
        shadow: iced::Shadow::default(),
        snap: false,
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

fn section_label<'a>(s: &'static str) -> Element<'a, Message> {
    text(s).size(11).color(DIM).into()
}

pub fn view_window<'a>(
    w_buf: &'a str,
    h_buf: &'a str,
    plot_area: &'a str,
    center: bool,
    offset_x: &'a str,
    offset_y: &'a str,
    rotation: &'a str,
    scale: &'a str,
) -> Element<'a, Message> {
    // ── Toolbar ───────────────────────────────────────────────────────────
    let toolbar = container(
        row![
            button(text("Cancel").size(12))
                .on_press(Message::PageSetupClose)
                .style(btn(false))
                .padding([4, 14]),
            Space::new().width(Fill),
            button(text("OK").size(12))
                .on_press(Message::PageSetupCommit)
                .style(btn(true))
                .padding([4, 20]),
        ]
        .align_y(iced::Center),
    )
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(TB)),
        ..Default::default()
    })
    .width(Fill)
    .padding([5, 10]);

    let lbl = |s: &'static str| text(s).size(11).color(DIM).width(130);

    // ── Paper size presets ────────────────────────────────────────────────
    let presets1 = row![
        button(text("A4 P").size(10))
            .on_press(Message::PageSetupPreset("A4 Portrait".into()))
            .style(pill(false))
            .padding([3, 6]),
        button(text("A4 L").size(10))
            .on_press(Message::PageSetupPreset("A4 Landscape".into()))
            .style(pill(false))
            .padding([3, 6]),
        button(text("A3 P").size(10))
            .on_press(Message::PageSetupPreset("A3 Portrait".into()))
            .style(pill(false))
            .padding([3, 6]),
        button(text("A3 L").size(10))
            .on_press(Message::PageSetupPreset("A3 Landscape".into()))
            .style(pill(false))
            .padding([3, 6]),
    ]
    .spacing(4);

    let presets2 = row![
        button(text("A2 L").size(10))
            .on_press(Message::PageSetupPreset("A2 Landscape".into()))
            .style(pill(false))
            .padding([3, 6]),
        button(text("A1 L").size(10))
            .on_press(Message::PageSetupPreset("A1 Landscape".into()))
            .style(pill(false))
            .padding([3, 6]),
        button(text("A0 L").size(10))
            .on_press(Message::PageSetupPreset("A0 Landscape".into()))
            .style(pill(false))
            .padding([3, 6]),
        button(text("Letter").size(10))
            .on_press(Message::PageSetupPreset("Letter Landscape".into()))
            .style(pill(false))
            .padding([3, 6]),
    ]
    .spacing(4);

    // ── Rotation buttons ──────────────────────────────────────────────────
    let rot_row = row![
        button(text("0°").size(10))
            .on_press(Message::PageSetupRotation("0".into()))
            .style(pill(rotation == "0"))
            .padding([3, 8]),
        button(text("90°").size(10))
            .on_press(Message::PageSetupRotation("90".into()))
            .style(pill(rotation == "90"))
            .padding([3, 8]),
        button(text("180°").size(10))
            .on_press(Message::PageSetupRotation("180".into()))
            .style(pill(rotation == "180"))
            .padding([3, 8]),
        button(text("270°").size(10))
            .on_press(Message::PageSetupRotation("270".into()))
            .style(pill(rotation == "270"))
            .padding([3, 8]),
    ]
    .spacing(4);

    // ── Scale buttons ─────────────────────────────────────────────────────
    let scale_row1 = row![
        button(text("Fit").size(10))
            .on_press(Message::PageSetupScale("Fit".into()))
            .style(pill(scale == "Fit"))
            .padding([3, 8]),
        button(text("1:1").size(10))
            .on_press(Message::PageSetupScale("1:1".into()))
            .style(pill(scale == "1:1"))
            .padding([3, 8]),
        button(text("1:2").size(10))
            .on_press(Message::PageSetupScale("1:2".into()))
            .style(pill(scale == "1:2"))
            .padding([3, 8]),
        button(text("1:5").size(10))
            .on_press(Message::PageSetupScale("1:5".into()))
            .style(pill(scale == "1:5"))
            .padding([3, 8]),
        button(text("1:10").size(10))
            .on_press(Message::PageSetupScale("1:10".into()))
            .style(pill(scale == "1:10"))
            .padding([3, 8]),
    ]
    .spacing(4);

    let scale_row2 = row![
        button(text("1:20").size(10))
            .on_press(Message::PageSetupScale("1:20".into()))
            .style(pill(scale == "1:20"))
            .padding([3, 8]),
        button(text("1:50").size(10))
            .on_press(Message::PageSetupScale("1:50".into()))
            .style(pill(scale == "1:50"))
            .padding([3, 8]),
        button(text("1:100").size(10))
            .on_press(Message::PageSetupScale("1:100".into()))
            .style(pill(scale == "1:100"))
            .padding([3, 8]),
        button(text("2:1").size(10))
            .on_press(Message::PageSetupScale("2:1".into()))
            .style(pill(scale == "2:1"))
            .padding([3, 8]),
    ]
    .spacing(4);

    // ── Main scrollable form ──────────────────────────────────────────────
    let form = column![
        section_label("Paper Size"),
        presets1,
        presets2,
        row![
            lbl("Width (mm)"),
            text_input("297", w_buf)
                .on_input(Message::PageSetupWidthEdit)
                .on_submit(Message::PageSetupCommit)
                .style(field_style)
                .size(12)
                .width(90),
        ]
        .spacing(8)
        .align_y(iced::Center),
        row![
            lbl("Height (mm)"),
            text_input("210", h_buf)
                .on_input(Message::PageSetupHeightEdit)
                .on_submit(Message::PageSetupCommit)
                .style(field_style)
                .size(12)
                .width(90),
        ]
        .spacing(8)
        .align_y(iced::Center),
        hdivider(),
        section_label("Plot Area"),
        row![
            button(text("Layout").size(10))
                .on_press(Message::PageSetupPlotArea("Layout".into()))
                .style(pill(plot_area == "Layout"))
                .padding([3, 8]),
            button(text("Extents").size(10))
                .on_press(Message::PageSetupPlotArea("Extents".into()))
                .style(pill(plot_area == "Extents"))
                .padding([3, 8]),
        ]
        .spacing(6),
        hdivider(),
        section_label("Position"),
        button(
            row![
                crate::ui::icons::check_cell(center, Color::WHITE),
                text("Center on page").size(11),
            ]
            .spacing(2)
            .align_y(iced::Center)
        )
        .on_press(Message::PageSetupCenterToggle)
        .style(pill(center))
        .padding([4, 10]),
        row![
            lbl("Offset X (mm)"),
            text_input("0", offset_x)
                .on_input(Message::PageSetupOffsetXEdit)
                .style(field_style)
                .size(12)
                .width(90),
        ]
        .spacing(8)
        .align_y(iced::Center),
        row![
            lbl("Offset Y (mm)"),
            text_input("0", offset_y)
                .on_input(Message::PageSetupOffsetYEdit)
                .style(field_style)
                .size(12)
                .width(90),
        ]
        .spacing(8)
        .align_y(iced::Center),
        hdivider(),
        section_label("Rotation"),
        rot_row,
        hdivider(),
        section_label("Plot Scale"),
        scale_row1,
        scale_row2,
    ]
    .spacing(10)
    .padding(16)
    .width(Fill);

    let content = scrollable(form).width(Fill).height(Fill);

    container(column![toolbar, hdivider(), content].spacing(0))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(BG)),
            ..Default::default()
        })
        .width(Fill)
        .height(Fill)
        .into()
}
