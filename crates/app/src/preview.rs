//! Live pictures for visual options.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;

use niri_settings_core::Value;
use niri_settings_core::session::Session;

use crate::util::hex_to_rgba;

/// A picture of columns, gaps, the focus ring, borders and shadows.
pub fn appearance_preview(session: &Session) -> gtk::DrawingArea {
    appearance_preview_with(session, &[])
}

/// The same picture with staged values taking precedence over the config, so
/// the welcome flow can show a preset before it is written.
pub fn appearance_preview_with(session: &Session, overrides: &[(&str, Value)]) -> gtk::DrawingArea {
    let gaps = number(session, overrides, "layout.gaps", 16.0);
    let ring_width = number(session, overrides, "layout.focus-ring.width", 4.0);
    let ring_on = boolean(session, overrides, "layout.focus-ring.off", true);
    let ring_active = color(
        session,
        overrides,
        "layout.focus-ring.active-color",
        "#7fc8ff",
    );
    let ring_inactive = color(
        session,
        overrides,
        "layout.focus-ring.inactive-color",
        "#505050",
    );
    let border_on = boolean(session, overrides, "layout.border.off", false);
    let border_width = number(session, overrides, "layout.border.width", 4.0);
    let border_color = color(session, overrides, "layout.border.active-color", "#ffc87f");
    let shadow_on = boolean(session, overrides, "layout.shadow.on", false);
    let shadow_softness = number(session, overrides, "layout.shadow.softness", 30.0);
    let shadow_spread = number(session, overrides, "layout.shadow.spread", 5.0);
    let shadow_color = color(session, overrides, "layout.shadow.color", "#00000077");
    let background = color(session, overrides, "layout.background-color", "#00000000");

    let area = gtk::DrawingArea::builder().content_height(190).build();
    area.set_draw_func(move |_, cr, width, height| {
        let width = width as f64;
        let height = height as f64;
        cr.set_source_rgba(0.11, 0.11, 0.12, 1.0);
        cr.paint().ok();
        let bg = hex_to_rgba(&background);
        cr.set_source_rgba(
            bg.red() as f64,
            bg.green() as f64,
            bg.blue() as f64,
            bg.alpha() as f64,
        );
        cr.paint().ok();

        let gap = gaps.clamp(0.0, 50.0) * 0.7;
        let margin = 18.0;
        let columns = 3.0;
        let column_width = (width - margin * 2.0 - gap * (columns - 1.0)) / columns;
        let top = margin + 8.0;
        let column_height = height - margin * 2.0 - 10.0;

        for index in 0..3 {
            let x = margin + index as f64 * (column_width + gap);
            let focused = index == 1;
            let y = top;
            if shadow_on {
                draw_shadow(
                    cr,
                    x,
                    y,
                    column_width,
                    column_height,
                    shadow_softness,
                    shadow_spread,
                    &shadow_color,
                );
            }
            rounded_rect(cr, x, y, column_width, column_height, 8.0);
            cr.set_source_rgba(0.20, 0.21, 0.23, 1.0);
            cr.fill().ok();

            // Draw the contents inside the window shape, never over the gap.
            let _ = cr.save();
            rounded_rect(cr, x, y, column_width, column_height, 8.0);
            cr.clip();
            draw_window_content(
                cr,
                index,
                x,
                y,
                column_width,
                column_height,
                focused,
                &ring_active,
            );
            let _ = cr.restore();

            let border = if focused {
                border_color.clone()
            } else {
                "rgba(255,255,255,0.08)".to_string()
            };
            let border_width = if focused && border_on {
                border_width
            } else {
                1.0
            };
            if focused && ring_on {
                draw_ring(
                    cr,
                    x,
                    y,
                    column_width,
                    column_height,
                    ring_width,
                    &ring_active,
                );
            } else if border_on {
                draw_ring(cr, x, y, column_width, column_height, border_width, &border);
            } else {
                draw_ring(cr, x, y, column_width, column_height, 1.0, &ring_inactive);
            }
        }
    });
    area
}

