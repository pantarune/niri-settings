//! The first-run welcome flow (design section 6.1).
//!
//! It runs on the Home page until the user finishes or skips it. Every choice
//! is staged in memory. The only write happens on the last step, as one
//! import-like change, so "Nothing is written until the last step" holds.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use relm4::ComponentSender;

use niri_settings_core::Value;
use niri_settings_core::lint;
use niri_settings_core::presets::{self, Preset, PresetGroup};
use niri_settings_core::schemes;
use niri_settings_core::session::Session;

use crate::app::{AppModel, Msg};
use crate::{i18n, preview, util};

const STEP_COUNT: usize = 6;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    Hello,
    Source,
    Look,
    Feel,
    Keyboard,
    Done,
}

impl Step {
    fn number(self) -> usize {
        match self {
            Self::Hello => 1,
            Self::Source => 2,
            Self::Look => 3,
            Self::Feel => 4,
            Self::Keyboard => 5,
            Self::Done => 6,
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "hello" => Some(Self::Hello),
            "source" => Some(Self::Source),
            "look" => Some(Self::Look),
            "feel" => Some(Self::Feel),
            "keyboard" => Some(Self::Keyboard),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Hello => Self::Source,
            Self::Source => Self::Look,
            Self::Look => Self::Feel,
            Self::Feel => Self::Keyboard,
            Self::Keyboard | Self::Done => Self::Done,
        }
    }

