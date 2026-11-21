//! Setting rows shared by every page.
//!
//! Each option uses the libadwaita row that fits its type, so the whole row
//! is the control: a switch row, a spin row, a combo row, or an action row
//! that opens the colour picker. Rows stay narrow enough for a 360 px window.

use std::rc::Rc;

use adw::prelude::*;
use relm4::ComponentSender;

use niri_settings_core::Value;
use niri_settings_core::schema::{self, Control, OptionInfo};
use niri_settings_core::session::{Session, Source};

use crate::app::{AppModel, Msg};
use crate::util::{self, Debounce};

/// Build one setting row.
pub fn option_row(
    session: &Session,
    key: &str,
    sender: &ComponentSender<AppModel>,
    root: &adw::ApplicationWindow,
    show_names: bool,
) -> adw::ActionRow {
    let option = schema::option(key).expect("catalogue key");
    let resolved = session.get_option(option);
    let mut subtitle = util::escape(&crate::i18n::tr_owned(option.help));
    match &resolved.source {
        Source::Managed(_, reason) => {
            subtitle.push_str(&format!(" · <b>{}</b>", util::escape(reason)));
        }
        Source::File(path) if path != &session.real_path => {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                subtitle.push_str(&format!(" · Set by {}", util::escape(name)));
            }
        }
        _ => {}
    }
    if show_names {
        subtitle.push_str(&format!(
            " · <span font_family=\"monospace\" alpha=\"55%\" size=\"small\">{}</span>",
            util::escape(option.key)
        ));
    }
    // A setting inside a section that is off has no effect. Grey it out and
    // show only the reason: the help and config name would push the row to a
    // second line. Both stay in the tooltip and the accessible description.
    let blocked_by = schema::section_switches(key)
        .find(|switch| !session.get_option(switch).value.as_bool().unwrap_or(true));
    let mut tooltip = option.help.to_string();
    if let Some(switch) = blocked_by {
        let label = crate::i18n::tr_owned(switch.label);
        subtitle = blocked_subtitle(&label);
        tooltip = format!("{}\n\n{}", off_note(&label), option.help);
    }
    let title = crate::i18n::tr_owned(option.label);

    let row: adw::ActionRow = match option.control {
        Control::Toggle { .. } => {
            toggle_row(option, resolved.value.as_bool().unwrap_or(false), sender).upcast()
        }
        Control::Number {
            default,
            min,
            max,
            step,
            unit,
            integer,
        } => number_row(
            option,
            resolved.value.as_f64().unwrap_or(default),
            (min, max, step),
            unit.suffix(),
            integer,
            sender,
        )
        .upcast(),
        Control::Text { placeholder, .. } => text_row(
            option,
            resolved.value.as_str().unwrap_or_default(),
            placeholder,
            sender,
        ),
        Control::Color { .. } => {
            color_row(option, resolved.value.as_str().unwrap_or("#000000"), sender)
        }
        Control::Choice { choices, .. } => choice_row(
            option,
            resolved.value.as_str().unwrap_or_default(),
            choices,
            sender,
        )
        .upcast(),
    };
    // The subtitle carries markup (the dimmed config name). Combo rows do
    // not read markup unless asked.
    row.set_use_markup(true);
    row.set_title(&util::escape(&title));
    row.set_subtitle(&subtitle);
    row.set_tooltip_text(Some(&tooltip));
    row.set_widget_name(&format!("opt:{key}"));
    if blocked_by.is_some() {
        row.set_sensitive(false);
    }
    row.upcast_ref::<gtk::Widget>()
        .update_property(&[gtk::accessible::Property::Description(&tooltip)]);

    // The help dialog also holds the reset action. It only earns a place on
    // rows that differ from the default; on other rows the help is already in
    // the subtitle and tooltip.
    if session.is_set(key) {
        let help = flat_icon_button("help-about-symbolic", crate::i18n::tr("About this setting"));
        help.set_widget_name(&format!("help:{key}"));
        let root = root.clone();
        let option = *option;
        let sender = sender.clone();
        let key = key.to_string();
        help.connect_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading(crate::i18n::tr_owned(option.label))
                .body(format!(
                    "{}\n\nConfig name: {}\n\nChanges apply at once and can be undone.",
                    option.help, option.key
                ))
                .build();
            dialog.add_response("reset", crate::i18n::tr("Reset to the niri default"));
            let sender = sender.clone();
            let key = key.clone();
            dialog.connect_response(Some("reset"), move |dialog, _| {
                sender.input(Msg::Reset(key.clone()));
                dialog.close();
            });
            dialog.add_response("close", "Close");
            dialog.set_default_response(Some("close"));
            dialog.present(Some(&root));
        });
        row.add_suffix(&help);
    }

    row
}

/// The note on a setting whose section is off.
pub fn off_note(switch_label: &str) -> String {
    crate::i18n::tr("Turn on {name} to use this.").replace("{name}", switch_label)
}

/// The subtitle for a row in an off section. Only the reason is shown, so the
/// row keeps one short line; the help and config name live in the tooltip.
fn blocked_subtitle(switch_label: &str) -> String {
    util::escape(&off_note(switch_label))
}

fn flat_icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = util::icon_button(icon, tooltip);
    button.add_css_class("flat");
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button
}

