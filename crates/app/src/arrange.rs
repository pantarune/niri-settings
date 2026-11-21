//! A small drag canvas for display arrangement.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use relm4::ComponentSender;

use niri_settings_core::Value;

use crate::app::{AppModel, Msg};
use crate::i18n;

#[derive(Debug, Clone)]
struct Rect {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    index: usize,
    origin_x: f64,
    origin_y: f64,
    scale: f64,
}

struct State {
    rects: Vec<Rect>,
    /// The position each display had when the canvas was built. Escape and
    /// no-op checks compare against this.
    start: Vec<(f64, f64)>,
    drag: Option<Drag>,
    hover: Option<usize>,
    selected: Option<usize>,
    focused: bool,
    scale: f64,
    pad_x: f64,
    pad_y: f64,
    min_x: f64,
    min_y: f64,
    width: f64,
    height: f64,
}

pub fn canvas(
    outputs: &[niri_ipc::Output],
    sender: &ComponentSender<AppModel>,
) -> gtk::DrawingArea {
    let rects: Vec<Rect> = outputs
        .iter()
        .filter_map(|output| {
            let logical = output.logical?;
            Some(Rect {
                name: output.name.clone(),
                x: logical.x as f64,
                y: logical.y as f64,
                width: logical.width as f64,
                height: logical.height as f64,
            })
        })
        .collect();

    let area = gtk::DrawingArea::builder().content_height(230).build();
    area.set_hexpand(true);
    area.set_focusable(true);
    area.update_property(&[
        gtk::accessible::Property::Label("Arrange displays"),
        gtk::accessible::Property::Description(
            "Drag a display to arrange it, or use the number keys to choose a display and the arrow keys to move it. Shift moves by one pixel. Press Enter to apply and Escape to cancel.",
        ),
    ]);

    let start: Vec<(f64, f64)> = rects.iter().map(|rect| (rect.x, rect.y)).collect();
    let state = Rc::new(RefCell::new(State {
        rects,
        start,
        drag: None,
        hover: None,
        selected: None,
        focused: false,
        scale: 1.0,
        pad_x: 24.0,
        pad_y: 24.0,
        min_x: 0.0,
        min_y: 0.0,
        width: 600.0,
        height: 230.0,
    }));

    {
        let state = state.clone();
        area.set_draw_func(move |_, cr, width, height| {
            let mut state = state.borrow_mut();
            state.width = width as f64;
            state.height = height as f64;
            draw(cr, &mut state);
        });
    }

    let gesture = gtk::GestureDrag::new();
    {
        let state = state.clone();
        let area2 = area.clone();
        gesture.connect_drag_begin(move |_, x, y| {
            let hit = {
                let mut state = state.borrow_mut();
                compute_layout(&mut state);
                state.rects.iter().position(|rect| {
                    let screen = screen_rect(rect, &state);
                    x >= screen.0
                        && x <= screen.0 + screen.2
                        && y >= screen.1
                        && y <= screen.1 + screen.3
                })
            };
            area2.grab_focus();
            let mut state = state.borrow_mut();
            state.selected = hit;
            state.drag = hit.map(|index| {
                let rect = &state.rects[index];
                Drag {
                    index,
                    origin_x: rect.x,
                    origin_y: rect.y,
                    scale: state.scale,
                }
            });
            area2.queue_draw();
        });
    }
    {
        let state = state.clone();
        let area2 = area.clone();
        gesture.connect_drag_update(move |_, offset_x, offset_y| {
            let mut state = state.borrow_mut();
            let Some(drag) = state.drag else {
                return;
            };
            let mut x = drag.origin_x + offset_x / drag.scale;
            let mut y = drag.origin_y + offset_y / drag.scale;
            let others: Vec<Rect> = state
                .rects
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != drag.index)
                .map(|(_, rect)| rect.clone())
                .collect();
            snap(&mut x, &mut y, &state.rects[drag.index], &others);
            state.rects[drag.index].x = x;
            state.rects[drag.index].y = y;
            area2.queue_draw();
        });
    }
    {
        let state = state.clone();
        let sender = sender.clone();
        gesture.connect_drag_end(move |_, _, _| {
            let mut state = state.borrow_mut();
            let Some(drag) = state.drag.take() else {
                return;
            };
            let rect = &state.rects[drag.index];
            if (rect.x - drag.origin_x).abs() < 1.0 && (rect.y - drag.origin_y).abs() < 1.0 {
                return;
            }
            let name = rect.name.clone();
            let value = format!("{},{}", rect.x.round(), rect.y.round());
            state.start[drag.index] = (rect.x, rect.y);
            drop(state);
            sender.input(Msg::DisplayChange {
                name,
                field: "position".to_string(),
                value: Value::String(value),
            });
        });
    }
    {
        let state = state.clone();
        let area2 = area.clone();
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(move |_, x, y| {
            let mut state = state.borrow_mut();
            compute_layout(&mut state);
            let hover = state.rects.iter().position(|rect| {
                let screen = screen_rect(rect, &state);
                x >= screen.0
                    && x <= screen.0 + screen.2
                    && y >= screen.1
                    && y <= screen.1 + screen.3
            });
            if state.hover != hover {
                state.hover = hover;
                area2.queue_draw();
            }
        });
        area.add_controller(motion);
    }
    {
        let enter_state = state.clone();
        let area2 = area.clone();
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(move |_| {
            let mut state = enter_state.borrow_mut();
            state.focused = true;
            if state.selected.is_none() && !state.rects.is_empty() {
                state.selected = Some(0);
            }
            area2.queue_draw();
        });
        let leave_state = state.clone();
        let area2 = area.clone();
        focus.connect_leave(move |_| {
            leave_state.borrow_mut().focused = false;
            area2.queue_draw();
        });
        area.add_controller(focus);
    }
    {
        let state = state.clone();
        let area2 = area.clone();
        let sender = sender.clone();
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let mut state = state.borrow_mut();
            if state.rects.is_empty() {
                return gtk::glib::Propagation::Proceed;
            }
            if let Some(digit) = key.to_unicode().and_then(|c| c.to_digit(10)) {
                let index = digit as usize;
                if index > 0 && index <= state.rects.len() {
                    state.selected = Some(index - 1);
                    area2.queue_draw();
                    return gtk::glib::Propagation::Stop;
                }
            }
            let index = state.selected.unwrap_or(0).min(state.rects.len() - 1);
            state.selected = Some(index);
            let step = if modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                1.0
            } else {
                10.0
            };
            if nudge(&mut state.rects[index], key, step) {
                area2.queue_draw();
                return gtk::glib::Propagation::Stop;
            }
            if key == gtk::gdk::Key::Escape {
                if let Some((x, y)) = state.start.get(index).copied() {
                    state.rects[index].x = x;
                    state.rects[index].y = y;
                    area2.queue_draw();
                }
                return gtk::glib::Propagation::Stop;
            }
            if key == gtk::gdk::Key::Return
                || key == gtk::gdk::Key::KP_Enter
                || key == gtk::gdk::Key::space
            {
                let (x, y) = {
                    let rect = &state.rects[index];
                    (rect.x, rect.y)
                };
                let Some((start_x, start_y)) = state.start.get(index).copied() else {
                    return gtk::glib::Propagation::Stop;
                };
                if (x - start_x).abs() < 0.5 && (y - start_y).abs() < 0.5 {
                    return gtk::glib::Propagation::Stop;
                }
                let name = state.rects[index].name.clone();
                let value = format!("{},{}", x.round(), y.round());
                state.start[index] = (x, y);
                drop(state);
                sender.input(Msg::DisplayChange {
                    name,
                    field: "position".to_string(),
                    value: Value::String(value),
                });
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        area.add_controller(keys);
    }
    area.add_controller(gesture);
    area
}

