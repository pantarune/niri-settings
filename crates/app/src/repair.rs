//! A small editor for a config that niri settings cannot load.
//!
//! The main window needs a parsed config. When the file is broken, this window
//! opens instead. The user can repair the text and save it after a validation.

use std::fs;
use std::path::Path;

use adw::prelude::*;
use gtk::gio::prelude::ApplicationExtManual;

use crate::util;
use niri_settings_core::session::{validate_text, write_text_atomic};

/// A window for when there is no config to open. It explains why and does
/// not offer an editor: saving one would create a config that niri then
/// prefers over any other.
pub fn run_missing(message: &str) -> i32 {
    run_notice("No config to open", "dialog-information-symbolic", message)
}

/// A status window for a failure that stops the app before the main window
/// can open, such as niri missing from PATH.
pub fn run_notice(title: &str, icon: &str, message: &str) -> i32 {
    let _ = adw::init();
    let app = adw::Application::builder()
        .application_id("io.github.niri_settings.NiriSettings.Missing")
        .build();
    let title = title.to_string();
    let icon = icon.to_string();
    let message = message.to_string();
    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("niri settings")
            .default_width(520)
            .default_height(420)
            .build();
        let status = adw::StatusPage::builder()
            .icon_name(&icon)
            .title(&title)
            .description(util::escape(&message))
            .build();
        let close = gtk::Button::with_label("Close");
        close.add_css_class("pill");
        close.set_halign(gtk::Align::Center);
        let window_close = window.clone();
        close.connect_clicked(move |_| window_close.close());
        status.set_child(Some(&close));
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&status));
        window.set_content(Some(&toolbar));
        window.present();
    });
    i32::from(app.run_with_args(&["niri-settings-app"]).get())
}

pub fn run(path: &Path, error: &str) -> i32 {
    let _ = adw::init();
    let app = adw::Application::builder()
        .application_id("io.github.niri_settings.NiriSettings.Repair")
        .build();
    let path = path.to_path_buf();
    let error = error.to_string();
    app.connect_activate(move |app| {
        present(app, &path, &error);
    });
    i32::from(app.run_with_args(&["niri-settings-app"]).get())
}

fn present(app: &adw::Application, path: &Path, error: &str) {
    let target = if path.exists() {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let text = fs::read_to_string(&target).unwrap_or_default();
    let original = text.clone();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Repair the niri config")
        .default_width(860)
        .default_height(640)
        .build();
    let header = adw::HeaderBar::new();

    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_top(18);
    body.set_margin_bottom(18);
    body.set_margin_start(18);
    body.set_margin_end(18);

    let heading = gtk::Label::new(Some("The config has an error"));
    heading.add_css_class("title-2");
    heading.set_xalign(0.0);
    body.append(&heading);
    let message = gtk::Label::new(Some(error));
    message.set_wrap(true);
    message.set_xalign(0.0);
    message.add_css_class("dim-label");
    body.append(&message);

    let view = gtk::TextView::new();
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::None);
    view.set_top_margin(10);
    view.set_bottom_margin(10);
    view.set_left_margin(10);
    view.set_right_margin(10);
    let buffer = view.buffer();
    buffer.set_text(&text);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .vexpand(true)
        .build();
    scroll.add_css_class("card");
    body.append(&scroll);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let save = gtk::Button::with_label("Check and save");
    save.add_css_class("suggested-action");
    let status = gtk::Label::new(Some(
        "niri checks the text first. The app replaces the file only after the check passes.",
    ));
    status.set_wrap(true);
    status.set_xalign(0.0);
    status.set_hexpand(true);
    status.add_css_class("dim-label");
    actions.append(&save);
    actions.append(&status);
    body.append(&actions);

    {
        let target = target.clone();
        let buffer = buffer.clone();
        let status = status.clone();
        let original = original.clone();
        save.connect_clicked(move |_| {
            let start = buffer.start_iter();
            let end = buffer.end_iter();
            let text = buffer.text(&start, &end, true).to_string();
            if target.exists() {
                let current = fs::read_to_string(&target).ok();
                if current.as_deref() != Some(original.as_str()) {
                    status.set_label(
                        "The file changed on disk. Close this window and open it again.",
                    );
                    status.add_css_class("error");
                    return;
                }
            }
            match validate_text(&target, &text) {
                Ok(()) => match write_text_atomic(&target, &text) {
                    Ok(()) => {
                        status.set_label(&format!(
                            "Saved {}. Close this window and start niri-settings-app again.",
                            target.display()
                        ));
                        status.remove_css_class("dim-label");
                        status.add_css_class("success");
                    }
                    Err(error) => {
                        status.set_label(&format!("Cannot write the file: {error}"));
                        status.add_css_class("error");
                    }
                },
                Err(error) => {
                    status.set_label(&error);
                    status.add_css_class("error");
                }
            }
        });
    }

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&body));
    window.set_content(Some(&toolbar_view));
    window.present();
}