    fn back(self) -> Option<Self> {
        match self {
            Self::Hello => None,
            Self::Source => Some(Self::Hello),
            Self::Look => Some(Self::Source),
            Self::Feel => Some(Self::Look),
            Self::Keyboard => Some(Self::Feel),
            Self::Done => Some(Self::Keyboard),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Welcome {
    pub active: bool,
    pub step: Step,
    pub source: Option<&'static str>,
    pub appearance: Option<&'static str>,
    pub animations: Option<&'static str>,
    pub layout: String,
    pub detected_layout: String,
    pub layout_edited: bool,
}

impl Welcome {
    pub fn start(active: bool) -> Self {
        let detected = detected_layout();
        let mut welcome = Self {
            active,
            layout: detected.clone(),
            detected_layout: detected,
            ..Self::default()
        };
        // Screenshot and QA hook: open the flow at a given step.
        if let Some(step) = std::env::var("NIRI_SETTINGS_WELCOME_STEP")
            .ok()
            .and_then(|id| Step::from_id(&id))
        {
            welcome.active = true;
            welcome.step = step;
        }
        welcome
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn next(&mut self) {
        self.step = self.step.next();
    }

    pub fn back(&mut self) {
        if let Some(step) = self.step.back() {
            self.step = step;
        }
    }

    pub fn progress(&self) -> String {
        format!("Step {} of {STEP_COUNT}", self.step.number())
    }

    pub fn set_source(&mut self, source: &'static str) {
        self.source = Some(source);
    }

    pub fn set_preset(&mut self, group: PresetGroup, id: &'static str) {
        match group {
            PresetGroup::Appearance => self.appearance = Some(id),
            PresetGroup::Animations => self.animations = Some(id),
        }
    }

    pub fn set_layout(&mut self, text: String) {
        self.layout_edited = text.trim() != self.detected_layout;
        self.layout = text;
    }

    pub fn scheme(&self) -> Option<&'static schemes::Scheme> {
        self.source.and_then(scheme_for)
    }

    pub fn binds(&self) -> &'static [(&'static str, &'static str)] {
        self.scheme().map(|scheme| scheme.binds).unwrap_or(&[])
    }

    /// The option changes the flow will write on the last step.
    pub fn changes(&self, session: &Session) -> Vec<(String, Value)> {
        let mut changes = Vec::new();
        for id in [self.appearance, self.animations].into_iter().flatten() {
            let Some(preset) = selected_preset(Some(id)) else {
                continue;
            };
            if !preset_differs(preset, session) {
                continue;
            }
            for (key, value) in preset.changes {
                changes.push(((*key).to_string(), value.clone()));
            }
        }
        let layout = self.layout.trim();
        if self.layout_edited && !layout.is_empty() {
            changes.push((
                "input.keyboard.xkb.layout".to_string(),
                Value::String(layout.to_string()),
            ));
        }
        changes
    }
}

pub fn page(model: &AppModel, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = util::page_body();
    let welcome = &model.welcome;
    let session = &model.session;

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let count = gtk::Label::new(Some(&welcome.progress()));
    count.add_css_class("caption");
    count.add_css_class("dim-label");
    top.append(&count);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    top.append(&spacer);
    let skip_label = if welcome.step == Step::Done {
        i18n::tr("Discard setup")
    } else {
        i18n::tr("Skip setup")
    };
    let skip_all = gtk::Button::with_label(skip_label);
    skip_all.add_css_class("flat");
    let sender_skip = sender.clone();
    skip_all.connect_clicked(move |_| sender_skip.input(Msg::WelcomeClose));
    top.append(&skip_all);
    body.append(&top);

    let (title, subtitle) = step_copy(welcome.step);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("title-1");
    heading.set_xalign(0.0);
    heading.set_wrap(true);
    body.append(&heading);
    let intro = gtk::Label::new(Some(subtitle));
    intro.add_css_class("dim-label");
    intro.set_xalign(0.0);
    intro.set_wrap(true);
    body.append(&intro);

    match welcome.step {
        Step::Hello => body.append(&preview_card(&welcome_strip())),
        Step::Source => {
            body.append(&source_buttons(welcome, sender));
            let note = gtk::Label::new(Some(
                "KDE, Hyprland, Sway and Windows use the Super-based set. A full import from Hyprland or Sway is on the Home page after setup.",
            ));
            note.add_css_class("dim-label");
            note.set_wrap(true);
            note.set_xalign(0.0);
            body.append(&note);
        }
        Step::Look => {
            let overrides = preset_changes(welcome.appearance);
            body.append(&preview_card(&preview::appearance_preview_with(
                session, overrides,
            )));
            let presets = util::group(i18n::tr("Preset"));
            presets.add(&preset_toggles(
                PresetGroup::Appearance,
                session,
                welcome.appearance,
                sender,
            ));
            body.append(&presets);
        }
        Step::Feel => {
            let overrides = preset_changes(welcome.animations);
            body.append(&preview_card(&preview::animation_preview_with(
                session, overrides,
            )));
            let presets = util::group(i18n::tr("Preset"));
            presets.add(&preset_toggles(
                PresetGroup::Animations,
                session,
                welcome.animations,
                sender,
            ));
            body.append(&presets);
        }
        Step::Keyboard => body.append(&keyboard_group(welcome, sender)),
        Step::Done => {
            body.append(&summary(model));
            body.append(&shortcuts(model));
        }
    }

    body.append(&nav(welcome.step, sender));
    body
}

fn step_copy(step: Step) -> (&'static str, &'static str) {
    match step {
        Step::Hello => (
            "Welcome to niri settings",
            "niri is a scrolling tiling compositor. Windows sit in a row, and the row scrolls sideways.",
        ),
        Step::Source => (
            "Where are you coming from?",
            "This picks shortcuts you probably know. You can change every one later.",
        ),
        Step::Look => (
            "Choose a look",
            "Gaps and borders, shown on your current setup. Your choice is written when you finish.",
        ),
        Step::Feel => (
            "Choose how motion feels",
            "Animation speed, shown live. Your choice is written when you finish.",
        ),
        Step::Keyboard => (
            "Check your keyboard layout",
            "niri follows your system layout. Change this only if you need a different one.",
        ),
        Step::Done => (
            "You are ready",
            "One last look. Keep writes these choices as one change that you can undo.",
        ),
    }
}

fn preview_card(child: &impl IsA<gtk::Widget>) -> gtk::Frame {
    let frame = gtk::Frame::new(None);
    frame.add_css_class("card");
    frame.set_child(Some(child));
    frame
}

fn nav(step: Step, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::End);
    row.set_margin_top(6);
    if step.back().is_some() {
        let back = gtk::Button::with_label(i18n::tr("Back"));
        let sender = sender.clone();
        back.connect_clicked(move |_| sender.input(Msg::WelcomeBack));
        row.append(&back);
    }
    if step == Step::Done {
        let keep = gtk::Button::with_label(i18n::tr("Keep"));
        keep.add_css_class("suggested-action");
        let sender = sender.clone();
        keep.connect_clicked(move |_| sender.input(Msg::WelcomeApply));
        row.append(&keep);
    } else {
        let skip = gtk::Button::with_label(i18n::tr("Skip"));
        skip.add_css_class("flat");
        let skip_sender = sender.clone();
        skip.connect_clicked(move |_| skip_sender.input(Msg::WelcomeSkip));
        row.append(&skip);
        let next = gtk::Button::with_label(i18n::tr("Continue"));
        next.add_css_class("suggested-action");
        let next_sender = sender.clone();
        next.connect_clicked(move |_| next_sender.input(Msg::WelcomeNext));
        row.append(&next);
    }
    row
}

const SOURCES: &[(&str, &str)] = &[
    ("gnome", "GNOME"),
    ("kde", "KDE"),
    ("hyprland", "Hyprland"),
    ("sway", "Sway"),
    ("macos", "macOS"),
    ("windows", "Windows"),
    ("vim", "Vim-style"),
    ("new", "New to niri"),
];

fn source_buttons(welcome: &Welcome, sender: &ComponentSender<AppModel>) -> adw::WrapBox {
    let wrap = adw::WrapBox::builder()
        .child_spacing(8)
        .line_spacing(8)
        .build();
    let mut first: Option<gtk::ToggleButton> = None;
    for (id, label) in SOURCES {
        let button = gtk::ToggleButton::with_label(i18n::tr(label));
        button.add_css_class("pill");
        if let Some(first) = &first {
            button.set_group(Some(first));
        }
        button.set_active(welcome.source == Some(*id));
        let sender = sender.clone();
        let id = *id;
        button.connect_clicked(move |_| sender.input(Msg::WelcomeSource(id)));
        wrap.append(&button);
        if first.is_none() {
            first = Some(button);
        }
    }
    wrap
}

fn preset_changes(id: Option<&'static str>) -> &'static [(&'static str, Value)] {
    selected_preset(id)
        .map(|preset| preset.changes)
        .unwrap_or(&[])
}

fn preset_toggles(
    group: PresetGroup,
    session: &Session,
    chosen: Option<&'static str>,
    sender: &ComponentSender<AppModel>,
) -> adw::ToggleGroup {
    let toggles = adw::ToggleGroup::builder()
        .halign(gtk::Align::Start)
        .build();
    for preset in presets::for_group(group) {
        let item = adw::Toggle::builder().name(preset.id).build();
        item.set_child(Some(&gtk::Label::new(Some(i18n::tr(preset.label)))));
        item.set_tooltip(i18n::tr(preset.detail));
        toggles.add(item);
    }
    let active = chosen.map(str::to_string).or_else(|| {
        presets::matching(group, |key| session.get(key).map(|resolved| resolved.value))
            .map(|preset| preset.id.to_string())
    });
    if let Some(active) = active {
        toggles.set_active_name(Some(&active));
    }
    let sender = sender.clone();
    toggles.connect_active_name_notify(move |toggles| {
        let Some(name) = toggles.active_name() else {
            return;
        };
        if let Some(preset) = presets::for_group(group).find(|preset| preset.id == name.as_str()) {
            sender.input(Msg::WelcomePreset(group, preset.id));
        }
    });
    toggles
}

fn keyboard_group(welcome: &Welcome, sender: &ComponentSender<AppModel>) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let group = util::group(i18n::tr("Keyboard"));
    let row = adw::ActionRow::builder()
        .title(i18n::tr("Layout"))
        .subtitle(format!(
            "Detected from your locale: {}",
            welcome.detected_layout
        ))
        .build();
    let entry = gtk::Entry::builder()
        .text(&welcome.layout)
        .placeholder_text("us")
        .width_chars(14)
        .valign(gtk::Align::Center)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label(i18n::tr(
        "Keyboard layout",
    ))]);
    let sender = sender.clone();
    entry.connect_changed(move |entry| {
        sender.input(Msg::WelcomeLayout(entry.text().to_string()));
    });
    row.add_suffix(&entry);
    row.set_activatable_widget(Some(&entry));
    group.add(&row);
    body.append(&group);
    let note = gtk::Label::new(Some(
        "For example us, de, or us,ru. Leave it as it is to keep the system layout.",
    ));
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.add_css_class("dim-label");
    body.append(&note);
    body
}

