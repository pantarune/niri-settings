//! Page builders. Each page is a plain widget; the shell swaps it in.

use adw::prelude::*;
use relm4::ComponentSender;

use niri_settings_core::Value;
use niri_settings_core::lint::{self, Severity};
use niri_settings_core::niri;
use niri_settings_core::presets::{self, PresetGroup};
use niri_settings_core::schema::{self, Page};
use niri_settings_core::schemes;
use niri_settings_core::session::Session;

use crate::app::{AppModel, Msg};
use crate::{arrange, generated, i18n, preview, rows, util};

pub fn build(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Widget {
    let content = match model.page {
        Page::Home => home(model, sender),
        Page::Appearance => appearance(model, sender),
        Page::Layout => layout(model, sender),
        Page::Apps => apps(model, sender),
        Page::Keyboard => keyboard(model, sender),
        Page::Shortcuts => shortcuts(model, sender),
        Page::Mouse => mouse(model, sender),
        Page::Displays => displays(model, sender),
        Page::Workspaces => workspaces(model, sender),
        Page::Animations => animations(model, sender),
        Page::Startup => startup(model, sender),
        Page::Health => health(model, sender),
        Page::All => all_options(model, sender),
        Page::Config => config(model, sender),
    };
    util::scrolled(&content).upcast()
}

fn home(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    if model.welcome.active {
        return crate::welcome::page(model, sender);
    }
    let body = util::page_body();
    let session = &model.session;
    // Checks run in the background after each change (see jobs.rs).
    let checked = model
        .checked
        .as_ref()
        .map(|checked| checked.problems.clone());
    let problems = checked.clone().unwrap_or_default();
    let errors = problems
        .iter()
        .filter(|problem| problem.severity == Severity::Error)
        .count();
    let warnings = problems
        .iter()
        .filter(|problem| problem.severity == Severity::Warning)
        .count();

    let hello = gtk::Label::new(Some(i18n::tr("Your setup")));
    hello.add_css_class("title-1");
    hello.set_xalign(0.0);
    body.append(&hello);
    let intro = gtk::Label::new(Some(
        "Change anything here. Each change applies at once, and you can undo it.",
    ));
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    intro.add_css_class("dim-label");
    body.append(&intro);

    let preview = preview::appearance_preview(session);
    let frame = gtk::Frame::new(None);
    frame.add_css_class("card");
    frame.set_child(Some(&preview));
    body.append(&frame);

    let status = util::group(i18n::tr("At a glance"));
    let niri_row = adw::ActionRow::builder()
        .title("niri")
        .subtitle(niri_status())
        .build();
    status.add(&niri_row);
    let config_row = adw::ActionRow::builder()
        .title("Config file")
        .subtitle(session.real_path.display().to_string())
        .build();
    let open = gtk::Button::with_label(i18n::tr("Open"));
    open.set_valign(gtk::Align::Center);
    let sender_open = sender.clone();
    open.connect_clicked(move |_| sender_open.input(Msg::Navigate(Page::Config)));
    config_row.add_suffix(&open);
    status.add(&config_row);
    let health_row = adw::ActionRow::builder()
        .title("Problems")
        .subtitle(if checked.is_none() {
            "Checking your setup…".to_string()
        } else if problems.is_empty() {
            "No problems found".to_string()
        } else {
            let errors = if errors == 1 {
                "1 error".to_string()
            } else {
                format!("{errors} errors")
            };
            let warnings = if warnings == 1 {
                "1 warning".to_string()
            } else {
                format!("{warnings} warnings")
            };
            format!("{errors} · {warnings}")
        })
        .build();
    let check = gtk::Button::with_label(i18n::tr("Check"));
    check.set_valign(gtk::Align::Center);
    let sender_check = sender.clone();
    check.connect_clicked(move |_| sender_check.input(Msg::Navigate(Page::Health)));
    health_row.add_suffix(&check);
    status.add(&health_row);
    body.append(&status);

    if !session.history().is_empty() {
        let recent = util::group(i18n::tr("Recent changes"));
        for entry in session.history().iter().rev().take(3) {
            let row = adw::ActionRow::builder()
                .title(entry.summary.clone())
                .subtitle(util::format_age(entry.time))
                .build();
            let restore = gtk::Button::with_label("Restore");
            restore.set_valign(gtk::Align::Center);
            let sender = sender.clone();
            let id = entry.id;
            restore.connect_clicked(move |_| sender.input(Msg::Restore(id)));
            row.add_suffix(&restore);
            recent.add(&row);
        }
        body.append(&recent);
    }

    let import_group = util::group(i18n::tr("From another compositor"));
    let import_row = action_button(
        "Import from Hyprland or Sway",
        "Copy your shortcuts and appearance",
        "Import",
        sender.clone(),
        Msg::OpenImport,
    );
    import_group.add(&import_row);
    body.append(&import_group);

    if matches!(
        session.main_managed(),
        Some(niri_settings_core::Managed::Nix)
    ) {
        let nix = util::group(i18n::tr("Nix managed config"));
        let row = adw::ActionRow::builder()
            .title("This file comes from the Nix store")
            .subtitle("niri settings cannot write it. Copy your changes as a Nix block and add them to your system config.")
            .build();
        let copy = gtk::Button::with_label("Copy as Nix");
        copy.set_valign(gtk::Align::Center);
        let sender = sender.clone();
        copy.connect_clicked(move |_| sender.input(Msg::CopyNix));
        row.add_suffix(&copy);
        nix.add(&row);
        body.append(&nix);
    }
    body
}

fn appearance(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let frame = gtk::Frame::new(None);
    frame.add_css_class("card");
    frame.set_child(Some(&preview::appearance_preview(session)));
    body.append(&frame);

    let presets_group = util::group(i18n::tr("Presets"));
    presets_group.add(&preset_buttons(PresetGroup::Appearance, session, sender));
    body.append(&presets_group);

    let spacing = util::group(i18n::tr("Spacing"));
    rows::add_options(
        &spacing,
        session,
        &["layout.gaps"],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&spacing);

    let ring = util::group(i18n::tr("Focus ring"));
    rows::add_options(
        &ring,
        session,
        &[
            "layout.focus-ring.off",
            "layout.focus-ring.width",
            "layout.focus-ring.active-color",
            "layout.focus-ring.inactive-color",
            "layout.focus-ring.urgent-color",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&ring);

    let border = util::group(i18n::tr("Window border"));
    rows::add_options(
        &border,
        session,
        &[
            "layout.border.off",
            "layout.border.width",
            "layout.border.active-color",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&border);

    let shadow = util::group(i18n::tr("Window shadows"));
    rows::add_options(
        &shadow,
        session,
        &[
            "layout.shadow.on",
            "layout.shadow.softness",
            "layout.shadow.spread",
            "layout.shadow.offset.x",
            "layout.shadow.offset.y",
            "layout.shadow.color",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&shadow);

    let tabs = util::group(i18n::tr("Tab indicator"));
    rows::add_options(
        &tabs,
        session,
        &[
            "layout.tab-indicator.off",
            "layout.tab-indicator.width",
            "layout.tab-indicator.position",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&tabs);

    let colors = util::group(i18n::tr("Background and overview"));
    rows::add_options(
        &colors,
        session,
        &[
            "layout.background-color",
            "overview.zoom",
            "overview.backdrop-color",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&colors);

    let cursor = util::group(i18n::tr("Cursor"));
    rows::add_options(
        &cursor,
        session,
        &[
            "cursor.xcursor-theme",
            "cursor.xcursor-size",
            "cursor.hide-when-typing",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&cursor);
    body
}

fn layout(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let columns = util::group(i18n::tr("Columns"));
    rows::add_options(
        &columns,
        session,
        &[
            "layout.center-focused-column",
            "layout.always-center-single-column",
            "layout.default-column-width.proportion",
            "layout.default-column-display",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&columns);

    let struts = util::group(i18n::tr("Reserved space for bars and panels"));
    rows::add_options(
        &struts,
        session,
        &[
            "layout.struts.left",
            "layout.struts.right",
            "layout.struts.top",
            "layout.struts.bottom",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&struts);

    let workspaces = util::group(i18n::tr("Workspaces"));
    rows::add_options(
        &workspaces,
        session,
        &["layout.empty-workspace-above-first"],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&workspaces);

    let note = gtk::Label::new(Some(
        "The preset widths for columns and the preset heights for windows are lists. Edit them on the Config file page or in All options.",
    ));
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.add_css_class("dim-label");
    body.append(&note);
    body
}

fn keyboard(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let layout_group = util::group(i18n::tr("Layout"));
    rows::add_options(
        &layout_group,
        session,
        &[
            "input.keyboard.xkb.layout",
            "input.keyboard.xkb.variant",
            "input.keyboard.xkb.options",
            "input.mod-key",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&layout_group);

    let test = util::group("Test the layout");
    let entry = gtk::Entry::builder()
        .placeholder_text("Type here to test the selected layout")
        .build();
    test.add(&entry);
    body.append(&test);

    let repeat = util::group(i18n::tr("Repeat"));
    rows::add_options(
        &repeat,
        session,
        &[
            "input.keyboard.repeat-delay",
            "input.keyboard.repeat-rate",
            "input.keyboard.numlock",
            "input.keyboard.track-layout",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&repeat);
    body
}

fn mouse(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let mouse_group = util::group(i18n::tr("Mouse"));
    rows::add_options(
        &mouse_group,
        session,
        &[
            "input.mouse.accel-speed",
            "input.mouse.accel-profile",
            "input.mouse.natural-scroll",
            "input.mouse.left-handed",
            "input.mouse.scroll-method",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&mouse_group);

    let touchpad = util::group(i18n::tr("Touchpad"));
    rows::add_options(
        &touchpad,
        session,
        &[
            "input.touchpad.tap",
            "input.touchpad.dwt",
            "input.touchpad.natural-scroll",
            "input.touchpad.accel-speed",
            "input.touchpad.accel-profile",
            "input.touchpad.click-method",
            "input.touchpad.tap-button-map",
            "input.touchpad.scroll-method",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&touchpad);

    let focus = util::group(i18n::tr("Focus"));
    rows::add_options(
        &focus,
        session,
        &[
            "input.focus-follows-mouse.max-scroll-amount",
            "input.warp-mouse-to-focus.mode",
            "input.workspace-auto-back-and-forth",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&focus);
    body
}

fn animations(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let frame = gtk::Frame::new(None);
    frame.add_css_class("card");
    frame.set_child(Some(&preview::animation_preview(session)));
    body.append(&frame);

    let presets_group = util::group(i18n::tr("Presets"));
    presets_group.add(&preset_buttons(PresetGroup::Animations, session, sender));
    body.append(&presets_group);

    let settings = util::group(i18n::tr("Motion"));
    rows::add_options(
        &settings,
        session,
        &["animations.off", "animations.slowdown"],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&settings);

    let list = util::group(i18n::tr("Per-animation settings"));
    rows::add_options(
        &list,
        session,
        &[
            "animations.workspace-switch.off",
            "animations.window-open.off",
            "animations.window-close.off",
            "animations.window-movement.off",
            "animations.window-resize.off",
            "animations.overview-open-close.off",
        ],
        sender,
        &model.root,
        model.prefs.show_config_names,
    );
    body.append(&list);
    body
}

fn all_options(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let query = model.search.trim();
    let options: Vec<_> = schema::OPTIONS
        .iter()
        .filter(|option| option.matches(query))
        .collect();
    let count = options.len();
    let heading_text = if query.is_empty() {
        format!("{count} options")
    } else {
        format!("{count} results for “{query}”")
    };
    let heading = gtk::Label::new(Some(&heading_text));
    heading.add_css_class("title-4");
    heading.set_xalign(0.0);
    body.append(&heading);

    // Pages whose name matches, so search also finds where things live.
    let needle = query.to_lowercase();
    let matching_pages: Vec<_> = if needle.is_empty() {
        Vec::new()
    } else {
        schema::PAGES
            .iter()
            .filter(|info| {
                info.title.to_lowercase().contains(&needle)
                    || info.subtitle.to_lowercase().contains(&needle)
                    || i18n::tr_owned(info.title).to_lowercase().contains(&needle)
            })
            .collect()
    };
    if !matching_pages.is_empty() {
        let group = util::group(i18n::tr("Pages"));
        for info in matching_pages {
            let row = adw::ActionRow::builder()
                .title(i18n::tr_owned(info.title))
                .subtitle(i18n::tr_owned(info.subtitle))
                .activatable(true)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name(info.icon));
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            let sender = sender.clone();
            let page = info.page;
            row.connect_activated(move |_| sender.input(Msg::Navigate(page)));
            group.add(&row);
        }
        body.append(&group);
    }

    let mut current: Option<&str> = None;
    let mut group: Option<adw::PreferencesGroup> = None;
    for option in options {
        let section = option.parent.first().copied().unwrap_or("General");
        if current != Some(section) {
            if let Some(group) = group.take() {
                body.append(&group);
            }
            let title = section_title(section);
            group = Some(util::group(title));
            current = Some(section);
        }
        if let Some(group) = &group {
            group.add(&rows::option_row(
                session,
                option.key,
                sender,
                &model.root,
                model.prefs.show_config_names,
            ));
        }
    }
    if let Some(group) = group {
        body.append(&group);
    }

    let known: std::collections::HashSet<String> = schema::OPTIONS
        .iter()
        .map(|option| option.key.to_string())
        .collect();
    let extras: Vec<_> = generated::extras(&known)
        .into_iter()
        .filter(|option| {
            query.is_empty()
                || option.path.to_lowercase().contains(&query.to_lowercase())
                || option.doc.to_lowercase().contains(&query.to_lowercase())
        })
        .collect();
    if !extras.is_empty() {
        let group = util::group(i18n::tr("From the niri schema"));
        let note = gtk::Label::new(Some(
            "These options come from the niri source code. If an option has no control here, edit it in the config file.",
        ));
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.add_css_class("dim-label");
        group.add(&note);
        for option in extras {
            let mut subtitle = if option.doc.is_empty() {
                format!("{} · {}", option.kind, option.path)
            } else {
                format!("{} · {}", option.doc, option.kind)
            };
            if let Some(choices) = &option.options {
                subtitle.push_str(&format!(" · {}", choices.join(", ")));
            }
            let row = adw::ActionRow::builder()
                .title(option.path.clone())
                .subtitle(subtitle)
                .build();
            if let Some(resolved) = session.get(&option.path) {
                let value = gtk::Label::new(Some(&resolved.value.to_string()));
                value.add_css_class("dim-label");
                value.add_css_class("monospace");
                value.set_valign(gtk::Align::Center);
                row.add_suffix(&value);
            }
            let open = gtk::Button::with_label(i18n::tr("Open"));
            open.set_valign(gtk::Align::Center);
            let sender = sender.clone();
            open.connect_clicked(move |_| sender.input(Msg::Navigate(Page::Config)));
            row.add_suffix(&open);
            group.add(&row);
        }
        body.append(&group);
    }

    let raw = util::group("Raw nodes in your file");
    let mut names = Vec::new();
    for node in &session.tree.nodes {
        if node.path.len() == 1 && !names.contains(&node.name) {
            names.push(node.name.clone());
        }
    }
    for name in names {
        if schema::OPTIONS.iter().any(|option| option.key == name) {
            continue;
        }
        let row = adw::ActionRow::builder()
            .title(name.clone())
            .subtitle("Edit this section on the Config file page")
            .build();
        let button = gtk::Button::with_label(i18n::tr("Open"));
        button.set_valign(gtk::Align::Center);
        let sender = sender.clone();
        button.connect_clicked(move |_| sender.input(Msg::Navigate(Page::Config)));
        row.add_suffix(&button);
        raw.add(&row);
    }
    body.append(&raw);
    body
}

fn config(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let files = session.tree.files.clone();
    let index = model.config_file.min(files.len().saturating_sub(1));
    let file = &files[index];

    let chooser = adw::WrapBox::builder()
        .child_spacing(8)
        .line_spacing(8)
        .build();
    let labels: Vec<String> = files
        .iter()
        .map(|file| {
            let name = file
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("config.kdl");
            if file.is_main {
                format!("{name} (main)")
            } else {
                name.to_string()
            }
        })
        .collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&label_refs);
    dropdown.set_selected(index as u32);
    let sender_file = sender.clone();
    dropdown.connect_selected_notify(move |dropdown| {
        sender_file.input(Msg::ConfigFile(dropdown.selected() as usize));
    });
    chooser.append(&dropdown);
    let reload = gtk::Button::with_label("Reload");
    let sender_reload = sender.clone();
    reload.connect_clicked(move |_| sender_reload.input(Msg::Reload));
    chooser.append(&reload);
    let open = gtk::Button::with_label("Open in editor");
    let sender_open = sender.clone();
    open.connect_clicked(move |_| sender_open.input(Msg::OpenConfig));
    chooser.append(&open);
    body.append(&chooser);

    if let Some(reason) = file.managed.reason() {
        let note = gtk::Label::new(Some(&format!(
            "{reason}. This file is not edited here. niri settings writes an override file instead."
        )));
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.add_css_class("dim-label");
        body.append(&note);
    }

    let view = gtk::TextView::new();
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::None);
    view.set_top_margin(10);
    view.set_bottom_margin(10);
    view.set_left_margin(10);
    view.set_right_margin(10);
    view.buffer().set_text(&file.text);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(360)
        .vexpand(true)
        .build();
    scroll.add_css_class("card");
    body.append(&scroll);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let apply = gtk::Button::with_label("Apply changes");
    apply.add_css_class("suggested-action");
    if !file.managed.is_writable() {
        apply.set_sensitive(false);
        view.set_editable(false);
    }
    let buffer = view.buffer();
    let path = file.path.clone();
    let sender_apply = sender.clone();
    apply.connect_clicked(move |_| {
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, true).to_string();
        sender_apply.input(Msg::WriteRaw(path.clone(), text));
    });
    actions.append(&apply);
    let note = gtk::Label::new(Some(
        "niri checks the file before it is saved. niri settings checks every changed file immediately before writing it.",
    ));
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.add_css_class("dim-label");
    note.set_hexpand(true);
    actions.append(&note);
    body.append(&actions);
    body
}

fn shortcuts(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let binds = lint::collect_binds(&session.tree);
    let query = model.shortcut_search.to_lowercase();
    let filtered: Vec<_> = binds
        .iter()
        .filter(|bind| {
            query.is_empty()
                || bind.key.to_lowercase().contains(&query)
                || bind.action.to_lowercase().contains(&query)
                || bind.args.join(" ").to_lowercase().contains(&query)
        })
        .collect();

    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Find a shortcut")
        .hexpand(true)
        .build();
    search.set_text(&model.shortcut_search);
    let sender_search = sender.clone();
    search.connect_search_changed(move |entry| {
        sender_search.input(Msg::ShortcutSearch(entry.text().to_string()));
    });
    bar.append(&search);
    let add = gtk::Button::with_label("Add shortcut");
    add.add_css_class("suggested-action");
    let sender_add = sender.clone();
    add.connect_clicked(move |_| sender_add.input(Msg::OpenAddBind));
    bar.append(&add);
    body.append(&bar);

    let scheme_group = util::group(i18n::tr("Shortcut scheme"));
    scheme_group.set_description(Some(
        "Adds or replaces the listed keys in the override file. You see the changes before they apply.",
    ));
    for scheme in schemes::SCHEMES {
        let row = adw::ActionRow::builder()
            .title(scheme.label)
            .subtitle(scheme.detail)
            .build();
        let apply = gtk::Button::with_label(i18n::tr("Apply"));
        apply.set_valign(gtk::Align::Center);
        apply.update_property(&[gtk::accessible::Property::Label(&format!(
            "Apply the {} scheme",
            scheme.label
        ))]);
        let sender_scheme = sender.clone();
        let id = scheme.id;
        apply.connect_clicked(move |_| sender_scheme.input(Msg::ApplyScheme(id)));
        row.add_suffix(&apply);
        scheme_group.add(&row);
    }
    body.append(&scheme_group);

    let duplicates: Vec<_> = session
        .tree
        .all_binds()
        .iter()
        .map(|node| node.name.clone())
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut duplicate_keys = Vec::new();
    for key in &duplicates {
        if !seen.insert(key.clone()) {
            duplicate_keys.push(key.clone());
        }
    }
    if !duplicate_keys.is_empty() {
        let warning = gtk::Label::new(Some(&format!(
            "{} is bound more than once. niri uses the last one.",
            duplicate_keys.join(", ")
        )));
        warning.set_wrap(true);
        warning.set_xalign(0.0);
        warning.add_css_class("warning");
        body.append(&warning);
    }

    let groups = [
        (
            "Windows",
            "close-window|fullscreen|maximize|toggle-window|move-column-to-monitor|consume|expel|center|set-column|switch-preset|reset-window",
        ),
        ("Navigation", "focus|move-window|move-column"),
        ("Workspaces", "workspace"),
        ("Apps", "spawn"),
        (
            "System",
            "screenshot|quit|power|reload|toggle-overview|show-hotkey|toggle-keyboard",
        ),
    ];
    for (title, patterns) in groups {
        let group = util::group(title);
        let mut any = false;
        for bind in &filtered {
            let pattern_match = patterns
                .split('|')
                .any(|pattern| bind.action.contains(pattern));
            if !pattern_match {
                continue;
            }
            any = true;
            let row = bind_row(bind, sender);
            group.add(&row);
        }
        if any {
            body.append(&group);
        }
    }
    let group = util::group("Other");
    let mut any = false;
    for bind in &filtered {
        let mut matched = false;
        for (_, patterns) in groups {
            if patterns
                .split('|')
                .any(|pattern| bind.action.contains(pattern))
            {
                matched = true;
                break;
            }
        }
        if !matched {
            any = true;
            group.add(&bind_row(bind, sender));
        }
    }
    if any {
        body.append(&group);
    }
    if filtered.is_empty() {
        let label = gtk::Label::new(Some("No shortcut matches this search."));
        label.add_css_class("dim-label");
        label.set_wrap(true);
        body.append(&label);
    }
    body
}

fn bind_row(bind: &lint::Bind, sender: &ComponentSender<AppModel>) -> adw::ActionRow {
    let args = if bind.args.is_empty() {
        String::new()
    } else {
        format!(" {}", bind.args.join(" "))
    };
    let row = adw::ActionRow::builder()
        .title(format!("{}{}", bind.action_label(), args))
        .activatable(true)
        .build();
    row.set_tooltip_text(Some("Change this shortcut"));
    let keys = gtk::Label::new(Some(&bind.key));
    keys.add_css_class("monospace");
    keys.set_valign(gtk::Align::Center);
    // Keep a chord on one line. A narrow row shortens the end of the chord;
    // breaking "Mod+Shift+Ctrl+Left" over two lines is unreadable.
    keys.set_ellipsize(gtk::pango::EllipsizeMode::End);
    keys.set_max_width_chars(key_width_chars(&bind.key));
    keys.set_tooltip_text(Some(&bind.key));
    row.add_suffix(&keys);
    let help = util::icon_button("help-about-symbolic", "About this shortcut");
    help.add_css_class("flat");
    {
        let heading = format!("{}{}", bind.action_label(), args);
        let action = format!("{}{}", bind.action, args);
        let key = bind.key.clone();
        let anchor = row.clone();
        help.connect_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading(&heading)
                .body(format!(
                    "Key: {key}\n\nniri action: {action}\n\nClick the shortcut to change the key."
                ))
                .build();
            dialog.add_response("close", "Close");
            dialog.set_default_response(Some("close"));
            dialog.present(Some(&anchor));
        });
    }
    row.add_suffix(&help);
    let sender_edit = sender.clone();
    let key = bind.key.clone();
    row.connect_activated(move |_| sender_edit.input(Msg::OpenEditBind(key.clone())));
    row
}

fn apps(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let pick = gtk::Button::with_label("Pick a window");
    let sender_pick = sender.clone();
    pick.connect_clicked(move |_| sender_pick.input(Msg::PickWindow));
    bar.append(&pick);
    let add = gtk::Button::with_label("New rule");
    add.add_css_class("suggested-action");
    let sender_add = sender.clone();
    add.connect_clicked(move |_| sender_add.input(Msg::OpenAddRule(None)));
    bar.append(&add);
    body.append(&bar);

    let group = util::group(i18n::tr("App rules"));
    let rules = session.tree.top_level("window-rule");
    if rules.is_empty() {
        let label = gtk::Label::new(Some("No app rules yet. Pick a window or add one."));
        label.add_css_class("dim-label");
        label.set_wrap(true);
        label.set_xalign(0.0);
        group.add(&label);
    }
    for (index, node) in rules.iter().enumerate() {
        let text = &session.tree.files[node.file].text[node.node_span.clone()];
        let (title, subtitle) = summarise_rule(text);
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(subtitle)
            .build();
        let edit = gtk::Button::with_label("Edit");
        edit.set_valign(gtk::Align::Center);
        let sender_edit = sender.clone();
        edit.connect_clicked(move |_| sender_edit.input(Msg::OpenEditRule(index)));
        row.add_suffix(&edit);
        let remove = util::icon_button("user-trash-symbolic", "Remove this rule");
        let sender_remove = sender.clone();
        remove.connect_clicked(move |_| sender_remove.input(Msg::RemoveRule(index)));
        row.add_suffix(&remove);
        group.add(&row);
    }
    body.append(&group);

    if !niri::windows().is_empty() {
        let open = util::group("Open windows without a rule");
        for window in niri::windows().into_iter().take(12) {
            let app_id = window.app_id.unwrap_or_default();
            let title = window.title.unwrap_or_default();
            if app_id.is_empty() {
                continue;
            }
            let covered = rules.iter().any(|node| {
                let text = &session.tree.files[node.file].text[node.node_span.clone()];
                text.contains(&app_id)
            });
            if covered {
                continue;
            }
            let row = adw::ActionRow::builder()
                .title(app_id.clone())
                .subtitle(title)
                .build();
            let add = gtk::Button::with_label(i18n::tr("Add"));
            add.set_valign(gtk::Align::Center);
            let sender = sender.clone();
            let app_id2 = app_id.clone();
            add.connect_clicked(move |_| sender.input(Msg::OpenAddRule(Some(app_id2.clone()))));
            row.add_suffix(&add);
            open.add(&row);
        }
        body.append(&open);
    }
    body
}

fn workspaces(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let add = gtk::Button::with_label("Add workspace");
    add.add_css_class("suggested-action");
    let sender_add = sender.clone();
    add.connect_clicked(move |_| sender_add.input(Msg::OpenAddWorkspace));
    bar.append(&add);
    body.append(&bar);

    let group = util::group(i18n::tr("Named workspaces"));
    let nodes = session.tree.top_level("workspace");
    if nodes.is_empty() {
        let label = gtk::Label::new(Some(
            "No named workspaces. niri names workspaces by number by default.",
        ));
        label.add_css_class("dim-label");
        label.set_wrap(true);
        label.set_xalign(0.0);
        group.add(&label);
    }
    for node in nodes {
        let name = node
            .entry(None)
            .and_then(|entry| entry.value.as_str().map(str::to_string))
            .unwrap_or_else(|| "Workspace".to_string());
        let text = &session.tree.files[node.file].text[node.node_span.clone()];
        let output = text
            .lines()
            .find(|line| line.trim().starts_with("open-on-output"))
            .map(|line| line.trim().to_string())
            .unwrap_or_else(|| "Any display".to_string());
        let row = adw::ActionRow::builder()
            .title(name.clone())
            .subtitle(output)
            .build();
        let remove = util::icon_button("user-trash-symbolic", "Remove this workspace");
        let sender = sender.clone();
        remove.connect_clicked(move |_| sender.input(Msg::RemoveWorkspace(name.clone())));
        row.add_suffix(&remove);
        group.add(&row);
    }
    body.append(&group);

    let outputs = niri::outputs();
    if !outputs.is_empty() {
        let group = util::group("Connected displays");
        for output in outputs {
            let row = adw::ActionRow::builder()
                .title(output.name.clone())
                .subtitle(niri::output_label(&output))
                .build();
            group.add(&row);
        }
        body.append(&group);
    }
    body
}

fn displays(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let outputs = niri::outputs();
    if outputs.is_empty() {
        let note = gtk::Label::new(Some(
            "niri is not running, so live display information is not available. Displays saved in the file are listed below.",
        ));
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.add_css_class("dim-label");
        body.append(&note);
        let group = util::group("Displays in the config");
        for node in model.session.tree.top_level("output") {
            let name = node
                .entry(None)
                .and_then(|entry| entry.value.as_str().map(str::to_string))
                .unwrap_or_else(|| "Display".to_string());
            let row = adw::ActionRow::builder()
                .title(name)
                .subtitle("Saved in the config")
                .build();
            group.add(&row);
        }
        body.append(&group);
        return body;
    }

    let arrange_group = util::group(i18n::tr("Arrange displays"));
    arrange_group.add(&arrange::canvas(&outputs, sender));
    body.append(&arrange_group);

    for output in outputs {
        let group = util::group(&format!(
            "{} — {}",
            output.name,
            niri::output_label(&output)
        ));
        let enabled = output.current_mode.is_some();

        let enabled_row = adw::SwitchRow::builder()
            .title(i18n::tr("Display on"))
            .subtitle("Turn the display off or on")
            .active(enabled)
            .build();
        {
            let sender = sender.clone();
            let name = output.name.clone();
            enabled_row.connect_active_notify(move |row| {
                sender.input(Msg::DisplayChange {
                    name: name.clone(),
                    field: "off".to_string(),
                    value: Value::Bool(!row.is_active()),
                });
            });
        }
        group.add(&enabled_row);

        let labels: Vec<String> = output
            .modes
            .iter()
            .map(|mode| {
                format!(
                    "{} × {} · {} Hz",
                    mode.width,
                    mode.height,
                    (mode.refresh_rate as f64 / 1000.0).round() as u32,
                )
            })
            .collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let dropdown = adw::ComboRow::builder()
            .title("Resolution and refresh rate")
            .subtitle("The change applies at once. It is saved when you press Keep.")
            .model(&gtk::StringList::new(&label_refs))
            .selected(output.current_mode.unwrap_or(0) as u32)
            .build();
        let values: Vec<(String, u16, u16, u32)> = output
            .modes
            .iter()
            .map(|mode| {
                (
                    output.name.clone(),
                    mode.width,
                    mode.height,
                    mode.refresh_rate,
                )
            })
            .collect();
        {
            let sender = sender.clone();
            dropdown.connect_selected_notify(move |dropdown| {
                if let Some((name, width, height, refresh)) =
                    values.get(dropdown.selected() as usize)
                {
                    sender.input(Msg::DisplayChange {
                        name: name.clone(),
                        field: "mode".to_string(),
                        value: Value::String(format!(
                            "{width}x{height}@{:.3}",
                            *refresh as f64 / 1000.0
                        )),
                    });
                }
            });
        }
        group.add(&dropdown);

        let scale = output.logical.map(|logical| logical.scale).unwrap_or(1.0);
        let scale_spin = adw::SpinRow::builder()
            .title("Scale")
            .subtitle("Make everything larger without a change of resolution")
            .adjustment(&gtk::Adjustment::new(scale, 0.5, 4.0, 0.25, 0.5, 0.0))
            .digits(2)
            .build();
        {
            let sender = sender.clone();
            let name = output.name.clone();
            let debounce = std::rc::Rc::new(util::Debounce::new());
            scale_spin.connect_value_notify(move |spin| {
                let sender = sender.clone();
                let name = name.clone();
                let value = spin.value();
                debounce.schedule(500, move || {
                    sender.input(Msg::DisplayChange {
                        name: name.clone(),
                        field: "scale".to_string(),
                        value: Value::Float(value),
                    });
                });
            });
        }
        group.add(&scale_spin);

        let transforms = [
            "normal",
            "90",
            "180",
            "270",
            "flipped",
            "flipped-90",
            "flipped-180",
            "flipped-270",
        ];
        let transform_labels = [
            "Normal",
            "90° clockwise",
            "180°",
            "270° clockwise",
            "Flipped",
            "Flipped, 90°",
            "Flipped, 180°",
            "Flipped, 270°",
        ];
        let transform_dropdown = adw::ComboRow::builder()
            .title("Rotation")
            .model(&gtk::StringList::new(&transform_labels))
            .build();
        let current_transform = output
            .logical
            .map(|logical| crate::app::transform_name(logical.transform))
            .unwrap_or("normal");
        let selected = transforms
            .iter()
            .position(|value| *value == current_transform)
            .unwrap_or(0);
        transform_dropdown.set_selected(selected as u32);
        {
            let sender = sender.clone();
            let name = output.name.clone();
            transform_dropdown.connect_selected_notify(move |dropdown| {
                if let Some(value) = transforms.get(dropdown.selected() as usize) {
                    sender.input(Msg::DisplayChange {
                        name: name.clone(),
                        field: "transform".to_string(),
                        value: Value::String((*value).to_string()),
                    });
                }
            });
        }
        group.add(&transform_dropdown);

        if output.vrr_supported {
            let vrr_labels = ["Off", "Always on", "On demand"];
            let vrr_dropdown = adw::ComboRow::builder()
                .title("Variable refresh rate")
                .subtitle("Change the refresh rate of the screen to match the app")
                .model(&gtk::StringList::new(&vrr_labels))
                .selected(if output.vrr_enabled { 1 } else { 0 })
                .build();
            {
                let sender = sender.clone();
                let name = output.name.clone();
                vrr_dropdown.connect_selected_notify(move |dropdown| {
                    let value = match dropdown.selected() {
                        0 => "off",
                        1 => "on",
                        _ => "on-demand",
                    };
                    sender.input(Msg::DisplayChange {
                        name: name.clone(),
                        field: "vrr".to_string(),
                        value: Value::String(value.to_string()),
                    });
                });
            }
            group.add(&vrr_dropdown);
        }
        body.append(&group);
    }

    let note = gtk::Label::new(Some(
        "A display change applies at once. Press Keep within 15 seconds, or niri settings reverts it. If the app stops unexpectedly, it restores the old display settings when it starts again.",
    ));
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.add_css_class("dim-label");
    body.append(&note);
    body
}

fn startup(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let session = &model.session;
    let group = util::group(i18n::tr("Programs that start with niri"));
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let add = gtk::Button::with_label("Add program");
    add.add_css_class("suggested-action");
    let sender_add = sender.clone();
    add.connect_clicked(move |_| sender_add.input(Msg::OpenAddStartup));
    bar.append(&add);
    group.add(&bar);
    for (index, node) in session
        .tree
        .top_level("spawn-at-startup")
        .iter()
        .enumerate()
    {
        let command = node
            .entry(None)
            .map(|entry| entry.value.to_string())
            .unwrap_or_default();
        let row = adw::ActionRow::builder()
            .title(command.clone())
            .subtitle("Starts with niri")
            .build();
        let remove = util::icon_button("user-trash-symbolic", "Remove from startup");
        let sender = sender.clone();
        remove.connect_clicked(move |_| sender.input(Msg::RemoveStartup(index)));
        row.add_suffix(&remove);
        group.add(&row);
    }
    body.append(&group);

    let env = util::group(i18n::tr("Environment variables"));
    let add_env = gtk::Button::with_label("Add variable");
    let sender_env = sender.clone();
    add_env.connect_clicked(move |_| sender_env.input(Msg::OpenAddEnv));
    env.add(&add_env);
    let mut seen = std::collections::HashSet::new();
    for node in &session.tree.nodes {
        if node.path.len() == 2 && node.path[0] == "environment" && seen.insert(node.name.clone()) {
            let value = node
                .entry(None)
                .map(|entry| entry.value.to_string())
                .unwrap_or_default();
            let row = adw::ActionRow::builder()
                .title(node.name.clone())
                .subtitle(value)
                .build();
            let remove = util::icon_button("user-trash-symbolic", "Remove this variable");
            let sender = sender.clone();
            let name = node.name.clone();
            remove.connect_clicked(move |_| sender.input(Msg::RemoveEnv(name.clone())));
            row.add_suffix(&remove);
            env.add(&row);
        }
    }
    body.append(&env);
    body
}

fn health(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let Some(checked) = &model.checked else {
        let label = gtk::Label::new(Some("Checking your setup…"));
        label.add_css_class("dim-label");
        label.set_xalign(0.0);
        body.append(&label);
        return body;
    };
    let problems = checked.problems.clone();
    if problems.is_empty() {
        let label = gtk::Label::new(Some("No problems found."));
        label.add_css_class("title-4");
        label.set_xalign(0.0);
        body.append(&label);
        return body;
    }
    let error_group = util::group(i18n::tr("Errors"));
    let warning_group = util::group(i18n::tr("Warnings"));
    let info_group = util::group(i18n::tr("Notes"));
    for problem in problems.iter() {
        let row = adw::ActionRow::builder()
            .title(problem.title.clone())
            .subtitle(problem.detail.clone())
            .build();
        if let Some(fix) = &problem.fix {
            let button = gtk::Button::with_label(&lint::fix_label(fix));
            button.set_valign(gtk::Align::Center);
            let sender = sender.clone();
            let fix = fix.clone();
            button.connect_clicked(move |_| sender.input(Msg::ApplyFix(fix.clone())));
            row.add_suffix(&button);
        }
        match problem.severity {
            Severity::Error => error_group.add(&row),
            Severity::Warning => warning_group.add(&row),
            Severity::Info => info_group.add(&row),
        }
    }
    if problems.iter().any(|p| p.severity == Severity::Error) {
        body.append(&error_group);
    }
    if problems.iter().any(|p| p.severity == Severity::Warning) {
        body.append(&warning_group);
    }
    if problems.iter().any(|p| p.severity == Severity::Info) {
        body.append(&info_group);
    }
    body
}

/// Buttons that show which preset the config currently matches. They are a
/// single choice, not three commands: clicking one applies it, and the
/// selected button follows the actual values.
fn preset_buttons(
    group: PresetGroup,
    session: &Session,
    sender: &ComponentSender<AppModel>,
) -> adw::ToggleGroup {
    let toggles = adw::ToggleGroup::builder()
        .halign(gtk::Align::Start)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    // The binding does not list Accessible; the widget has it.
    toggles
        .upcast_ref::<gtk::Widget>()
        .update_property(&[gtk::accessible::Property::Label(i18n::tr("Presets"))]);
    for preset in presets::for_group(group) {
        let label = gtk::Label::new(Some(i18n::tr(preset.label)));
        let item = adw::Toggle::builder().name(preset.id).build();
        item.set_child(Some(&label));
        item.set_tooltip(i18n::tr(preset.detail));
        toggles.add(item);
    }
    // Selecting before connecting keeps a rebuild from applying the preset
    // again. The match follows the current config, so a hand edit that
    // breaks the preset leaves no button selected.
    if let Some(active) =
        presets::matching(group, |key| session.get(key).map(|resolved| resolved.value))
    {
        toggles.set_active_name(Some(active.id));
    }
    let sender = sender.clone();
    toggles.connect_active_name_notify(move |toggles| {
        let Some(name) = toggles.active_name() else {
            return;
        };
        if let Some(preset) = presets::for_group(group).find(|preset| preset.id == name.as_str()) {
            sender.input(Msg::ApplyPreset(preset.id));
        }
    });
    toggles
}

/// The character width a shortcut row reserves for its key chord. Longer
/// chords are ellipsized rather than wrapped inside a key name.
fn key_width_chars(key: &str) -> i32 {
    i32::try_from(key.chars().count()).unwrap_or(24).min(24)
}

/// One line for the Home status row: the compositor version, or that niri is
/// not running. `niri msg version` prints a second CLI line, which belongs in
/// diagnostics, not on Home.
fn niri_status() -> String {
    niri::version()
        .and_then(|text| text.lines().next().map(str::to_string))
        .map(|line| {
            line.trim_start_matches("Compositor version: ")
                .trim_end_matches(" (Nixpkgs)")
                .to_string()
        })
        .unwrap_or_else(|| "not running".to_string())
}

fn action_button(
    title: &str,
    subtitle: &str,
    label: &str,
    sender: ComponentSender<AppModel>,
    msg: Msg,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let button = gtk::Button::with_label(label);
    button.set_valign(gtk::Align::Center);
    button.connect_clicked(move |_| sender.input(msg.clone()));
    row.add_suffix(&button);
    row
}

fn summarise_rule(text: &str) -> (String, String) {
    let mut app = None;
    let mut title = None;
    let mut flags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("match app-id=") {
            app = quoted(rest).map(str::to_string);
        } else if let Some(rest) = line.strip_prefix("match title=") {
            title = quoted(rest).map(str::to_string);
        } else if line == "open-floating true" {
            flags.push("floats");
        } else if line.starts_with("geometry-corner-radius") {
            flags.push("rounded corners");
        } else if line.starts_with("opacity") {
            flags.push("transparent");
        }
    }
    let name = app
        .clone()
        .or_else(|| title.clone())
        .unwrap_or_else(|| "All windows".to_string());
    let title_text = match (&app, &title) {
        (Some(app), Some(title)) => format!("{app} · title {title}"),
        (Some(app), None) => app.clone(),
        (None, Some(title)) => format!("Title {title}"),
        (None, None) => "All windows".to_string(),
    };
    let subtitle = if flags.is_empty() {
        "No actions".to_string()
    } else {
        flags.join(" · ")
    };
    let _ = name;
    (title_text, subtitle)
}

fn quoted(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn section_title(section: &str) -> &str {
    match section {
        "layout" => "Layout",
        "overview" => "Overview",
        "cursor" => "Cursor",
        "input" => "Input",
        "animations" => "Animations",
        "hotkey-overlay" => "Shortcut list",
        "General" => "General",
        "" => "General",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::key_width_chars;

    #[test]
    fn a_shortcut_chord_keeps_its_natural_width() {
        assert_eq!(key_width_chars("Mod+Q"), 5);
        assert_eq!(key_width_chars("Mod+Shift+Ctrl+Right"), 20);
    }

    #[test]
    fn a_long_chord_is_capped_instead_of_wrapped() {
        assert_eq!(key_width_chars("Mod+Shift+Ctrl+Page_Down"), 24);
        assert_eq!(key_width_chars(&"x".repeat(200)), 24);
    }
}