fn toggle_row(
    option: &OptionInfo,
    value: bool,
    sender: &ComponentSender<AppModel>,
) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder().active(value).build();
    let sender = sender.clone();
    let key = option.key.to_string();
    row.connect_active_notify(move |row| {
        sender.input(Msg::SetRebuild(key.clone(), Value::Bool(row.is_active())));
    });
    row
}

fn number_row(
    option: &OptionInfo,
    value: f64,
    (min, max, step): (f64, f64, f64),
    suffix: &str,
    integer: bool,
    sender: &ComponentSender<AppModel>,
) -> adw::SpinRow {
    let adjustment = gtk::Adjustment::new(value, min, max, step, step * 10.0, 0.0);
    let row = adw::SpinRow::builder()
        .adjustment(&adjustment)
        .digits(if integer { 0 } else { 2 })
        .numeric(true)
        .climb_rate(1.0)
        .build();
    if !suffix.trim().is_empty() {
        let unit = gtk::Label::new(Some(suffix.trim()));
        unit.add_css_class("dim-label");
        row.add_suffix(&unit);
    }
    // One write per pause, not one per click or key repeat.
    let debounce = Rc::new(Debounce::new());
    let sender = sender.clone();
    let key = option.key.to_string();
    row.connect_value_notify(move |row| {
        let value = row.value();
        let sender = sender.clone();
        let key = key.clone();
        debounce.schedule(250, move || {
            sender.input(Msg::Set(key.clone(), Value::Float(value)));
        });
    });
    row
}

fn text_row(
    option: &OptionInfo,
    value: &str,
    placeholder: &str,
    sender: &ComponentSender<AppModel>,
) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    let entry = gtk::Entry::builder()
        .text(value)
        .placeholder_text(placeholder)
        .width_chars(12)
        .valign(gtk::Align::Center)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label(option.label)]);
    row.add_suffix(&entry);
    row.set_activatable_widget(Some(&entry));
    let debounce = Rc::new(Debounce::new());
    let sender = sender.clone();
    let key = option.key.to_string();
    entry.connect_changed(move |entry| {
        let text = entry.text().to_string();
        let sender = sender.clone();
        let key = key.clone();
        debounce.schedule(500, move || {
            sender.input(Msg::Set(key.clone(), Value::String(text)));
        });
    });
    row
}

fn color_row(
    option: &OptionInfo,
    value: &str,
    sender: &ComponentSender<AppModel>,
) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    let dialog = gtk::ColorDialog::builder().with_alpha(true).build();
    let button = gtk::ColorDialogButton::new(Some(dialog));
    button.set_rgba(&util::hex_to_rgba(value));
    button.set_valign(gtk::Align::Center);
    button.update_property(&[gtk::accessible::Property::Label(option.label)]);
    row.add_suffix(&button);
    // Activating the row opens the colour picker.
    row.set_activatable_widget(Some(&button));
    let sender = sender.clone();
    let key = option.key.to_string();
    button.connect_rgba_notify(move |button| {
        let hex = util::rgba_to_hex(&button.rgba());
        sender.input(Msg::SetRebuild(key.clone(), Value::String(hex)));
    });
    row
}

fn choice_row(
    option: &OptionInfo,
    value: &str,
    choices: &'static [schema::Choice],
    sender: &ComponentSender<AppModel>,
) -> adw::ComboRow {
    let labels: Vec<String> = choices
        .iter()
        .map(|choice| crate::i18n::tr_owned(choice.label))
        .collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let model = gtk::StringList::new(&labels);
    let selected = choices
        .iter()
        .position(|choice| choice.value == value)
        .unwrap_or(0);
    // Choice labels are short. Without a factory, libadwaita shrinks the
    // shown value to "…" before it wraps the description.
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let label = gtk::Label::builder().xalign(0.0).build();
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            item.set_child(Some(&label));
        }
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let (Some(label), Some(text)) = (
            item.child().and_downcast::<gtk::Label>(),
            item.item().and_downcast::<gtk::StringObject>(),
        ) {
            let text = text.string();
            // Keep short values whole; long ones may shorten to "…".
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_width_chars(text.chars().count().min(14) as i32);
            label.set_label(&text);
        }
    });
    let row = adw::ComboRow::builder()
        .model(&model)
        .factory(&factory)
        .selected(selected as u32)
        .build();
    let values: Vec<&'static str> = choices.iter().map(|choice| choice.value).collect();
    let sender = sender.clone();
    let key = option.key.to_string();
    row.connect_selected_notify(move |row| {
        if let Some(value) = values.get(row.selected() as usize) {
            sender.input(Msg::SetRebuild(
                key.clone(),
                Value::String((*value).to_string()),
            ));
        }
    });
    row
}

/// Add several option rows to a group.
pub fn add_options(
    group: &adw::PreferencesGroup,
    session: &Session,
    keys: &[&str],
    sender: &ComponentSender<AppModel>,
    root: &adw::ApplicationWindow,
    show_names: bool,
) {
    for key in keys {
        if schema::option(key).is_some() {
            group.add(&option_row(session, key, sender, root, show_names));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_off_note_names_the_switch() {
        assert_eq!(off_note("Focus ring"), "Turn on Focus ring to use this.");
    }

    #[test]
    fn a_blocked_row_shows_only_the_reason() {
        let subtitle = blocked_subtitle("Window border");
        assert_eq!(subtitle, "Turn on Window border to use this.");
        assert!(!subtitle.contains("layout.border.width"));
        assert!(!subtitle.contains('\n'));
    }
}