fn summary(model: &AppModel) -> adw::PreferencesGroup {
    let group = util::group(i18n::tr("Your changes"));
    let welcome = &model.welcome;
    let session = &model.session;
    let mut rows = 0;
    if let Some(scheme) = welcome.scheme() {
        group.add(&summary_row(i18n::tr("Shortcuts"), scheme.label));
        rows += 1;
    }
    if let Some(preset) = selected_preset(welcome.appearance)
        && preset_differs(preset, session)
    {
        group.add(&summary_row(i18n::tr("Look"), preset.label));
        rows += 1;
    }
    if let Some(preset) = selected_preset(welcome.animations)
        && preset_differs(preset, session)
    {
        group.add(&summary_row(i18n::tr("Motion"), preset.label));
        rows += 1;
    }
    if welcome.layout_edited && !welcome.layout.trim().is_empty() {
        group.add(&summary_row(
            i18n::tr("Keyboard layout"),
            welcome.layout.trim(),
        ));
        rows += 1;
    }
    if rows == 0 {
        group.add(&summary_row(
            i18n::tr("Nothing yet"),
            i18n::tr("Go back and pick a look or shortcuts, or skip this step."),
        ));
    }
    group
}

fn summary_row(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(value)
        .build()
}

fn shortcuts(model: &AppModel) -> adw::PreferencesGroup {
    let group = util::group(i18n::tr("Five shortcuts to remember"));
    let picks = five_binds(model);
    if picks.is_empty() {
        group.add(&summary_row(
            i18n::tr("No shortcuts yet"),
            i18n::tr("Add some on the Shortcuts page."),
        ));
        return group;
    }
    for (key, action) in picks {
        let row = adw::ActionRow::builder().title(action).build();
        let keys = gtk::Label::new(Some(&key));
        keys.add_css_class("monospace");
        keys.set_valign(gtk::Align::Center);
        keys.set_ellipsize(gtk::pango::EllipsizeMode::End);
        keys.set_max_width_chars(24);
        keys.set_tooltip_text(Some(&key));
        row.add_suffix(&keys);
        group.add(&row);
    }
    group
}

