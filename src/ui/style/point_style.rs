//! Point Style (DDPTYPE) dialog — pick a PDMODE glyph from a grid and set the
//! point size (PDSIZE), relative to the screen or in absolute units.
//!
//! Changes apply live to the active document header; the renderer rebuilds the
//! point glyphs (see `entities::point`). Command entry (`PDMODE` / `PDSIZE`)
//! does the same without the dialog.

use crate::app::Message;
use iced::widget::{button, canvas, column, container, radio, row, text, text_input, Space};
use iced::{mouse, Background, Border, Color, Element, Length, Point, Rectangle, Size, Theme};

const BG: Color = Color { r: 0.15, g: 0.15, b: 0.15, a: 1.0 };
const WHITE: Color = Color { r: 0.92, g: 0.92, b: 0.92, a: 1.0 };
const DIM: Color = Color { r: 0.55, g: 0.55, b: 0.55, a: 1.0 };
const ACCENT: Color = Color { r: 0.30, g: 0.62, b: 0.95, a: 1.0 };
const CELL: Color = Color { r: 0.20, g: 0.20, b: 0.20, a: 1.0 };
const CELL_SEL: Color = Color { r: 0.30, g: 0.62, b: 0.95, a: 1.0 };
const GLYPH: Color = Color { r: 0.90, g: 0.90, b: 0.90, a: 1.0 };

const CELL_PX: f32 = 44.0;

/// Glyph-grid columns (low-nibble shape) × rows (enclosure bits):
///   shapes:     0=dot, 1=none, 2='+', 3='×', 4='|'
///   enclosures: 0=none, 32=circle, 64=square, 96=both
const ENCLOSURES: [i16; 4] = [0, 32, 64, 96];
const SHAPES: [i16; 5] = [0, 1, 2, 3, 4];

/// Canvas that renders a single PDMODE glyph inside a grid cell.
struct GlyphCanvas {
    mode: i16,
}

impl canvas::Program<Message> for GlyphCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (cx, cy) = (bounds.width * 0.5, bounds.height * 0.5);
        let r = bounds.width.min(bounds.height) * 0.30;
        let stroke = canvas::Stroke {
            width: 1.4,
            style: canvas::Style::Solid(GLYPH),
            ..Default::default()
        };
        let line = |a: Point, b: Point| canvas::Path::line(a, b);

        match self.mode & 0x0F {
            0 => frame.fill(&canvas::Path::circle(Point::new(cx, cy), 2.4), GLYPH),
            1 => {}
            2 => {
                frame.stroke(&line(Point::new(cx - r, cy), Point::new(cx + r, cy)), stroke.clone());
                frame.stroke(&line(Point::new(cx, cy - r), Point::new(cx, cy + r)), stroke.clone());
            }
            3 => {
                frame.stroke(
                    &line(Point::new(cx - r, cy - r), Point::new(cx + r, cy + r)),
                    stroke.clone(),
                );
                frame.stroke(
                    &line(Point::new(cx - r, cy + r), Point::new(cx + r, cy - r)),
                    stroke.clone(),
                );
            }
            4 => frame.stroke(&line(Point::new(cx, cy - r), Point::new(cx, cy + r)), stroke.clone()),
            _ => {}
        }
        if self.mode & 32 != 0 {
            frame.stroke(&canvas::Path::circle(Point::new(cx, cy), r), stroke.clone());
        }
        if self.mode & 64 != 0 {
            let sq = canvas::Path::rectangle(Point::new(cx - r, cy - r), Size::new(2.0 * r, 2.0 * r));
            frame.stroke(&sq, stroke.clone());
        }
        vec![frame.into_geometry()]
    }
}

fn cell<'a>(value: i16, selected: bool) -> Element<'a, Message> {
    let glyph = canvas(GlyphCanvas { mode: value })
        .width(Length::Fixed(CELL_PX))
        .height(Length::Fixed(CELL_PX));
    button(glyph)
        .padding(0)
        .on_press(Message::PointStyleSetMode(value))
        .style(move |_: &Theme, status| {
            let bg = if selected {
                CELL_SEL
            } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Color { r: 0.28, g: 0.28, b: 0.28, a: 1.0 }
            } else {
                CELL
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: Color { r: 0.35, g: 0.35, b: 0.35, a: 1.0 },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

fn field_style(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 }),
        border: Border {
            color: Color { r: 0.3, g: 0.3, b: 0.3, a: 1.0 },
            width: 1.0,
            radius: 4.0.into(),
        },
        icon: DIM,
        placeholder: DIM,
        value: WHITE,
        selection: ACCENT,
    }
}

pub fn view_window<'a>(pdmode: i16, relative: bool, size_buf: &str) -> Element<'a, Message> {
    // Glyph grid: a row per enclosure, a cell per shape.
    let mut grid = column![].spacing(6);
    for enc in ENCLOSURES {
        let mut r = row![].spacing(6);
        for sh in SHAPES {
            let value = enc + sh;
            r = r.push(cell(value, value == pdmode));
        }
        grid = grid.push(r);
    }

    let size_row = row![
        text("Point Size:").size(13).color(WHITE),
        Space::new().width(10),
        text_input("0", size_buf)
            .on_input(Message::PointStyleSizeInput)
            .on_submit(Message::PointStyleApplySize)
            .style(field_style)
            .size(13)
            .width(110),
        Space::new().width(6),
        text(if relative { "%" } else { "units" }).size(12).color(DIM),
    ]
    .align_y(iced::Center);

    let radios = column![
        radio(
            "Set Size Relative to Screen",
            true,
            Some(relative),
            Message::PointStyleSizeRelative,
        )
        .size(15)
        .text_size(13),
        radio(
            "Set Size in Absolute Units",
            false,
            Some(relative),
            Message::PointStyleSizeRelative,
        )
        .size(15)
        .text_size(13),
    ]
    .spacing(6);

    let ok = button(text("OK").size(13).color(WHITE))
        .padding([5, 22])
        .on_press(Message::PointStyleOk)
        .style(|_: &Theme, status| {
            let bg = if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Color { r: 0.32, g: 0.55, b: 0.85, a: 1.0 }
            } else {
                ACCENT
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: WHITE,
                border: Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            }
        });

    container(
        column![
            text("Point Style").size(18).color(WHITE),
            Space::new().height(6),
            grid,
            Space::new().height(12),
            size_row,
            Space::new().height(8),
            radios,
            Space::new().height(12),
            row![Space::new().width(Length::Fill), ok],
        ]
        .spacing(4)
        .padding(20),
    )
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(BG)),
        ..Default::default()
    })
    .into()
}
