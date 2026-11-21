//! Shortcut writes and their confirmation dialogs.

use adw::prelude::*;
use relm4::prelude::*;

use niri_settings_core::lint;
use niri_settings_core::schemes;

use crate::app::{AppModel, Msg};

impl AppModel {
    pub(super) fn apply_scheme(&mut self, id: &'static str, sender: &ComponentSender<Self>) {
        let Some(scheme) = schemes::scheme(id) else {
            return;
        };
        let keys = scheme
            .binds
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>()
            .join(", ");
        let dialog = adw::AlertDialog::builder()
            .heading(format!("Apply the {} shortcuts?", scheme.label))
            .body(format!(
                "This changes {} shortcuts: {keys}. niri settings writes them to niri-settings-overrides.kdl, and niri uses them instead of the values in your file. You can undo this as one step.",
                scheme.binds.len()
            ))
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("apply", "Apply");
        dialog.set_default_response(Some("apply"));
        dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
        let sender = sender.clone();
        dialog.connect_response(None, move |dialog, response| {
            if response == "apply" {
                sender.input(Msg::SchemeConfirmed(id));
            }
            dialog.close();
        });
        dialog.present(Some(&self.root));
    }

    pub(super) fn scheme_confirmed(&mut self, id: &'static str, sender: &ComponentSender<Self>) {
        let Some(scheme) = schemes::scheme(id) else {
            return;
        };
        match self.session.apply_binds(
            scheme.binds,
            &format!("Applied the {} shortcuts", scheme.label),
        ) {
            Ok(applied) => {
                self.toast_undo(applied.summary, sender);
                self.refresh(sender);
            }
            Err(error) => self.show_error(format!("{error}")),
        }
    }

    pub(super) fn add_bind(&mut self, key: String, action: String, sender: &ComponentSender<Self>) {
        match self.session.add_bind(&key, &action) {
            Ok(applied) => {
                self.toast_undo(&applied.summary, sender);
                self.refresh(sender);
            }
            Err(error) => self.show_error(format!("{error}")),
        }
    }

    pub(super) fn replace_bind(
        &mut self,
        old: String,
        new: String,
        action: String,
        sender: &ComponentSender<Self>,
    ) {
        match self.session.replace_bind(&old, &new, &action) {
            Ok(applied) => {
                self.toast_undo(&applied.summary, sender);
                self.refresh(sender);
            }
            Err(error) => self.show_error(format!("{error}")),
        }
    }

    pub(super) fn remove_bind(&mut self, key: String, sender: &ComponentSender<Self>) {
        let binds = lint::collect_binds(&self.session.tree);
        let terminal: Vec<_> = binds
            .iter()
            .filter(|bind| lint::is_terminal(&bind.action, &bind.args))
            .collect();
        let removes_terminal = terminal.iter().any(|bind| bind.key == key);
        if removes_terminal && terminal.len() == 1 {
            let dialog = adw::AlertDialog::builder()
                .heading("Remove the only terminal shortcut?")
                .body(format!(
                    "{key} is the only shortcut that opens a terminal or launcher. Without it, a broken config is hard to repair from the desktop."
                ))
                .build();
            dialog.add_response("cancel", "Keep the shortcut");
            dialog.add_response("remove", "Remove anyway");
            dialog.set_default_response(Some("cancel"));
            dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
            let sender = sender.clone();
            let key = key.clone();
            dialog.connect_response(None, move |dialog, response| {
                if response == "remove" {
                    sender.input(Msg::RemoveBindConfirmed(key.clone()));
                }
                dialog.close();
            });
            dialog.present(Some(&self.root));
        } else {
            self.remove_bind_now(key, sender);
        }
    }

    pub(super) fn remove_bind_confirmed(&mut self, key: String, sender: &ComponentSender<Self>) {
        self.remove_bind_now(key, sender);
    }

    fn remove_bind_now(&mut self, key: String, sender: &ComponentSender<Self>) {
        match self.session.remove_bind(&key) {
            Ok(applied) => {
                self.toast_undo(&applied.summary, sender);
                self.refresh(sender);
            }
            Err(error) => self.show_error(format!("{error}")),
        }
    }
}