fn five_binds(model: &AppModel) -> Vec<(String, String)> {
    if let Some(scheme) = model.welcome.scheme() {
        return scheme
            .binds
            .iter()
            .take(5)
            .map(|(key, action)| ((*key).to_string(), short_action(action)))
            .collect();
    }
    let binds = lint::collect_binds(&model.session.tree);
    let preferred = [
        "spawn",
        "close-window",
        "focus-column-left",
        "focus-column-right",
        "toggle-overview",
    ];
    let mut out: Vec<(String, String)> = Vec::new();
    for action in preferred {
        if let Some(bind) = binds.iter().find(|bind| bind.action == action) {
            out.push((bind.key.clone(), bind.action_label()));
        }
    }
    for bind in &binds {
        if out.len() >= 5 {
            break;
        }
        if out.iter().any(|(key, _)| key == &bind.key) {
            continue;
        }
        out.push((bind.key.clone(), bind.action_label()));
    }
    out.truncate(5);
    out
}

fn short_action(action: &str) -> String {
    let name = action.split_whitespace().next().unwrap_or(action);
    match name {
        "spawn" | "spawn-sh" => i18n::tr("Open a program").to_string(),
        "close-window" => i18n::tr("Close the window").to_string(),
        "quit" => i18n::tr("Quit niri").to_string(),
        "toggle-overview" => i18n::tr("Show all windows").to_string(),
        "screenshot" => i18n::tr("Take a screenshot").to_string(),
        other => lint::action_label(other),
    }
}