fn compute_layout(state: &mut State) {
    if state.rects.is_empty() {
        return;
    }
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for rect in &state.rects {
        min_x = min_x.min(rect.x);
        min_y = min_y.min(rect.y);
        max_x = max_x.max(rect.x + rect.width);
        max_y = max_y.max(rect.y + rect.height);
    }
    let content_width = (max_x - min_x).max(1.0);
    let content_height = (max_y - min_y).max(1.0);
    let available_w = (state.width - 48.0).max(80.0);
    let available_h = (state.height - 48.0).max(80.0);
    state.scale = (available_w / content_width)
        .min(available_h / content_height)
        .clamp(0.03, 1.0);
    let drawn_w = content_width * state.scale;
    let drawn_h = content_height * state.scale;
    state.pad_x = (state.width - drawn_w) / 2.0;
    state.pad_y = (state.height - drawn_h) / 2.0;
    state.min_x = min_x;
    state.min_y = min_y;
}

fn screen_rect(rect: &Rect, state: &State) -> (f64, f64, f64, f64) {
    (
        state.pad_x + (rect.x - state.min_x) * state.scale,
        state.pad_y + (rect.y - state.min_y) * state.scale,
        (rect.width * state.scale).max(24.0),
        (rect.height * state.scale).max(18.0),
    )
}

