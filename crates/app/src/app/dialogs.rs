//! Dialogs, prompts and their small parsing helpers.
//!
//! These are method bodies on `AppModel`, kept apart from the shell so the
//! app module stays about wiring and state.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use relm4::prelude::*;

use niri_settings_core::Value;
use niri_settings_core::import::{self, BindChange, ImportResult};
use niri_settings_core::lint;
use niri_settings_core::session::Session;

use crate::app::{AppModel, Msg};
use crate::util;

impl AppModel {
    pub(super) fn show_history(&self, sender: &ComponentSender<Self>) {
        let window = gtk::Window::builder()
            .title(crate::i18n::tr("History"))
            .transient_for(&self.root)
            .modal(true)
            .default_width(620)
            .default_height(560)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        toolbar.add_top_bar(&header);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        list.set_margin_all(18);
        for entry in self.session.history().iter().rev().take(100) {
            let row = adw::ActionRow::builder()
                .title(entry.summary.clone())
                .subtitle(util::format_age(entry.time))
                .build();
            let restore = gtk::Button::with_label("Restore");
            restore.set_valign(gtk::Align::Center);
            let sender = sender.clone();
            let id = entry.id;
            let window2 = window.clone();
            restore.connect_clicked(move |_| {
                sender.input(Msg::Restore(id));
                window2.close();
            });
            row.add_suffix(&restore);
            list.append(&row);
        }
        if self.session.history().is_empty() {
            let label = gtk::Label::new(Some("No changes yet."));
            label.add_css_class("dim-label");
            label.set_margin_all(24);
            list.append(&label);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .build();
        toolbar.set_content(Some(&scroll));
        window.set_child(Some(&toolbar));
        window.present();
    }

    pub(super) fn show_preferences(&self, sender: &ComponentSender<Self>) {
        let window = gtk::Window::builder()
            .title(crate::i18n::tr("Preferences"))
            .transient_for(&self.root)
            .modal(true)
            .default_width(520)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        let group = util::group("Advanced");
        let names = adw::ActionRow::builder()
            .title("Show config names")
            .subtitle("Show the niri option name under each setting")
            .build();
        let names_switch = gtk::Switch::builder()
            .active(self.prefs.show_config_names)
            .tooltip_text("Show config names")
            .valign(gtk::Align::Center)
            .build();
        let sender2 = sender.clone();
        names_switch.connect_state_set(move |_, state| {
            sender2.input(Msg::ToggleNames(state));
            gtk::glib::Propagation::Proceed
        });
        names.add_suffix(&names_switch);
        group.add(&names);
        let body = util::page_body();
        body.append(&group);
        let scroll = util::scrolled(&body);
        toolbar.set_content(Some(&scroll));
        window.set_child(Some(&toolbar));
        window.present();
    }

    pub(super) fn close_welcome(&mut self, sender: &ComponentSender<Self>) {
        self.welcome.close();
        self.prefs.welcome_seen = true;
        self.prefs.save();
        self.refresh(sender);
    }

    pub(super) fn welcome_apply(&mut self, sender: &ComponentSender<Self>) {
        let result = ImportResult {
            changes: self.welcome.changes(&self.session),
            binds: self
                .welcome
                .binds()
                .iter()
                .map(|(key, action)| BindChange {
                    key: (*key).to_string(),
                    action: (*action).to_string(),
                })
                .collect(),
            ..ImportResult::default()
        };
        let empty = result.is_empty();
        let mut wrote = false;
        if !empty {
            match self.session.apply_import(&result) {
                Ok(mut applied) => {
                    applied.summary = "Welcome setup".to_string();
                    self.toast_undo(&applied.summary, sender);
                    wrote = true;
                }
                Err(niri_settings_core::ApplyError::NoChange) => {}
                Err(error) => {
                    self.show_error(format!("{error}"));
                    return;
                }
            }
        }
        self.welcome.close();
        self.prefs.welcome_seen = true;
        self.prefs.save();
        self.refresh(sender);
        if !wrote {
            self.toast("Setup finished");
        }
    }

    pub(super) fn show_add_bind(&self, sender: &ComponentSender<Self>, editing: Option<String>) {
        let window = gtk::Window::builder()
            .title(if editing.is_some() {
                "Change shortcut"
            } else {
                "Add shortcut"
            })
            .transient_for(&self.root)
            .modal(true)
            .default_width(520)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let add = gtk::Button::with_label("Save");
        add.add_css_class("suggested-action");
        header.pack_end(&add);
        if let Some(key) = &editing {
            let remove = gtk::Button::with_label("Remove");
            remove.add_css_class("destructive-action");
            header.pack_start(&remove);
            let sender = sender.clone();
            let key = key.clone();
            let window = window.clone();
            remove.connect_clicked(move |_| {
                sender.input(Msg::RemoveBind(key.clone()));
                window.close();
            });
        }
        toolbar.add_top_bar(&header);

        let body = util::page_body();
        let group = util::group("Shortcut");
        let key_row = adw::ActionRow::builder()
            .title("Keys")
            .subtitle("Click Record, then press the combination. Escape cancels.")
            .build();
        let record = gtk::Button::with_label("Record");
        record.set_valign(gtk::Align::Center);
        let key_label = gtk::Label::new(Some(editing.as_deref().unwrap_or("Not recorded")));
        key_label.add_css_class("monospace");
        key_label.set_valign(gtk::Align::Center);
        key_row.add_suffix(&key_label);
        key_row.add_suffix(&record);
        group.add(&key_row);

        let action_row = adw::ActionRow::builder()
            .title("Action")
            .subtitle("For example: spawn \"foot\" or close-window")
            .build();
        let current_action = editing
            .as_ref()
            .and_then(|key| {
                lint::collect_binds(&self.session.tree)
                    .into_iter()
                    .find(|bind| bind.key == *key)
                    .map(|bind| {
                        if bind.args.is_empty() {
                            bind.action
                        } else {
                            let args = bind
                                .args
                                .iter()
                                .map(|arg| Value::from(arg.as_str()).literal())
                                .collect::<Vec<_>>()
                                .join(" ");
                            format!("{} {args}", bind.action)
                        }
                    })
            })
            .unwrap_or_default();
        let action_entry = gtk::Entry::builder()
            .placeholder_text("spawn \"foot\"")
            .width_chars(24)
            .text(current_action)
            .valign(gtk::Align::Center)
            .build();
        action_row.add_suffix(&action_entry);
        group.add(&action_row);
        body.append(&group);
        let note = gtk::Label::new(Some(
            "Press Record, then press the keys. The Shortcuts page reports combinations that are already in use.",
        ));
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.add_css_class("dim-label");
        body.append(&note);
        toolbar.set_content(Some(&util::scrolled(&body)));
        window.set_child(Some(&toolbar));

        let recorded = Rc::new(std::cell::RefCell::new(editing.clone().unwrap_or_default()));
        let recording = Rc::new(Cell::new(false));
        {
            let recorded = recorded.clone();
            let recording = recording.clone();
            let key_label = key_label.clone();
            let controller = gtk::EventControllerKey::new();
            controller.set_propagation_phase(gtk::PropagationPhase::Capture);
            let label = key_label.clone();
            let shared = recorded.clone();
            controller.connect_key_pressed(move |_, key, _, state| {
                if !recording.get() {
                    return gtk::glib::Propagation::Proceed;
                }
                if key == gtk::gdk::Key::Shift_L
                    || key == gtk::gdk::Key::Shift_R
                    || key == gtk::gdk::Key::Control_L
                    || key == gtk::gdk::Key::Control_R
                    || key == gtk::gdk::Key::Alt_L
                    || key == gtk::gdk::Key::Alt_R
                    || key == gtk::gdk::Key::Super_L
                    || key == gtk::gdk::Key::Super_R
                    || key == gtk::gdk::Key::Meta_L
                    || key == gtk::gdk::Key::Meta_R
                {
                    return gtk::glib::Propagation::Proceed;
                }
                let chords = gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::SUPER_MASK
                    | gtk::gdk::ModifierType::META_MASK
                    | gtk::gdk::ModifierType::HYPER_MASK;
                let plain = !state.intersects(chords);
                if key == gtk::gdk::Key::Escape && plain {
                    recording.set(false);
                    restore_key_label(&label, &shared);
                    return gtk::glib::Propagation::Stop;
                }
                // A plain Tab leaves the recorder. Capturing it would trap
                // keyboard users in a hidden mode with no way out.
                if key == gtk::gdk::Key::Tab && plain {
                    recording.set(false);
                    restore_key_label(&label, &shared);
                    return gtk::glib::Propagation::Proceed;
                }
                let text = key_to_niri(key, state);
                if !text.is_empty() {
                    *shared.borrow_mut() = text.clone();
                    label.set_label(&text);
                    recording.set(false);
                }
                gtk::glib::Propagation::Stop
            });
            record.add_controller(controller);
        }
        {
            let key_label = key_label.clone();
            let recording = recording.clone();
            record.connect_clicked(move |button| {
                recording.set(true);
                key_label.set_label("Press the keys now…");
                button.grab_focus();
            });
        }
        {
            let sender = sender.clone();
            let window = window.clone();
            let key_label = key_label.clone();
            let action_entry = action_entry.clone();
            let recorded = recorded.clone();
            let editing = editing.clone();
            add.connect_clicked(move |_| {
                let key = recorded.borrow().clone();
                let action = action_entry.text().to_string();
                if key.is_empty() || action.trim().is_empty() {
                    key_label.set_label("Record the keys first");
                    return;
                }
                if let Some(old) = &editing {
                    sender.input(Msg::ReplaceBind(old.clone(), key.clone(), action.clone()));
                } else {
                    sender.input(Msg::AddBind(key.clone(), action.clone()));
                }
                window.close();
            });
        }
        window.present();
    }

    pub(super) fn show_edit_rule(&self, sender: &ComponentSender<Self>, index: usize) {
        let Some(fields) = rule_fields(&self.session, index) else {
            self.show_error("That rule no longer exists.");
            return;
        };
        self.show_rule_dialog(sender, Some((index, fields)));
    }

    pub(super) fn show_add_rule(&self, sender: &ComponentSender<Self>, app_id: Option<String>) {
        self.show_rule_dialog(
            sender,
            Some((
                usize::MAX,
                RuleFields {
                    app_id: app_id.unwrap_or_default(),
                    ..RuleFields::default()
                },
            )),
        );
    }

    fn show_rule_dialog(&self, sender: &ComponentSender<Self>, edit: Option<(usize, RuleFields)>) {
        let fields = edit
            .as_ref()
            .map(|(_, fields)| fields.clone())
            .unwrap_or_default();
        let editing = edit
            .as_ref()
            .map(|(index, _)| *index)
            .filter(|index| *index != usize::MAX);
        let window = gtk::Window::builder()
            .title(if editing.is_some() {
                "Edit app rule"
            } else {
                "App rule"
            })
            .transient_for(&self.root)
            .modal(true)
            .default_width(560)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let add = gtk::Button::with_label("Save");
        add.add_css_class("suggested-action");
        header.pack_end(&add);
        toolbar.add_top_bar(&header);
        let body = util::page_body();
        let group = util::group("When this app opens");
        let id_row = adw::ActionRow::builder()
            .title("App ID")
            .subtitle("Exact match. The app ID is shown on the window.")
            .build();
        let id_entry = gtk::Entry::builder()
            .text(fields.app_id.clone())
            .placeholder_text("firefox")
            .width_chars(22)
            .valign(gtk::Align::Center)
            .build();
        id_row.add_suffix(&id_entry);
        group.add(&id_row);

        let title_row = adw::ActionRow::builder()
            .title("Title contains")
            .subtitle("Optional. Leave empty to match every window of the app.")
            .build();
        let title_entry = gtk::Entry::builder()
            .text(fields.title.clone())
            .placeholder_text("Optional")
            .width_chars(22)
            .valign(gtk::Align::Center)
            .build();
        title_row.add_suffix(&title_entry);
        group.add(&title_row);

        let float_row = adw::ActionRow::builder().title("Float the window").build();
        let float_switch = gtk::Switch::builder()
            .active(fields.floating)
            .tooltip_text("Float the window")
            .valign(gtk::Align::Center)
            .build();
        float_row.add_suffix(&float_switch);
        group.add(&float_row);

        let size_row = adw::ActionRow::builder()
            .title("Size")
            .subtitle("Width × height in pixels. 0 keeps the app's own size.")
            .build();
        let width = gtk::SpinButton::with_range(0.0, 10_000.0, 10.0);
        width.set_width_chars(6);
        width.set_value(fields.width);
        let height = gtk::SpinButton::with_range(0.0, 10_000.0, 10.0);
        height.set_width_chars(6);
        height.set_value(fields.height);
        let size_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        size_box.append(&width);
        size_box.append(&gtk::Label::new(Some("×")));
        size_box.append(&height);
        size_row.add_suffix(&size_box);
        group.add(&size_row);

        let corner_row = adw::ActionRow::builder()
            .title("Corner radius")
            .subtitle("Pixels. 0 keeps square corners.")
            .build();
        let corner = gtk::SpinButton::with_range(0.0, 128.0, 1.0);
        corner.set_width_chars(5);
        corner.set_value(fields.corner);
        corner_row.add_suffix(&corner);
        group.add(&corner_row);

        let opacity_row = adw::ActionRow::builder()
            .title("Opacity")
            .subtitle("Percent. 100 is fully opaque.")
            .build();
        let opacity = gtk::SpinButton::with_range(1.0, 100.0, 1.0);
        opacity.set_width_chars(5);
        opacity.set_value(fields.opacity);
        opacity_row.add_suffix(&opacity);
        group.add(&opacity_row);

        let workspace_row = adw::ActionRow::builder()
            .title("Open on workspace")
            .subtitle("Optional. The workspace name.")
            .build();
        let workspace = gtk::Entry::builder()
            .text(fields.workspace.clone())
            .placeholder_text("Optional")
            .width_chars(18)
            .valign(gtk::Align::Center)
            .build();
        workspace_row.add_suffix(&workspace);
        group.add(&workspace_row);
        body.append(&group);
        toolbar.set_content(Some(&util::scrolled(&body)));
        window.set_child(Some(&toolbar));

        let sender = sender.clone();
        let window2 = window.clone();
        let edit = editing;
        add.connect_clicked(move |_| {
            let app = id_entry.text().to_string();
            if app.trim().is_empty() {
                return;
            }
            let mut body = String::new();
            body.push_str(&format!("match app-id={}\n", quote(&app_id_pattern(&app))));
            let title = title_entry.text().to_string();
            if !title.trim().is_empty() {
                body.push_str(&format!("match title={}\n", quote(&title_pattern(&title))));
            }
            if float_switch.is_active() {
                body.push_str("open-floating true\n");
            }
            if width.value() > 0.0 && height.value() > 0.0 {
                body.push_str(&format!(
                    "default-column-width {{ fixed {}; }}\ndefault-window-height {{ fixed {}; }}\n",
                    width.value() as i64,
                    height.value() as i64
                ));
            }
            if corner.value() > 0.0 {
                body.push_str(&format!(
                    "geometry-corner-radius {}\nclip-to-geometry true\n",
                    corner.value()
                ));
            }
            if opacity.value() < 100.0 {
                body.push_str(&format!("opacity {}\n", opacity.value() / 100.0));
            }
            if !workspace.text().trim().is_empty() {
                body.push_str(&format!(
                    "open-on-workspace {}\n",
                    quote(workspace.text().trim())
                ));
            }
            if let Some(index) = edit {
                sender.input(Msg::EditRule(index, body));
            } else {
                sender.input(Msg::AddRule(body));
            }
            window2.close();
        });
        window.present();
    }

    pub(super) fn show_prompt(
        &self,
        sender: &ComponentSender<Self>,
        title: &str,
        placeholder: &str,
        msg: impl Fn(String) -> Msg + 'static,
    ) {
        let window = gtk::Window::builder()
            .title(title)
            .transient_for(&self.root)
            .modal(true)
            .default_width(440)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let add = gtk::Button::with_label("Add");
        add.add_css_class("suggested-action");
        header.pack_end(&add);
        toolbar.add_top_bar(&header);
        let body = util::page_body();
        let entry = gtk::Entry::builder().placeholder_text(placeholder).build();
        body.append(&entry);
        toolbar.set_content(Some(&util::scrolled(&body)));
        window.set_child(Some(&toolbar));
        let msg = Rc::new(msg);
        let sender1 = sender.clone();
        let window2 = window.clone();
        let msg2 = msg.clone();
        entry.clone().connect_activate(move |entry| {
            let text = entry.text().to_string();
            if !text.trim().is_empty() {
                sender1.input(msg2(text));
                window2.close();
            }
        });
        let sender2 = sender.clone();
        let window3 = window.clone();
        let entry2 = entry.clone();
        add.connect_clicked(move |_| {
            let text = entry2.text().to_string();
            if !text.trim().is_empty() {
                sender2.input(msg(text));
                window3.close();
            }
        });
        window.present();
        entry.grab_focus();
    }

    pub(super) fn show_env_prompt(&self, sender: &ComponentSender<Self>) {
        let window = gtk::Window::builder()
            .title("Environment variable")
            .transient_for(&self.root)
            .modal(true)
            .default_width(480)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let add = gtk::Button::with_label("Add");
        add.add_css_class("suggested-action");
        header.pack_end(&add);
        toolbar.add_top_bar(&header);
        let body = util::page_body();
        let group = util::group("Variable");
        let name_row = adw::ActionRow::builder().title("Name").build();
        let name = gtk::Entry::builder()
            .placeholder_text("QT_QPA_PLATFORMTHEME")
            .valign(gtk::Align::Center)
            .build();
        name_row.add_suffix(&name);
        group.add(&name_row);
        let value_row = adw::ActionRow::builder().title("Value").build();
        let value = gtk::Entry::builder()
            .placeholder_text("qt6ct")
            .valign(gtk::Align::Center)
            .build();
        value_row.add_suffix(&value);
        group.add(&value_row);
        body.append(&group);
        toolbar.set_content(Some(&util::scrolled(&body)));
        window.set_child(Some(&toolbar));
        let sender = sender.clone();
        let window2 = window.clone();
        add.connect_clicked(move |_| {
            if !name.text().trim().is_empty() {
                sender.input(Msg::SetEnv(
                    name.text().to_string(),
                    value.text().to_string(),
                ));
                window2.close();
            }
        });
        window.present();
    }

    pub(super) fn show_workspace_prompt(&self, sender: &ComponentSender<Self>) {
        let window = gtk::Window::builder()
            .title("Workspace")
            .transient_for(&self.root)
            .modal(true)
            .default_width(460)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let add = gtk::Button::with_label("Add");
        add.add_css_class("suggested-action");
        header.pack_end(&add);
        toolbar.add_top_bar(&header);
        let body = util::page_body();
        let group = util::group("Workspace");
        let name_row = adw::ActionRow::builder()
            .title("Name")
            .subtitle("For example: browser, chat, music")
            .build();
        let name = gtk::Entry::builder()
            .placeholder_text("Name")
            .valign(gtk::Align::Center)
            .build();
        name_row.add_suffix(&name);
        group.add(&name_row);
        let output_row = adw::ActionRow::builder()
            .title("Display")
            .subtitle("Optional. The output name, for example HDMI-A-1.")
            .build();
        let output = gtk::Entry::builder()
            .placeholder_text("Optional")
            .valign(gtk::Align::Center)
            .build();
        output_row.add_suffix(&output);
        group.add(&output_row);
        body.append(&group);
        toolbar.set_content(Some(&util::scrolled(&body)));
        window.set_child(Some(&toolbar));
        let sender = sender.clone();
        let window2 = window.clone();
        add.connect_clicked(move |_| {
            if !name.text().trim().is_empty() {
                sender.input(Msg::AddWorkspace(
                    name.text().to_string(),
                    output.text().to_string(),
                ));
                window2.close();
            }
        });
        window.present();
    }

    pub(super) fn show_import(&self, sender: &ComponentSender<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Choose a Hyprland or Sway config")
            .build();
        let sender = sender.clone();
        let root = self.root.clone();
        dialog.open(Some(&root), None::<&gio::Cancellable>, move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                return;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                sender.input(Msg::Error(format!("Cannot read {}", path.display())));
                return;
            };
            let Some(source) = import::detect(&path, &text) else {
                sender.input(Msg::Error(
                    "Cannot tell whether this is a Hyprland or Sway config.".to_string(),
                ));
                return;
            };
            let result = import::parse(&text, source);
            sender.input(Msg::ImportPreview(result, path));
        });
    }

    pub(super) fn show_import_preview(
        &self,
        sender: &ComponentSender<Self>,
        result: &ImportResult,
        path: &std::path::Path,
    ) {
        let window = gtk::Window::builder()
            .title("Import")
            .transient_for(&self.root)
            .modal(true)
            .default_width(620)
            .default_height(560)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let apply = gtk::Button::with_label("Apply");
        apply.add_css_class("suggested-action");
        header.pack_end(&apply);
        toolbar.add_top_bar(&header);
        let body = util::page_body();
        let intro = gtk::Label::new(Some(&format!(
            "From {}: {}",
            result.source.map(|s| s.label()).unwrap_or("a config"),
            path.display()
        )));
        intro.set_wrap(true);
        intro.set_xalign(0.0);
        body.append(&intro);
        let group = util::group("What will change");
        for (key, value) in &result.changes {
            let row = adw::ActionRow::builder()
                .title(key.clone())
                .subtitle(value.to_string())
                .build();
            group.add(&row);
        }
        for bind in &result.binds {
            let row = adw::ActionRow::builder()
                .title(bind.key.clone())
                .subtitle(bind.action.clone())
                .build();
            group.add(&row);
        }
        for program in &result.startup {
            let row = adw::ActionRow::builder()
                .title("Start with niri")
                .subtitle(program.clone())
                .build();
            group.add(&row);
        }
        for rule in &result.window_rules {
            let row = adw::ActionRow::builder()
                .title("App rule")
                .subtitle(rule.replace('\n', " · "))
                .build();
            group.add(&row);
        }
        body.append(&group);
        if !result.notes.is_empty() {
            let notes = util::group("Needs your attention");
            for note in &result.notes {
                let row = adw::ActionRow::builder().title(note.clone()).build();
                notes.add(&row);
            }
            body.append(&notes);
        }
        toolbar.set_content(Some(&util::scrolled(&body)));
        window.set_child(Some(&toolbar));
        let sender = sender.clone();
        let window2 = window.clone();
        let result = result.clone();
        apply.connect_clicked(move |_| {
            sender.input(Msg::ImportApply(result.clone()));
            window2.close();
        });
        window.present();
    }
}