fn selected_preset(id: Option<&'static str>) -> Option<&'static Preset> {
    id.and_then(|id| presets::PRESETS.iter().find(|preset| preset.id == id))
}

fn preset_differs(preset: &Preset, session: &Session) -> bool {
    preset.changes.iter().any(|(key, want)| {
        session
            .get(key)
            .is_none_or(|resolved| !resolved.value.loose_eq(want))
    })
}

fn scheme_for(source: &str) -> Option<&'static schemes::Scheme> {
    let id = match source {
        "gnome" | "kde" | "hyprland" | "sway" | "windows" | "new" => "gnome",
        "macos" => "macos",
        "vim" => "vim",
        _ => return None,
    };
    schemes::scheme(id)
}

fn detected_layout() -> String {
    for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(name)
            && let Some(layout) = layout_from_locale(&value)
        {
            return layout;
        }
    }
    "us".to_string()
}

fn layout_from_locale(raw: &str) -> Option<String> {
    let base = raw.split('.').next()?.split('@').next()?.trim();
    if base.is_empty() || base == "C" || base == "POSIX" {
        return None;
    }
    let (lang, country) = match base.split_once('_') {
        Some((lang, country)) => (lang, Some(country)),
        None => (base, None),
    };
    let lang = lang.to_ascii_lowercase();
    let country = country.map(str::to_ascii_uppercase);
    let layout = match (lang.as_str(), country.as_deref()) {
        ("en", Some("GB")) => "gb",
        ("en", _) => "us",
        ("pt", Some("BR")) => "br",
        ("de", Some("CH")) | ("fr", Some("CH")) => "ch",
        ("de", Some("AT")) => "at",
        ("fr", Some("BE")) => "be",
        ("fr", Some("CA")) => "ca",
        ("sv", _) => "se",
        ("da", _) => "dk",
        ("nb" | "nn" | "no", _) => "no",
        ("cs", _) => "cz",
        ("el", _) => "gr",
        ("uk", _) => "ua",
        ("ja", _) => "jp",
        ("ko", _) => "kr",
        ("zh", _) => "cn",
        ("he", _) => "il",
        ("ar", _) => "ara",
        (lang, _) if LAYOUTS.contains(&lang) => lang,
        _ => "us",
    };
    Some(layout.to_string())
}

const LAYOUTS: &[&str] = &[
    "us", "gb", "de", "fr", "es", "it", "pt", "br", "ru", "pl", "nl", "se", "no", "fi", "dk", "cz",
    "gr", "hu", "ro", "bg", "ua", "ca", "ch", "be", "at", "tr", "jp", "kr", "cn", "il", "ara",
];

