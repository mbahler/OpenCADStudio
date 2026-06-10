//! Shared scaffold for every style-manager window.
//!
//! All five managers (text / dimension / table / multileader / multiline)
//! share the same frame: a top toolbar (New / Copy / Delete on the left,
//! manager-specific actions such as Set Current / Apply on the right), a style
//! list on the left, and a property editor on the right. Only the editor
//! differs, so each manager builds just that and hands it to [`view`]; the
//! toolbar, list, inline-rename wiring and chrome live here once.

use crate::app::{Message, StyleKind};
use iced::widget::button::{Status, Style};
use iced::widget::{button, column, container, row, scrollable, text, Space};
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
const LIST: Color = Color {
    r: 0.12,
    g: 0.12,
    b: 0.12,
    a: 1.0,
};

/// Everything the shared frame needs. The per-manager `editor` element is the
/// only bespoke part.
///
/// The toolbar is uniform across every manager: New / Copy / Delete on the
/// left, then **Set Current** and **Apply** on the right. Each manager just
/// supplies the two messages, so the right side can never drift or go missing.
///
/// Two lifetimes: `'a` is what the returned element keeps alive (the editor and
/// the rename buffer the inline `text_input` borrows); `'b` is the transient
/// list data (`styles`, `selected`, …) that the frame only reads while building
/// rows, so callers may pass a locally-built `Vec`.
pub struct Scaffold<'a, 'b> {
    pub kind: StyleKind,
    pub styles: &'b [String],
    pub selected: &'b str,
    /// Current style for this manager, marked with a ◀ in the list. `None`
    /// when the manager has no "current" concept.
    pub current: Option<&'b str>,
    pub rename_active: Option<&'b str>,
    pub rename_buf: &'a str,
    pub on_new: Message,
    pub on_copy: Message,
    pub on_delete: Message,
    /// Tuple-variant constructor for the per-row select message
    /// (e.g. `Message::TextStyleDialogSelect`).
    pub on_select: fn(String) -> Message,
    /// "Set Current" action (right side). Every manager has one.
    pub on_set_current: Message,
    /// "Apply" action (right side, primary). Every manager has one.
    pub on_apply: Message,
    pub editor: Element<'a, Message>,
}

pub fn view<'a, 'b>(s: Scaffold<'a, 'b>) -> Element<'a, Message> {
    // ── Toolbar: New / Copy / Delete | … | Set Current / Apply ────────────
    let bar = row![
        tb_button("New", s.on_new, false),
        tb_button("Copy", s.on_copy, false),
        tb_button("Delete", s.on_delete, false),
        Space::new().width(Fill),
        tb_button("Set Current", s.on_set_current, false),
        tb_button("Apply", s.on_apply, true),
    ]
    .spacing(4)
    .align_y(iced::Center);
    let toolbar = container(bar)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(TB)),
            ..Default::default()
        })
        .width(Fill)
        .padding([5, 8]);

    // ── Left: style list (single click selects, double click renames) ─────
    let rows: Vec<Element<'_, Message>> = s
        .styles
        .iter()
        .map(|name| {
            let is_sel = name.as_str() == s.selected;
            // Mark the current style with a leading ✓; pad the rest so names
            // line up.
            let label = if s.current == Some(name.as_str()) {
                format!("✓ {name}")
            } else {
                format!("    {name}")
            };
            crate::ui::style_list::item(
                name,
                label,
                is_sel,
                s.kind,
                (s.on_select)(name.clone()),
                s.rename_active,
                s.rename_buf,
            )
        })
        .collect();

    let list_panel = container(
        column![
            text("Styles").size(10).color(DIM),
            container(scrollable(column(rows).spacing(1)).height(Fill))
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
    .width(170)
    .height(Fill)
    .padding(iced::Padding {
        top: 12.0,
        right: 8.0,
        bottom: 12.0,
        left: 12.0,
    });

    let body = row![list_panel, vsep(), s.editor].height(Fill);

    container(column![toolbar, hdivider(), body])
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(BG)),
            ..Default::default()
        })
        .width(Fill)
        .height(Fill)
        .into()
}

// ── Shared chrome ──────────────────────────────────────────────────────────

fn tb_button<'a>(label: &'a str, msg: Message, accent: bool) -> Element<'a, Message> {
    let pad = if accent { [4, 14] } else { [4, 10] };
    button(text(label).size(11))
        .on_press(msg)
        .style(btn_s(accent))
        .padding(pad)
        .into()
}

fn btn_s(accent: bool) -> impl Fn(&Theme, Status) -> Style {
    move |_: &Theme, st| Style {
        background: Some(Background::Color(match (accent, st) {
            (true, Status::Hovered | Status::Pressed) => Color {
                r: 0.20,
                g: 0.42,
                b: 0.72,
                a: 1.0,
            },
            (false, Status::Hovered | Status::Pressed) => Color {
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
