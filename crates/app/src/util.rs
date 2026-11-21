//! Small helpers shared by the app: colors, preferences and dialogs.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::prelude::*;
use serde::{Deserialize, Serialize};

/// User preferences that are not part of the niri config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub show_config_names: bool,
    #[serde(default = "seen_before_welcome")]
    pub welcome_seen: bool,
}

/// A preferences file without the field belongs to a copy that ran before the
/// welcome flow existed. Do not show onboarding to that user again.
fn seen_before_welcome() -> bool {
    true
}

impl Prefs {
    pub fn path() -> PathBuf {
        let state = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        state.join("niri-settings/preferences.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }
}

/// Parse `#rrggbb`, `#rrggbbaa` or `transparent`.
pub fn hex_to_rgba(hex: &str) -> gtk::gdk::RGBA {
    let hex = hex.trim();
    if hex.eq_ignore_ascii_case("transparent") {
        return gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0);
    }
    let value = hex.trim_start_matches('#');
    let parse = |text: &str| u8::from_str_radix(text, 16).unwrap_or(0);
    let (r, g, b, a) = match value.len() {
        3 => {
            let r = parse(&value[0..1]);
            let g = parse(&value[1..2]);
            let b = parse(&value[2..3]);
            (r, g, b, 255)
        }
        6 => (
            parse(&value[0..2]),
            parse(&value[2..4]),
            parse(&value[4..6]),
            255,
        ),
        8 => (
            parse(&value[0..2]),
            parse(&value[2..4]),
            parse(&value[4..6]),
            parse(&value[6..8]),
        ),
        _ => (0, 0, 0, 255),
    };
    gtk::gdk::RGBA::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    )
}

/// Write a color as `#rrggbb` or `#rrggbbaa`.
pub fn rgba_to_hex(rgba: &gtk::gdk::RGBA) -> String {
    let r = (rgba.red() * 255.0).round() as u8;
    let g = (rgba.green() * 255.0).round() as u8;
    let b = (rgba.blue() * 255.0).round() as u8;
    let a = (rgba.alpha() * 255.0).round() as u8;
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// A compact age string for history rows.
pub fn format_age(seconds: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(seconds);
    let delta = now.saturating_sub(seconds);
    if delta < 60 {
        "now".to_string()
    } else if delta < 3600 {
        format!("{} min ago", delta / 60)
    } else if delta < 86_400 {
        format!("{} h ago", delta / 3600)
    } else {
        format!("{} d ago", delta / 86_400)
    }
}

/// Send a message once after the user stops moving a control.
pub struct Debounce {
    generation: Rc<Cell<u64>>,
}

impl Debounce {
    pub fn new() -> Self {
        Self {
            generation: Rc::new(Cell::new(0)),
        }
    }

    pub fn schedule<F>(&self, milliseconds: u64, action: F)
    where
        F: FnOnce() + 'static,
    {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let shared = self.generation.clone();
        let mut action = Some(action);
        gtk::glib::timeout_add_local(Duration::from_millis(milliseconds), move || {
            if shared.get() == generation
                && let Some(action) = action.take()
            {
                action();
            }
            gtk::glib::ControlFlow::Break
        });
    }
}

/// An icon button with an accessible name.
pub fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button
}

/// A preferences group with a title.
pub fn group(title: &str) -> adw::PreferencesGroup {
    adw::PreferencesGroup::builder().title(title).build()
}

/// A page body with a sensible reading width.
pub fn page_body() -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content
}

/// Wrap page content in a scrolled area with a reading-width clamp.
pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::builder()
        .maximum_size(900)
        .tightening_threshold(600)
        .child(child)
        .build();

    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build()
}

/// Escape text for Pango markup.
pub fn escape(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_names_are_off_for_a_new_install() {
        assert!(!Prefs::default().show_config_names);
        assert!(!Prefs::default().welcome_seen);
    }

    #[test]
    fn an_old_preferences_file_still_loads() {
        let prefs: Prefs = serde_json::from_str(r#"{"show_config_names": true}"#).unwrap();
        assert!(prefs.show_config_names);
        assert!(
            prefs.welcome_seen,
            "an old prefs file means the app ran before"
        );
    }
}
