//! Window rule, startup, environment and workspace list writes.

use relm4::prelude::*;

use niri_settings_core::niri;
use niri_settings_core::session::{Applied, ApplyError};

use crate::app::{AppModel, Msg};

impl AppModel {
    pub(super) fn edit_rule(&mut self, index: usize, body: String, sender: &ComponentSender<Self>) {
        let result = self.session.edit_window_rule(index, &body);
        self.finish_list_write(result, sender);
    }

    pub(super) fn add_rule(&mut self, body: String, sender: &ComponentSender<Self>) {
        let result = self.session.add_window_rule(&body);
        self.finish_list_write(result, sender);
    }

    pub(super) fn remove_rule(&mut self, index: usize, sender: &ComponentSender<Self>) {
        let result = self.session.remove_window_rule(index);
        self.finish_list_write(result, sender);
    }

    pub(super) fn pick_window(sender: &ComponentSender<Self>) {
        let sender = sender.clone();
        std::thread::spawn(move || {
            if let Some(window) = niri::pick_window() {
                let app_id = window.app_id.unwrap_or_default();
                let title = window.title.unwrap_or_default();
                sender.input(Msg::AddRuleFor(app_id, title));
            }
        });
    }

    pub(super) fn open_add_startup(&self, sender: &ComponentSender<Self>) {
        self.show_prompt(
            sender,
            "Start a program with niri",
            "For example: waybar",
            Msg::AddStartup,
        );
    }

    pub(super) fn add_startup(&mut self, command: String, sender: &ComponentSender<Self>) {
        let result = self.session.add_startup(&command);
        self.finish_list_write(result, sender);
    }

    pub(super) fn remove_startup(&mut self, index: usize, sender: &ComponentSender<Self>) {
        let result = self.session.remove_startup(index);
        self.finish_list_write(result, sender);
    }

    pub(super) fn set_env(&mut self, name: String, value: String, sender: &ComponentSender<Self>) {
        let result = self.session.set_environment(&name, &value);
        self.finish_list_write(result, sender);
    }

    pub(super) fn remove_env(&mut self, name: String, sender: &ComponentSender<Self>) {
        let result = self.session.remove_environment(&name);
        self.finish_list_write(result, sender);
    }

    pub(super) fn add_workspace(
        &mut self,
        name: String,
        output: String,
        sender: &ComponentSender<Self>,
    ) {
        let result = self.session.add_workspace(&name, &output);
        self.finish_list_write(result, sender);
    }

    pub(super) fn remove_workspace(&mut self, name: String, sender: &ComponentSender<Self>) {
        let result = self.session.remove_workspace(&name);
        self.finish_list_write(result, sender);
    }

    fn finish_list_write(
        &mut self,
        result: Result<Applied, ApplyError>,
        sender: &ComponentSender<Self>,
    ) {
        match result {
            Ok(applied) => {
                self.toast_undo(&applied.summary, sender);
                self.refresh(sender);
            }
            Err(error) => self.show_error(format!("{error}")),
        }
    }
}