/// A miniature window: a title bar and content that differs per column, so
/// the picture reads as three apps rather than three placeholders.
#[allow(clippy::too_many_arguments)]
fn draw_window_content(
    cr: &gtk::cairo::Context,
    index: usize,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    focused: bool,
    accent: &str,
) {
    use std::f64::consts::PI;
    let bar = 24.0;
    let lift = if focused { 1.0 } else { 0.55 };

    // Title bar.
    cr.rectangle(x, y, width, bar);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.055 * lift);
    cr.fill().ok();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.07);
    cr.set_line_width(1.0);
    cr.move_to(x, y + bar);
    cr.line_to(x + width, y + bar);
    cr.stroke().ok();

    // A window dot and a title pill. Neutral shapes, not another desktop's
    // window buttons.
    let accent = hex_to_rgba(accent);
    if focused {
        cr.set_source_rgba(
            accent.red() as f64,
            accent.green() as f64,
            accent.blue() as f64,
            0.95,
        );
    } else {
        cr.set_source_rgba(0.55, 0.58, 0.63, 0.55);
    }
    cr.arc(x + 12.0, y + bar / 2.0, 3.0, 0.0, 2.0 * PI);
    cr.fill().ok();
    rounded_rect(
        cr,
        x + 22.0,
        y + bar / 2.0 - 3.0,
        (width * 0.22).clamp(24.0, 64.0),
        6.0,
        3.0,
    );
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.18 * lift);
    cr.fill().ok();
    for step in 0..3 {
        cr.arc(
            x + width - 12.0 - step as f64 * 8.0,
            y + bar / 2.0,
            1.8,
            0.0,
            2.0 * PI,
        );
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.22 * lift);
        cr.fill().ok();
    }

    let left = x + 12.0;
    let inner = width - 24.0;
    let top = y + bar + 12.0;
    let bottom = y + height - 12.0;
    let line = |cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, alpha: f64| {
        rounded_rect(cr, x, y, w, 4.0, 2.0);
        cr.set_source_rgba(0.85, 0.87, 0.92, alpha);
        cr.fill().ok();
    };

    match index {
        0 => {
            // A sidebar and a document.
            let side = (width * 0.26).clamp(34.0, 64.0);
            cr.rectangle(x, y + bar, side, height - bar);
            cr.set_source_rgba(0.10, 0.11, 0.13, 0.55);
            cr.fill().ok();
            for row in 0..4 {
                let w = side - 16.0 - if row == 3 { 8.0 } else { 0.0 };
                line(
                    cr,
                    x + 8.0,
                    y + bar + 14.0 + row as f64 * 14.0,
                    w,
                    0.20 * lift,
                );
            }
            let start = x + side + 12.0;
            let w = x + width - 12.0 - start;
            for row in 0..4 {
                let shorten = if row == 1 { 0.7 } else { 1.0 };
                line(cr, start, top + row as f64 * 15.0, w * shorten, 0.24 * lift);
            }
        }
        1 => {
            // The focused window: one accent panel and a few lines.
            let hero = ((bottom - top) * 0.5).max(36.0);
            rounded_rect(cr, left, top, inner, hero, 6.0);
            cr.set_source_rgba(
                accent.red() as f64,
                accent.green() as f64,
                accent.blue() as f64,
                0.22,
            );
            cr.fill().ok();
            line(cr, left + 10.0, top + 12.0, inner * 0.45, 0.5);
            line(cr, left + 10.0, top + 24.0, inner * 0.7, 0.30);
            let rest = top + hero + 14.0;
            for row in 0..2 {
                let shorten = if row == 1 { 0.6 } else { 0.85 };
                line(cr, left, rest + row as f64 * 14.0, inner * shorten, 0.26);
            }
        }
        _ => {
            // A grid of cards.
            let gap = 8.0;
            let card_w = (inner - gap) / 2.0;
            let card_h = ((bottom - top) - gap).max(20.0) / 2.0;
            for row in 0..2 {
                for col in 0..2 {
                    let cx = left + col as f64 * (card_w + gap);
                    let cy = top + row as f64 * (card_h + gap);
                    rounded_rect(cr, cx, cy, card_w, card_h, 5.0);
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.055 * lift);
                    cr.fill().ok();
                    line(cr, cx + 8.0, cy + card_h * 0.55, card_w - 16.0, 0.18 * lift);
                }
            }
        }
    }
}

fn draw_ring(
    cr: &gtk::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    thickness: f64,
    color: &str,
) {
    let rgba = hex_to_rgba(color);
    rounded_rect(
        cr,
        x - thickness,
        y - thickness,
        width + thickness * 2.0,
        height + thickness * 2.0,
        8.0 + thickness,
    );
    cr.set_source_rgba(
        rgba.red() as f64,
        rgba.green() as f64,
        rgba.blue() as f64,
        rgba.alpha() as f64,
    );
    cr.set_line_width(thickness);
    cr.stroke().ok();
}