#[derive(Debug, Clone, Default)]
struct RuleFields {
    app_id: String,
    title: String,
    floating: bool,
    width: f64,
    height: f64,
    corner: f64,
    opacity: f64,
    workspace: String,
}

fn rule_fields(session: &Session, index: usize) -> Option<RuleFields> {
    let node = session
        .tree
        .top_level("window-rule")
        .into_iter()
        .nth(index)?;
    let text = &session.tree.files[node.file].text[node.node_span.clone()];
    let mut fields = RuleFields {
        opacity: 100.0,
        ..RuleFields::default()
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("match app-id=") {
            fields.app_id = unquote(rest).unwrap_or_default();
        } else if let Some(rest) = line.strip_prefix("match title=") {
            fields.title = unquote(rest).unwrap_or_default();
        } else if line == "open-floating true" {
            fields.floating = true;
        } else if let Some(rest) = line.strip_prefix("geometry-corner-radius ") {
            fields.corner = rest.trim().parse().unwrap_or(0.0);
        } else if let Some(rest) = line.strip_prefix("opacity ") {
            fields.opacity = rest
                .trim()
                .parse::<f64>()
                .map(|value| value * 100.0)
                .unwrap_or(100.0);
        } else if let Some(rest) = line.strip_prefix("open-on-workspace ") {
            fields.workspace = unquote(rest).unwrap_or_default();
        } else if let Some(rest) = line.strip_prefix("default-column-width")
            && let Some(value) = fixed_size(rest)
        {
            fields.width = value;
        } else if let Some(rest) = line.strip_prefix("default-window-height")
            && let Some(value) = fixed_size(rest)
        {
            fields.height = value;
        }
    }
    if fields.opacity <= 0.0 {
        fields.opacity = 100.0;
    }
    Some(fields)
}

