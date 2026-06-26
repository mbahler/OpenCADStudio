use crate::app::Message;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Background, Border, Color, Element, Fill, Theme};

const BG: Color = Color {
    r: 0.15,
    g: 0.15,
    b: 0.15,
    a: 1.0,
};
const PANEL: Color = Color {
    r: 0.12,
    g: 0.12,
    b: 0.12,
    a: 1.0,
};
const BORDER: Color = Color {
    r: 0.30,
    g: 0.30,
    b: 0.30,
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
const WHITE: Color = Color {
    r: 0.92,
    g: 0.92,
    b: 0.92,
    a: 1.0,
};

/// Renders one of the two "Installed" / "Latest" cards. The `highlight`
/// flag tints the border + label with the accent colour, making the new
/// version the visual anchor of the row.
fn version_card<'a>(label: &'static str, value: String, highlight: bool) -> Element<'a, Message> {
    let label_color = if highlight { ACCENT } else { DIM };
    let value_color = WHITE;
    let border_color = if highlight { ACCENT } else { BORDER };
    let bg = if highlight {
        Color { r: 0.10, g: 0.16, b: 0.22, a: 1.0 } // accent-tinted dark
    } else {
        PANEL
    };
    container(
        column![
            text(label).size(10).color(label_color),
            text(value).size(20).color(value_color),
        ]
        .spacing(4)
        .align_x(iced::Center),
    )
    .width(Fill)
    .padding(iced::Padding {
        top: 14.0,
        right: 12.0,
        bottom: 14.0,
        left: 12.0,
    })
    .align_x(iced::Center)
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Light-weight renderer for a single line of GitHub-style release-notes
/// markdown. Recognises:
///   * `## Heading` → bold accent line
///   * `### Heading` → smaller bold line
///   * `- bullet`   → indented bullet text
///   * `**bold**` runs and `` `code` `` runs (rendered tonally, not styled
///     differently — iced's text widget has no inline run styling).
/// Anything else is plain body text. Strips the markdown markers so the
/// dialog reads cleanly even if the user has a Patreon-formatted note.
fn render_notes_line<'a>(raw: &str) -> Element<'a, Message> {
    let trimmed = raw.trim_end();
    if trimmed.is_empty() {
        return Space::new()
            .height(iced::Length::Fixed(6.0))
            .into();
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
        return text(strip_inline_md(rest))
            .size(13)
            .color(ACCENT)
            .into();
    }
    if let Some(rest) = trimmed.strip_prefix("### ") {
        return text(strip_inline_md(rest))
            .size(12)
            .color(WHITE)
            .into();
    }
    if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
        return row![
            container(crate::ui::icons::tinted(crate::ui::icons::DOT, 5.0, DIM)).width(14),
            text(strip_inline_md(rest)).size(11).color(WHITE),
        ]
        .spacing(4)
        .into();
    }
    text(strip_inline_md(trimmed)).size(11).color(WHITE).into()
}

/// Drop `**…**` and `` `…` `` markers without preserving emphasis (iced 0.14
/// Text widgets style the whole string uniformly). Keeps the inner text.
fn strip_inline_md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next();
            continue;
        }
        if c == '`' {
            continue;
        }
        out.push(c);
    }
    out
}

pub fn view_window<'a>(latest: &'a str, body: &'a str) -> Element<'a, Message> {
    let header = container(
        column![
            text("New Release Available").size(20).color(ACCENT),
            text("A newer Open CAD Studio version is published on GitHub.")
                .size(11)
                .color(DIM),
        ]
        .spacing(4)
        .align_x(iced::Center),
    )
    .width(Fill)
    .padding(iced::Padding {
        top: 14.0,
        right: 0.0,
        bottom: 6.0,
        left: 0.0,
    })
    .align_x(iced::Center);

    // Two side-by-side version cards (Installed → Latest) with the latest
    // one accent-tinted to draw the eye. Replaces the previous label/value
    // row layout. The arrow between them is purely decorative.
    let installed = version_card(
        "Installed",
        format!("v{}", env!("CARGO_PKG_VERSION")),
        false,
    );
    let latest_card = version_card("Latest", format!("v{}", latest), true);
    let arrow = container(crate::ui::icons::tinted(
        crate::ui::icons::ARROW_LONG_RIGHT,
        20.0,
        DIM,
    ))
    .width(iced::Length::Fixed(32.0))
        .height(Fill)
        .align_x(iced::Center)
        .align_y(iced::Center);
    let info_block = row![installed, arrow, latest_card]
        .spacing(0)
        .align_y(iced::Center)
        .width(Fill);

    let later_btn = button(text("Later").size(11))
        .on_press(Message::UpdateNoticeClose)
        .style(|_: &Theme, st| button::Style {
            background: Some(Background::Color(match st {
                button::Status::Hovered | button::Status::Pressed => Color {
                    r: 0.22,
                    g: 0.22,
                    b: 0.22,
                    a: 1.0,
                },
                _ => Color {
                    r: 0.18,
                    g: 0.18,
                    b: 0.18,
                    a: 1.0,
                },
            })),
            text_color: WHITE,
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .padding([6, 16]);

    let open_btn = button(text("Open Release Page").size(11))
        .on_press(Message::UpdateNoticeOpenRelease)
        .style(|_: &Theme, st| button::Style {
            background: Some(Background::Color(match st {
                button::Status::Hovered | button::Status::Pressed => Color {
                    r: 0.20,
                    g: 0.42,
                    b: 0.72,
                    a: 1.0,
                },
                _ => ACCENT,
            })),
            text_color: WHITE,
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .padding([6, 16]);

    let footer = row![Space::new().width(Fill), later_btn, open_btn]
        .spacing(8)
        .align_y(iced::Center)
        .padding(iced::Padding {
            top: 14.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        });

    // Release notes panel. Rendered as a light-markdown column inside a
    // bordered scrollable so long bodies stay contained and don't
    // explode the window. Empty body → "No release notes provided."
    let notes_heading = container(text("What's new").size(11).color(DIM))
        .padding(iced::Padding {
            top: 10.0,
            right: 0.0,
            bottom: 4.0,
            left: 0.0,
        });

    let notes_body: Element<'a, Message> = if body.trim().is_empty() {
        text("No release notes provided.")
            .size(11)
            .color(DIM)
            .into()
    } else {
        let mut col = column![].spacing(4);
        for line in body.lines() {
            col = col.push(render_notes_line(line));
        }
        scrollable(container(col).padding([10, 14])).height(Fill).into()
    };

    let notes_block = container(notes_body)
        .width(Fill)
        .height(Fill)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(PANEL)),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

    // Wrap notes_block in a Fill-height container outside the column so it
    // greedily claims every pixel left over after the fixed-height rows
    // (header, version cards, heading, footer). Without this iced lets the
    // notes panel shrink to its content height, leaving a gap above the
    // footer.
    let notes_fill = container(notes_block)
        .width(Fill)
        .height(Fill);

    container(
        column![header, info_block, notes_heading, notes_fill, footer]
            .spacing(0)
            .height(Fill)
            .padding(iced::Padding {
                top: 0.0,
                right: 20.0,
                bottom: 20.0,
                left: 20.0,
            }),
    )
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(BG)),
        ..Default::default()
    })
    .width(Fill)
    .height(Fill)
    .into()
}