#[allow(clippy::too_many_arguments)]
fn draw_shadow(
    cr: &gtk::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    softness: f64,
    spread: f64,
    color: &str,
) {
    let rgba = hex_to_rgba(color);
    let steps = 6;
    for step in (0..steps).rev() {
        let grow = spread * (step as f64 / steps as f64) + softness * 0.15;
        let alpha = rgba.alpha() as f64 * (1.0 - step as f64 / steps as f64) * 0.5;
        rounded_rect(
            cr,
            x - grow,
            y - grow,
            width + grow * 2.0,
            height + grow * 2.0,
            10.0,
        );
        cr.set_source_rgba(
            rgba.red() as f64,
            rgba.green() as f64,
            rgba.blue() as f64,
            alpha,
        );
        cr.fill().ok();
    }
}

pub(crate) fn rounded_rect(
    cr: &gtk::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    cr.new_sub_path();
    cr.arc(
        x + width - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x + radius,
        y + height - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        1.5 * std::f64::consts::PI,
    );
    cr.close_path();
}

/// A moving column that follows the animation slowdown.
pub fn animation_preview(session: &Session) -> gtk::DrawingArea {
    animation_preview_with(session, &[])
}

/// The animation preview with staged values taking precedence.
pub fn animation_preview_with(session: &Session, overrides: &[(&str, Value)]) -> gtk::DrawingArea {
    let slowdown = number(session, overrides, "animations.slowdown", 1.0).clamp(0.1, 10.0);
    let enabled = boolean(session, overrides, "animations.off", true);
    let phase = Rc::new(Cell::new(0.0f64));
    let area = gtk::DrawingArea::builder().content_height(120).build();
    let phase_tick = phase.clone();
    // A frame-clock tick stops when the page is not mapped, and it runs in
    // the frame's update phase instead of racing it from a timer.
    area.add_tick_callback(move |area, _clock| {
        if enabled {
            let step = 0.006 / slowdown;
            phase_tick.set((phase_tick.get() + step) % 1.0);
        }
        area.queue_draw();
        gtk::glib::ControlFlow::Continue
    });
    area.set_draw_func(move |_, cr, width, height| {
        let width = width as f64;
        let height = height as f64;
        cr.set_source_rgba(0.11, 0.11, 0.12, 1.0);
        cr.paint().ok();
        let travel = width - 120.0;
        let t = ease(phase.get());
        let x = 40.0 + travel * t;
        rounded_rect(cr, x, 28.0, 80.0, height - 56.0, 8.0);
        cr.set_source_rgba(0.30, 0.55, 0.85, 1.0);
        cr.fill().ok();
        cr.set_source_rgba(0.85, 0.90, 1.0, 0.8);
        cr.rectangle(x + 14.0, 44.0, 52.0, 4.0);
        cr.rectangle(x + 14.0, 56.0, 36.0, 4.0);
        cr.fill().ok();
        cr.set_source_rgba(0.45, 0.47, 0.50, 0.8);
        cr.set_line_width(2.0);
        cr.move_to(40.0, height - 18.0);
        cr.line_to(width - 40.0, height - 18.0);
        cr.stroke().ok();
    });
    area
}

fn ease(t: f64) -> f64 {
    // Smooth start and end.
    t * t * (3.0 - 2.0 * t)
}

fn number(session: &Session, overrides: &[(&str, Value)], key: &str, fallback: f64) -> f64 {
    read(session, overrides, key)
        .and_then(|value| value.as_f64())
        .unwrap_or(fallback)
}

fn boolean(session: &Session, overrides: &[(&str, Value)], key: &str, fallback: bool) -> bool {
    read(session, overrides, key)
        .and_then(|value| value.as_bool())
        .unwrap_or(fallback)
}

fn color(session: &Session, overrides: &[(&str, Value)], key: &str, fallback: &str) -> String {
    read(session, overrides, key)
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| fallback.to_string())
}

fn read(session: &Session, overrides: &[(&str, Value)], key: &str) -> Option<Value> {
    overrides
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value.clone())
        .or_else(|| session.get(key).map(|resolved| resolved.value))
}
