//! Display changes and the pending-output marker.
//!
//! A display change applies immediately and asks for confirmation. The marker
//! lets the next start restore the previous setup if the app is killed.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use relm4::prelude::*;

use niri_settings_core::Value;
use niri_settings_core::niri;

use super::transform_name;
use crate::app::{AppModel, Msg};

impl AppModel {
    pub(super) fn display_change(
        &mut self,
        name: String,
        field: String,
        value: Value,
        sender: &ComponentSender<Self>,
    ) {
        if self.display_guard.get() {
            self.toast("Confirm the display change first.");
            return;
        }
        let outputs = niri::outputs();
        let Some(output) = outputs.iter().find(|output| output.name == name) else {
            self.show_error("That display is no longer connected.");
            return;
        };
        let restore = restore_commands(output);
        let args = display_args(&field, &value);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        write_output_marker(&restore);
        if let Err(error) = niri::output(&name, &refs) {
            remove_output_marker();
            self.show_error(format!("niri rejected the display change: {error}"));
            self.refresh(sender);
            return;
        }
        self.display_guard.set(true);

        let dialog = adw::AlertDialog::builder()
            .heading("Keep these display settings?")
            .body("If the screen stays dark or unreadable, wait 15 seconds or press Revert.")
            .build();
        dialog.add_response("revert", "Revert");
        dialog.add_response("keep", "Keep changes");
        dialog.set_response_appearance("revert", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("keep", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("keep"));

        let done = Rc::new(Cell::new(false));
        let done_response = done.clone();
        let guard = self.display_guard.clone();
        let sender_response = sender.clone();
        let name_response = name.clone();
        let field_response = field.clone();
        let value_response = value.clone();
        let restore_response = restore.clone();
        let _dialog_response = dialog.clone();
        dialog.connect_response(None, move |dialog, response| {
            if done_response.replace(true) {
                return;
            }
            dialog.close();
            guard.set(false);
            if response == "keep" {
                sender_response.input(Msg::DisplayKeep {
                    name: name_response.clone(),
                    field: field_response.clone(),
                    value: value_response.clone(),
                });
            } else {
                apply_restore(&restore_response);
                sender_response.input(Msg::DisplayReverted);
                sender_response.input(Msg::Toast("Reverted the display change".to_string()));
            }
        });

        let done_closed = done.clone();
        let guard_closed = self.display_guard.clone();
        let restore_closed = restore.clone();
        let sender_closed = sender.clone();
        dialog.connect_closed(move |_| {
            if !done_closed.replace(true) {
                guard_closed.set(false);
                remove_output_marker();
                apply_restore(&restore_closed);
                sender_closed.input(Msg::DisplayReverted);
                sender_closed.input(Msg::Toast("Reverted the display change".to_string()));
            }
        });

        let done_timeout = done.clone();
        let guard_timeout = self.display_guard.clone();
        let sender_timeout = sender.clone();
        let restore_timeout = restore.clone();
        let dialog_timeout = dialog.clone();
        gtk::glib::timeout_add_seconds_local(15, move || {
            if !done_timeout.replace(true) {
                dialog_timeout.close();
                guard_timeout.set(false);
                remove_output_marker();
                apply_restore(&restore_timeout);
                sender_timeout.input(Msg::DisplayReverted);
                sender_timeout.input(Msg::Toast("Reverted the display change".to_string()));
            }
            gtk::glib::ControlFlow::Break
        });
        dialog.present(Some(&self.root));
    }

    pub(super) fn display_keep(&mut self, name: String, field: String, value: Value) {
        match self.session.set_output(&name, &field, value) {
            Ok(applied) => {
                remove_output_marker();
                if let Err(error) = self.session.reload() {
                    self.show_error(format!("{error}"));
                }
                self.toast(applied.summary);
            }
            Err(niri_settings_core::ApplyError::NoChange) => {
                remove_output_marker();
                self.toast("Display settings kept");
            }
            Err(error) => {
                remove_output_marker();
                self.show_error(format!("{error}"));
                niri::reload_config();
            }
        }
    }
}

fn display_args(field: &str, value: &Value) -> Vec<String> {
    match field {
        "off" => {
            if value.as_bool().unwrap_or(false) {
                vec!["off".to_string()]
            } else {
                vec!["on".to_string()]
            }
        }
        "vrr" => match value.as_str().unwrap_or("off") {
            "on-demand" => vec![
                "vrr".to_string(),
                "on".to_string(),
                "--on-demand".to_string(),
            ],
            other => vec!["vrr".to_string(), other.to_string()],
        },
        "position" => {
            let text = value.as_str().unwrap_or("0,0");
            let mut parts = text.split(',');
            vec![
                "position".to_string(),
                parts.next().unwrap_or("0").trim().to_string(),
                parts.next().unwrap_or("0").trim().to_string(),
            ]
        }
        "mode" | "scale" | "transform" => {
            vec![field.to_string(), value.to_string()]
        }
        _ => Vec::new(),
    }
}

fn restore_commands(output: &niri_ipc::Output) -> Vec<Vec<String>> {
    let name = output.name.clone();
    let mut commands = Vec::new();
    if let Some((width, height, refresh)) = niri::current_mode(output) {
        commands.push(vec![
            name.clone(),
            "mode".to_string(),
            format!("{width}x{height}@{:.3}", refresh as f64 / 1000.0),
        ]);
    }
    if let Some(logical) = output.logical {
        commands.push(vec![
            name.clone(),
            "scale".to_string(),
            logical.scale.to_string(),
        ]);
        commands.push(vec![
            name.clone(),
            "transform".to_string(),
            transform_name(logical.transform).to_string(),
        ]);
    }
    commands.push(vec![
        name,
        "vrr".to_string(),
        if output.vrr_enabled { "on" } else { "off" }.to_string(),
    ]);
    commands
}

fn apply_restore(commands: &[Vec<String>]) {
    for command in commands {
        if let Some((name, args)) = command.split_first() {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let _ = niri::output(name, &refs);
        }
    }
}

fn marker_path() -> std::path::PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/state"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    state.join("niri-settings/pending-output.json")
}

fn write_output_marker(commands: &[Vec<String>]) {
    let path = marker_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(commands) {
        let _ = std::fs::write(path, text);
    }
}

fn remove_output_marker() {
    let _ = std::fs::remove_file(marker_path());
}

pub(super) fn recover_pending_output() -> Option<String> {
    let text = std::fs::read_to_string(marker_path()).ok()?;
    let commands: Vec<Vec<String>> = serde_json::from_str(&text).ok()?;
    let _ = std::fs::remove_file(marker_path());
    for command in &commands {
        let (name, args) = command.split_first()?;
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let _ = niri::output(name, &refs);
    }
    Some("Restored the display settings from the last session".to_string())
}