fn welcome_strip() -> gtk::DrawingArea {
    let phase = Rc::new(Cell::new(0.0f64));
    let area = gtk::DrawingArea::builder().content_height(150).build();
    let tick = phase.clone();
    area.add_tick_callback(move |area, clock| {
        // One loop every six seconds, driven by the frame clock.
        tick.set((clock.frame_time() as f64 / 1_000_000.0 % 6.0) / 6.0);
        area.queue_draw();
        gtk::glib::ControlFlow::Continue
    });
    let draw_phase = phase.clone();
    area.set_draw_func(move |_, cr, width, height| {
        let width = width as f64;
        let height = height as f64;
        cr.set_source_rgba(0.11, 0.11, 0.12, 1.0);
        cr.paint().ok();
        let column = 132.0;
        let gap = 14.0;
        let span = column + gap;
        let shift = (draw_phase.get() * span) % span;
        let y = 22.0;
        let column_height = height - 44.0;
        let mut index = -1i32;
        while 24.0 + index as f64 * span - shift < width {
            let x = 24.0 + index as f64 * span - shift;
            preview::rounded_rect(cr, x, y, column, column_height, 10.0);
            cr.set_source_rgba(0.20, 0.21, 0.23, 1.0);
            cr.fill().ok();
            cr.set_source_rgba(0.65, 0.67, 0.70, 0.42);
            for line in 0..3 {
                let line_y = y + 26.0 + line as f64 * 13.0;
                let line_width = if line == 0 {
                    (column - 28.0) * 0.55
                } else {
                    column - 28.0
                };
                cr.rectangle(x + 14.0, line_y, line_width, 4.0);
            }
            cr.fill().ok();
            index += 1;
        }
        // A focus ring on the column nearest the middle.
        let nearest = ((width / 2.0 - 24.0 + shift) / span).round();
        let x = 24.0 + nearest * span - shift;
        cr.set_source_rgba(0.50, 0.78, 1.0, 1.0);
        cr.set_line_width(2.0);
        preview::rounded_rect(
            cr,
            x - 2.0,
            y - 2.0,
            column + 4.0,
            column_height + 4.0,
            12.0,
        );
        cr.stroke().ok();
    });
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_walk_forward_and_back() {
        let mut step = Step::Hello;
        for _ in 0..5 {
            step = step.next();
        }
        assert_eq!(step, Step::Done);
        assert_eq!(Step::Done.next(), Step::Done);
        assert_eq!(Step::Done.back(), Some(Step::Keyboard));
        assert_eq!(Step::Hello.back(), None);
        assert_eq!(Step::from_id("keyboard"), Some(Step::Keyboard));
        assert_eq!(Step::from_id("unknown"), None);
    }

    #[test]
    fn locales_map_to_keyboard_layouts() {
        assert_eq!(layout_from_locale("de_DE.UTF-8").as_deref(), Some("de"));
        assert_eq!(layout_from_locale("en_US.UTF-8").as_deref(), Some("us"));
        assert_eq!(layout_from_locale("en_GB.UTF-8").as_deref(), Some("gb"));
        assert_eq!(layout_from_locale("pt_BR.UTF-8").as_deref(), Some("br"));
        assert_eq!(layout_from_locale("fr_CH.UTF-8").as_deref(), Some("ch"));
        assert_eq!(layout_from_locale("ja_JP.UTF-8").as_deref(), Some("jp"));
        assert_eq!(layout_from_locale("tr_TR.UTF-8").as_deref(), Some("tr"));
        assert_eq!(layout_from_locale("C.UTF-8"), None);
        assert_eq!(layout_from_locale("POSIX"), None);
        assert_eq!(layout_from_locale(""), None);
    }

    #[test]
    fn sources_pick_a_known_scheme() {
        assert_eq!(scheme_for("gnome").map(|scheme| scheme.id), Some("gnome"));
        assert_eq!(scheme_for("macos").map(|scheme| scheme.id), Some("macos"));
        assert_eq!(scheme_for("vim").map(|scheme| scheme.id), Some("vim"));
        assert_eq!(scheme_for("unknown").map(|scheme| scheme.id), None);
    }

    #[test]
    fn editing_the_layout_marks_it_changed() {
        let mut welcome = Welcome::start(false);
        welcome.detected_layout = "de".to_string();
        welcome.layout = "de".to_string();
        welcome.set_layout("de".to_string());
        assert!(!welcome.layout_edited);
        welcome.set_layout("us".to_string());
        assert!(welcome.layout_edited);
    }
}