fn app_id_pattern(input: &str) -> String {
    let input = input.trim();
    if input.starts_with('^') || input.contains('\\') {
        input.to_string()
    } else {
        format!("^{}$", regex_escape(input))
    }
}

fn title_pattern(input: &str) -> String {
    let input = input.trim();
    if input.starts_with('^') || input.contains('\\') {
        input.to_string()
    } else {
        regex_escape(input)
    }
}

fn regex_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ".^$*+?()[]{}|\\".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn fixed_size(rest: &str) -> Option<f64> {
    rest.split("fixed").nth(1).and_then(|value| {
        value
            .trim()
            .trim_end_matches(['}', ';'])
            .trim()
            .parse()
            .ok()
    })
}

fn unquote(text: &str) -> Option<String> {
    let rest = text.trim().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn quote(text: &str) -> String {
    Value::from(text).literal()
}

fn restore_key_label(label: &gtk::Label, recorded: &std::cell::RefCell<String>) {
    let text = recorded.borrow().clone();
    label.set_label(if text.is_empty() {
        "Not recorded"
    } else {
        &text
    });
}

fn key_to_niri(key: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> String {
    let mut parts = Vec::new();
    if state.contains(gtk::gdk::ModifierType::SUPER_MASK)
        || state.contains(gtk::gdk::ModifierType::META_MASK)
        || state.contains(gtk::gdk::ModifierType::HYPER_MASK)
    {
        parts.push("Mod".to_string());
    }
    if state.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
        parts.push("Ctrl".to_string());
    }
    if state.contains(gtk::gdk::ModifierType::ALT_MASK) {
        parts.push("Alt".to_string());
    }
    if state.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
        parts.push("Shift".to_string());
    }
    let Some(name) = key.name() else {
        return String::new();
    };
    let name = name.to_string();
    let name = match name.as_str() {
        "space" => "Space".to_string(),
        "Return" | "KP_Enter" => "Return".to_string(),
        "Escape" => "Escape".to_string(),
        "Tab" => "Tab".to_string(),
        "BackSpace" => "BackSpace".to_string(),
        "Delete" => "Delete".to_string(),
        "Home" => "Home".to_string(),
        "End" => "End".to_string(),
        "Page_Up" => "Page_Up".to_string(),
        "Page_Down" => "Page_Down".to_string(),
        "Left" => "Left".to_string(),
        "Right" => "Right".to_string(),
        "Up" => "Up".to_string(),
        "Down" => "Down".to_string(),
        "Print" => "Print".to_string(),
        "bracketleft" => "BracketLeft".to_string(),
        "bracketright" => "BracketRight".to_string(),
        "minus" => "Minus".to_string(),
        "equal" => "Equal".to_string(),
        "slash" => "Slash".to_string(),
        "comma" => "Comma".to_string(),
        "period" => "Period".to_string(),
        "semicolon" => "Semicolon".to_string(),
        "apostrophe" => "Apostrophe".to_string(),
        "grave" => "Grave".to_string(),
        "exclam" => "1".to_string(),
        "at" => "2".to_string(),
        "numbersign" => "3".to_string(),
        "dollar" => "4".to_string(),
        "percent" => "5".to_string(),
        "asciicircum" => "6".to_string(),
        "ampersand" => "7".to_string(),
        "asterisk" => "8".to_string(),
        "parenleft" => "9".to_string(),
        "parenright" => "0".to_string(),
        "underscore" => "Minus".to_string(),
        "plus" => "Equal".to_string(),
        "braceleft" => "BracketLeft".to_string(),
        "braceright" => "BracketRight".to_string(),
        "bar" => "Backslash".to_string(),
        "colon" => "Semicolon".to_string(),
        "quotedbl" => "Apostrophe".to_string(),
        "less" => "Comma".to_string(),
        "greater" => "Period".to_string(),
        "question" => "Slash".to_string(),
        "asciitilde" => "Grave".to_string(),
        other if other.len() == 1 => other.to_uppercase(),
        other => other.to_string(),
    };
    parts.push(name);
    parts.join("+")
}