fn draw(cr: &gtk::cairo::Context, state: &mut State) {
    cr.set_source_rgba(0.10, 0.10, 0.11, 1.0);
    cr.paint().ok();
    compute_layout(state);
    if state.rects.is_empty() {
        cr.set_source_rgba(0.7, 0.7, 0.72, 0.8);
        cr.set_font_size(13.0);
        cr.move_to(24.0, 40.0);
        cr.show_text(i18n::tr("No display positions are available."))
            .ok();
        return;
    }
    for (index, rect) in state.rects.iter().enumerate() {
        let (x, y, width, height) = screen_rect(rect, state);
        rounded_rect(cr, x, y, width, height, 10.0);
        let active = state.drag.map(|drag| drag.index == index).unwrap_or(false)
            || state.hover == Some(index);
        if active {
            cr.set_source_rgba(0.22, 0.52, 0.88, 1.0);
        } else {
            cr.set_source_rgba(0.20, 0.21, 0.24, 1.0);
        }
        cr.fill_preserve().ok();
        cr.set_source_rgba(0.55, 0.65, 0.80, 0.9);
        cr.set_line_width(1.5);
        cr.stroke().ok();

        if state.focused && state.selected == Some(index) {
            rounded_rect(cr, x - 3.0, y - 3.0, width + 6.0, height + 6.0, 12.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
            cr.set_line_width(2.0);
            cr.set_dash(&[6.0, 4.0], 0.0);
            cr.stroke().ok();
            cr.set_dash(&[], 0.0);
        }

        cr.set_source_rgba(0.93, 0.94, 0.96, 0.95);
        cr.set_font_size(13.0);
        cr.move_to(x + 10.0, y + 22.0);
        let label = if state.rects.len() > 1 {
            format!("{}. {}", index + 1, rect.name)
        } else {
            rect.name.clone()
        };
        cr.show_text(&label).ok();
        cr.set_source_rgba(0.75, 0.78, 0.82, 0.85);
        cr.set_font_size(11.0);
        cr.move_to(x + 10.0, y + 38.0);
        cr.show_text(&format!("{} × {}", rect.width as i32, rect.height as i32))
            .ok();
        if state.drag.map(|drag| drag.index == index).unwrap_or(false)
            || (state.focused && state.selected == Some(index))
        {
            cr.set_source_rgba(0.85, 0.92, 1.0, 0.95);
            cr.set_font_size(11.0);
            cr.move_to(x + 10.0, y + 54.0);
            cr.show_text(&format!(
                "x={} y={}",
                rect.x.round() as i32,
                rect.y.round() as i32
            ))
            .ok();
        }
    }
    let hint = if state.drag.is_some() {
        i18n::tr("Release to apply. niri settings asks you to keep the change.")
    } else if state.focused {
        if state.rects.len() > 1 {
            i18n::tr("1–9 select · arrows move · Enter applies")
        } else {
            i18n::tr("Arrows move · Enter applies")
        }
    } else {
        i18n::tr("Drag a display. Edges snap. The change applies when you release.")
    };
    cr.set_source_rgba(0.65, 0.67, 0.70, 0.9);
    cr.set_font_size(11.0);
    cr.move_to(12.0, state.height - 10.0);
    cr.show_text(hint).ok();
}

fn nudge(rect: &mut Rect, key: gtk::gdk::Key, step: f64) -> bool {
    let (dx, dy) = match key {
        gtk::gdk::Key::Left => (-step, 0.0),
        gtk::gdk::Key::Right => (step, 0.0),
        gtk::gdk::Key::Up => (0.0, -step),
        gtk::gdk::Key::Down => (0.0, step),
        _ => return false,
    };
    rect.x += dx;
    rect.y += dy;
    true
}

fn snap(x: &mut f64, y: &mut f64, moving: &Rect, others: &[Rect]) {
    const THRESHOLD: f64 = 24.0;
    let mut best_x = None;
    let mut best_y = None;
    for other in others {
        for candidate in [
            other.x,
            other.x + other.width,
            other.x - moving.width,
            other.x + other.width - moving.width,
        ] {
            let distance = (*x - candidate).abs();
            if distance < THRESHOLD && best_x.map(|(_, d)| distance < d).unwrap_or(true) {
                best_x = Some((candidate, distance));
            }
        }
        for candidate in [
            other.y,
            other.y + other.height,
            other.y - moving.height,
            other.y + other.height - moving.height,
        ] {
            let distance = (*y - candidate).abs();
            if distance < THRESHOLD && best_y.map(|(_, d)| distance < d).unwrap_or(true) {
                best_y = Some((candidate, distance));
            }
        }
    }
    if let Some((value, _)) = best_x {
        *x = value;
    }
    if let Some((value, _)) = best_y {
        *y = value;
    }
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect {
            name: "HDMI-A-1".to_string(),
            x: 10.0,
            y: 20.0,
            width: 1920.0,
            height: 1080.0,
        }
    }

    #[test]
    fn arrow_keys_move_the_selected_display() {
        let mut rect = rect();
        assert!(nudge(&mut rect, gtk::gdk::Key::Right, 10.0));
        assert_eq!((rect.x, rect.y), (20.0, 20.0));
        assert!(nudge(&mut rect, gtk::gdk::Key::Up, 1.0));
        assert_eq!((rect.x, rect.y), (20.0, 19.0));
        assert!(!nudge(&mut rect, gtk::gdk::Key::Return, 10.0));
    }
}
