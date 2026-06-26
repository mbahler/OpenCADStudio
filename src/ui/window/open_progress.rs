//! Modal overlay shown while a CAD file is being loaded.
//!
//! Displays the file name, size, current phase, an indeterminate animated
//! progress bar, and a Cancel button.

use iced::time::Instant;
use iced::widget::{button, column, container, row, stack, text, Space};
use iced::{Background, Border, Color, Element, Fill, Length, Theme};
use std::sync::atomic::Ordering;

use crate::app::{
    Message, OpenProgress, OPEN_PHASE_CACHING, OPEN_PHASE_FINALIZING, OPEN_PHASE_PARSING,
    OPEN_PHASE_READING,
};

const CARD_WIDTH: f32 = 420.0;
const BAR_TRACK_WIDTH: f32 = 380.0;
const BAR_TRACK_HEIGHT: f32 = 6.0;
const BAR_WINDOW_WIDTH: f32 = 100.0;
/// Period of one back-and-forth bounce, in milliseconds.
const BAR_PERIOD_MS: f32 = 1800.0;

fn phase_label(phase: u8) -> &'static str {
    match phase {
        OPEN_PHASE_READING => "Reading file…",
        OPEN_PHASE_PARSING => "Parsing entities…",
        OPEN_PHASE_CACHING => "Building scene caches…",
        OPEN_PHASE_FINALIZING => "Finalizing…",
        _ => "Working…",
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Compute the left-offset of the moving highlight inside the track.
/// Bounces left↔right using a triangle wave so the user sees motion even when
/// the actual phase atomic stays put for a while.
fn bar_offset(elapsed_ms: f32) -> f32 {
    let travel = (BAR_TRACK_WIDTH - BAR_WINDOW_WIDTH).max(0.0);
    let cycle = (elapsed_ms / BAR_PERIOD_MS).fract();
    let tri = if cycle < 0.5 {
        cycle * 2.0
    } else {
        (1.0 - cycle) * 2.0
    };
    tri * travel
}

pub fn view<'a>(progress: &'a OpenProgress, now: Instant) -> Element<'a, Message> {
    let phase = progress.phase.load(Ordering::Relaxed);
    let elapsed_ms = now.saturating_duration_since(progress.started).as_millis() as f32;

    // ── Animated indeterminate bar ────────────────────────────────────────
    let offset = bar_offset(elapsed_ms);
    let trailing = (BAR_TRACK_WIDTH - BAR_WINDOW_WIDTH - offset).max(0.0);

    let bar_window: Element<'_, Message> = container(Space::new().width(Length::Fixed(BAR_WINDOW_WIDTH)).height(Length::Fixed(BAR_TRACK_HEIGHT)))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(Color {
                r: 0.30,
                g: 0.62,
                b: 0.95,
                a: 1.0,
            })),
            border: Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

    let bar_moving: Element<'_, Message> = row![
        Space::new()
            .width(Length::Fixed(offset))
            .height(Length::Fixed(BAR_TRACK_HEIGHT)),
        bar_window,
        Space::new()
            .width(Length::Fixed(trailing))
            .height(Length::Fixed(BAR_TRACK_HEIGHT)),
    ]
    .into();

    let bar_track: Element<'_, Message> = container(
        stack![
            container(Space::new().width(Length::Fixed(BAR_TRACK_WIDTH)).height(Length::Fixed(BAR_TRACK_HEIGHT)))
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.18,
                        g: 0.18,
                        b: 0.18,
                        a: 1.0,
                    })),
                    border: Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            bar_moving,
        ]
        .width(Length::Fixed(BAR_TRACK_WIDTH))
        .height(Length::Fixed(BAR_TRACK_HEIGHT)),
    )
    .into();

    // ── Card body ────────────────────────────────────────────────────────
    let title = text("Opening file")
        .size(15)
        .color(Color::WHITE);

    let name_line = text(format!(
        "{}  ({})",
        progress.name,
        format_size(progress.size_bytes)
    ))
    .size(13)
    .color(Color {
        r: 0.82,
        g: 0.82,
        b: 0.82,
        a: 1.0,
    });

    let phase_line = text(phase_label(phase))
        .size(12)
        .color(Color {
            r: 0.70,
            g: 0.80,
            b: 0.95,
            a: 1.0,
        });

    let cancel_btn: Element<'_, Message> = button(text("Cancel").size(12).color(Color::WHITE))
        .on_press(Message::OpenCancel)
        .style(|_: &Theme, status| {
            let bg = match status {
                button::Status::Hovered => Color {
                    r: 0.32,
                    g: 0.32,
                    b: 0.32,
                    a: 1.0,
                },
                button::Status::Pressed => Color {
                    r: 0.42,
                    g: 0.18,
                    b: 0.18,
                    a: 1.0,
                },
                _ => Color {
                    r: 0.22,
                    g: 0.22,
                    b: 0.22,
                    a: 1.0,
                },
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: Color {
                        r: 0.40,
                        g: 0.40,
                        b: 0.40,
                        a: 1.0,
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                text_color: Color::WHITE,
                ..Default::default()
            }
        })
        .padding([4, 14])
        .into();

    let cancel_row: Element<'_, Message> = container(cancel_btn).align_right(Fill).into();

    let card = container(
        column![title, name_line, bar_track, phase_line, cancel_row]
            .spacing(10)
            .width(Length::Fixed(CARD_WIDTH)),
    )
    .padding([18, 22])
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(Color {
            r: 0.13,
            g: 0.13,
            b: 0.13,
            a: 0.98,
        })),
        border: Border {
            color: Color {
                r: 0.45,
                g: 0.45,
                b: 0.45,
                a: 1.0,
            },
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    });

    // ── Backdrop (click-blocker + dim) ────────────────────────────────────
    let backdrop: Element<'_, Message> = container(Space::new().width(Fill).height(Fill))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.55,
            })),
            ..Default::default()
        })
        .width(Fill)
        .height(Fill)
        .into();

    let centered: Element<'_, Message> = container(card)
        .center_x(Fill)
        .center_y(Fill)
        .width(Fill)
        .height(Fill)
        .into();

    stack![backdrop, centered].width(Fill).height(Fill).into()
}
