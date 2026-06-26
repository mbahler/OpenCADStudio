//! Shared style-manager list row.
//!
//! Every style manager renders the same left-hand list of style names where a
//! single click selects and a double click renames inline.
//!
//! The whole row is a `mouse_area` (not a `button`): an inner interactive
//! widget would capture the press and the parent `mouse_area` would never see
//! the second click, so `on_double_click` would never fire. `mouse_area`
//! carries both `on_press` (select) and `on_double_click` (rename) itself.

use crate::app::{Message, StyleKind};
use iced::widget::{container, mouse_area, row, text, text_input};
use iced::{Background, Color, Element, Fill, Theme};

/// Shared id for the inline rename field, so the rename-start handler can focus
/// it the moment the row turns editable.
pub fn rename_input_id() -> iced::widget::Id {
    iced::widget::Id::new("style-rename-input")
}

const TEXT: Color = Color {
    r: 0.88,
    g: 0.88,
    b: 0.88,
    a: 1.0,
};
const ACTIVE: Color = Color {
    r: 0.20,
    g: 0.40,
    b: 0.70,
    a: 1.0,
};
const CURRENT_CHECK: Color = Color {
    r: 0.30,
    g: 0.82,
    b: 0.36,
    a: 1.0,
};

/// One row of the style list. Renders an editable `text_input` when `name` is
/// the style being renamed (`rename_active`), otherwise a selectable row whose
/// double click starts the rename. The current style gets a green ✓.
pub fn item<'a>(
    name: &str,
    is_current: bool,
    is_selected: bool,
    kind: StyleKind,
    on_select: Message,
    rename_active: Option<&str>,
    rename_buf: &'a str,
) -> Element<'a, Message> {
    if rename_active == Some(name) {
        text_input("", rename_buf)
            .id(iced::widget::Id::new("style-rename-input"))
            .on_input(Message::StyleRenameEdit)
            .on_submit(Message::StyleRenameCommit(kind))
            .size(11)
            .padding([4, 8])
            .width(Fill)
            .into()
    } else {
        // Fixed-width ✓ column keeps every name left-aligned whether or not the
        // row is current.
        let check = crate::ui::icons::check_cell(is_current, CURRENT_CHECK);
        let label = row![check, text(name.to_string()).size(11).color(TEXT)]
            .align_y(iced::Center);
        let cell = container(label)
            .padding([4, 8])
            .width(Fill)
            .style(move |_: &Theme| container::Style {
                background: is_selected.then_some(Background::Color(ACTIVE)),
                text_color: Some(TEXT),
                ..Default::default()
            });
        mouse_area(cell)
            .on_press(on_select)
            .on_double_click(Message::StyleRenameStart(kind, name.to_string()))
            .into()
    }
}
