//! The GTK4 + libadwaita application shell.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use relm4::prelude::*;

use niri_settings_core::Value;
use niri_settings_core::import::ImportResult;
use niri_settings_core::lint::Fix;
use niri_settings_core::niri;
use niri_settings_core::presets::PresetGroup;
use niri_settings_core::schema::{PAGES, Page};
use niri_settings_core::session::Session;
use niri_settings_core::watch::{self, ConfigWatcher};

use crate::jobs::{self, Job};
use crate::util::Prefs;
use crate::welcome::{self, Welcome};
use crate::{nav, pages, toasts};

mod dialogs;
mod displays;
mod lists;
mod shortcuts;

pub const APP_ID: &str = "io.github.niri_settings.NiriSettings";

/// The app-wide menu. Only commands that have no natural page belong here;
/// import, setup checks and Copy as Nix live where they apply.
const MENU_SECTIONS: &[&[(&str, &str)]] = &[
    &[("Undo", "app.undo"), ("Redo", "app.redo")],
    &[("History", "app.history")],
    &[
        ("Preferences", "app.preferences"),
        ("About niri settings", "app.about"),
    ],
];

pub struct Init {
    pub session: Session,
    pub page: Option<String>,
    pub location: niri_settings_core::Location,
}

pub struct AppModel {
    pub(crate) session: Session,
    pub(crate) page: Page,
    pub(crate) search: String,
    pub(crate) shortcut_search: String,
    pub(crate) config_file: usize,
    pub(crate) prefs: Prefs,
    pub(crate) welcome: Welcome,
    pub(crate) ui: Ui,
    pub(crate) root: adw::ApplicationWindow,
    /// Where the config came from (user file, system file, explicit path).
    location: niri_settings_core::Location,
    /// The page last shown in each top bar section, by section id.
    last_pages: std::collections::HashMap<&'static str, Page>,
    /// The page and search text on screen, to keep scroll and focus when
    /// the same page is rebuilt.
    shown: Option<(Page, String)>,
    /// A write is running on the worker thread.
    busy: Rc<Cell<bool>>,
    /// Changes that arrived during a write, run in order afterwards.
    queue: std::collections::VecDeque<Msg>,
    /// Setup check results for the current config, once known.
    pub(crate) checked: Option<jobs::Checked>,
    checking: bool,
    _watch: Option<ConfigWatcher>,
    /// The paths the watcher is watching. Includes can add or remove files.
    watched_paths: Vec<PathBuf>,
    last_watch_error: Option<String>,
    display_guard: Rc<Cell<bool>>,
}

pub struct Ui {
    pub stack: adw::ViewStack,
    pub sections: Vec<SectionUi>,
    pub search: gtk::SearchEntry,
    pub search_bar: gtk::SearchBar,
    pub window_title: adw::WindowTitle,
    pub header: adw::HeaderBar,
    /// Shown while a write takes longer than a moment.
    pub spinner: adw::Spinner,
    /// Shown while the config is the system-wide file.
    pub system_banner: adw::Banner,
    /// The change notice on screen: id, kind and toast.
    pub notice: Rc<std::cell::RefCell<Option<(u64, toasts::Kind, adw::Toast)>>>,
    pub notice_id: Cell<u64>,
    pub toast_overlay: adw::ToastOverlay,
    pub selecting: Rc<Cell<bool>>,
}

/// One top bar section: an optional row of page buttons and the page itself.
pub struct SectionUi {
    pub id: &'static str,
    pub toggles: Option<adw::ToggleGroup>,
    pub holder: gtk::Box,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Navigate(Page),
    Section(&'static str),
    /// Copy the system-wide config to the user's config path.
    MakePersonalCopy,
    /// A write finished on the worker thread.
    JobDone(jobs::Finished),
    /// Setup checks finished on the worker thread.
    Checked(jobs::Checked),
    Search(String),
    ShortcutSearch(String),
    Set(String, Value),
    SetRebuild(String, Value),
    Reset(String),
    Undo,
    Redo,
    Reload,
    Toast(String),
    Error(String),
    OpenHistory,
    Restore(u64),
    OpenPreferences,
    ToggleNames(bool),
    WelcomeNext,
    WelcomeBack,
    WelcomeSkip,
    WelcomeClose,
    WelcomeSource(&'static str),
    WelcomePreset(PresetGroup, &'static str),
    WelcomeLayout(String),
    WelcomeApply,
    OpenAddBind,
    OpenEditBind(String),
    ApplyScheme(&'static str),
    SchemeConfirmed(&'static str),
    AddBind(String, String),
    ReplaceBind(String, String, String),
    RemoveBind(String),
    RemoveBindConfirmed(String),
    OpenAddRule(Option<String>),
    OpenEditRule(usize),
    AddRule(String),
    EditRule(usize, String),
    RemoveRule(usize),
    PickWindow,
    AddRuleFor(String, String),
    OpenAddStartup,
    AddStartup(String),
    RemoveStartup(usize),
    OpenAddEnv,
    SetEnv(String, String),
    RemoveEnv(String),
    OpenAddWorkspace,
    AddWorkspace(String, String),
    RemoveWorkspace(String),
    ApplyFix(Fix),
    ApplyPreset(&'static str),
    OpenImport,
    ImportPreview(ImportResult, PathBuf),
    ImportApply(ImportResult),
    ConfigFile(usize),
    WriteRaw(PathBuf, String),
    OpenConfig,
    OpenAbout,
    CopyNix,
    PollExternal,
    DisplayChange {
        name: String,
        field: String,
        value: Value,
    },
    DisplayKeep {
        name: String,
        field: String,
        value: Value,
    },
    DisplayReverted,
}

impl AppModel {
    fn toast(&self, text: impl AsRef<str>) {
        self.notify(toasts::Event::Plain, text.as_ref(), None);
    }

    fn toast_undo(&self, text: impl AsRef<str>, sender: &ComponentSender<Self>) {
        self.notify(toasts::Event::Changed, text.as_ref(), Some(sender));
    }

    /// Show one notice, merged with the one on screen when they are the
    /// same kind (design section 5.1). libadwaita queues toasts, so the old
    /// notice is dismissed first, or quick changes would line up.
    fn notify(&self, event: toasts::Event, text: &str, sender: Option<&ComponentSender<Self>>) {
        let showing = self.ui.notice.borrow().as_ref().map(|notice| notice.1);
        let kind = toasts::next(showing, event);
        let old = self.ui.notice.borrow_mut().take();
        if let Some((_, _, toast)) = old {
            toast.dismiss();
        }
        let toast = adw::Toast::new(&toasts::title(kind, text));
        toast.set_timeout(5);
        if let (Some(button), Some(sender)) = (toasts::button(kind), sender) {
            let (label, message, count) = match button {
                toasts::Button::Undo(count) => (crate::i18n::tr("Undo"), Msg::Undo, count),
                toasts::Button::Redo(count) => (crate::i18n::tr("Redo"), Msg::Redo, count),
            };
            toast.set_button_label(Some(label));
            let sender = sender.clone();
            toast.connect_button_clicked(move |_| {
                // Each step goes through the write queue, in order.
                for _ in 0..count {
                    sender.input(message.clone());
                }
            });
        }
        let id = self.ui.notice_id.get() + 1;
        self.ui.notice_id.set(id);
        let slot = self.ui.notice.clone();
        toast.connect_dismissed(move |_| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().map(|notice| notice.0) == Some(id) {
                *slot = None;
            }
        });
        *self.ui.notice.borrow_mut() = Some((id, kind, toast.clone()));
        self.ui.toast_overlay.add_toast(toast);
    }

    fn show_error(&self, text: impl AsRef<str>) {
        let text = text.as_ref();
        if text.contains('\n') || text.len() > 160 {
            let dialog = adw::AlertDialog::builder()
                .heading("The change was not applied")
                .body(text)
                .build();
            dialog.add_response("close", "Close");
            dialog.set_default_response(Some("close"));
            dialog.present(Some(&self.root));
            return;
        }
        let toast = adw::Toast::new(text);
        toast.set_timeout(8);
        toast.set_priority(adw::ToastPriority::High);
        self.ui.toast_overlay.add_toast(toast);
    }

    pub(crate) fn search_active(&self) -> bool {
        !self.search.trim().is_empty()
    }

    /// The page widget on screen: a scrolled window in a section holder.
    fn shown_page(&self) -> Option<gtk::Widget> {
        self.ui
            .sections
            .iter()
            .find_map(|section| section.holder.first_child())
    }

    fn rewatch(&mut self) {
        self.watched_paths = self.session.watched_paths();
        self._watch = watch::watch(&self.watched_paths);
    }

    /// Rebuild the watcher only when an include change altered the file set.
    fn sync_watch(&mut self) {
        if self.watched_paths != self.session.watched_paths() {
            self.rewatch();
        }
    }

    fn refresh(&mut self, sender: &ComponentSender<Self>) {
        self.sync_watch();
        let same_page = self.shown.as_ref() == Some(&(self.page, self.search.clone()));
        let old = self.shown_page().filter(|_| same_page);
        let scroll = old
            .as_ref()
            .and_then(|page| page.downcast_ref::<gtk::ScrolledWindow>())
            .map(|scrolled| scrolled.vadjustment().value());
        let focus = old
            .as_ref()
            .and_then(|page| crate::keep::focus_mark(&self.root, page));
        self.shown = Some((self.page, self.search.clone()));

        for section in &self.ui.sections {
            while let Some(child) = section.holder.first_child() {
                section.holder.remove(&child);
            }
        }
        let page = pages::build(self, sender);
        let info = self.page.info();
        let section = nav::section_of(self.page);
        self.ui.selecting.set(true);
        if let Some(ui) = self.ui.sections.iter().find(|ui| ui.id == section.id) {
            ui.holder.append(&page);
            if let Some(toggles) = &ui.toggles {
                toggles.set_active_name(Some(info.id));
            }
        }
        self.ui.stack.set_visible_child_name(section.id);
        self.ui.selecting.set(false);
        let welcome_home = self.page == Page::Home && self.welcome.active;
        let title = if welcome_home {
            crate::i18n::tr("Setup").to_string()
        } else {
            crate::i18n::tr_owned(info.title)
        };
        self.ui.window_title.set_title(&title);
        let subtitle = if welcome_home {
            self.welcome.progress()
        } else if self.search_active() {
            crate::i18n::tr("Search results").to_string()
        } else {
            crate::i18n::tr_owned(info.subtitle)
        };
        self.ui.window_title.set_subtitle(&subtitle);

        if let Some(value) = scroll
            && let Some(scrolled) = page.downcast_ref::<gtk::ScrolledWindow>()
        {
            crate::keep::restore_scroll(scrolled, value);
        }
        if let Some(mark) = focus {
            crate::keep::restore_focus(&page, &mark);
        }
    }

    /// Print the minimum width of the top bar and of every page. The window
    /// minimum and the breakpoint in `init` must stay above the largest value.
    fn print_page_widths(&mut self, sender: &ComponentSender<Self>) {
        // The top bar fits from its minimum, but tab labels only show in
        // full from its natural width; the breakpoint follows the natural one.
        let (header, natural, _, _) = self.ui.header.measure(gtk::Orientation::Horizontal, -1);
        println!(
            "{:<12} {header} px (full labels from {natural} px)",
            "top bar"
        );
        let start = self.page;
        for info in PAGES {
            self.page = info.page;
            self.refresh(sender);
            let section = nav::section_of(info.page);
            let width = self
                .ui
                .sections
                .iter()
                .find(|ui| ui.id == section.id)
                .map(|ui| {
                    ui.holder.parent().map_or(0, |parent| {
                        parent.measure(gtk::Orientation::Horizontal, -1).0
                    })
                })
                .unwrap_or(0);
            println!("{:<12} {width} px", info.id);
        }
        self.page = start;
        self.refresh(sender);
    }

    fn navigate(&mut self, page: Page, sender: &ComponentSender<Self>) {
        let searching = self.search_active();
        // Selecting the button of the current page again after a rebuild
        // must not loop.
        if page == self.page && !searching {
            return;
        }
        self.page = page;
        self.last_pages.insert(nav::section_of(page).id, page);
        if searching {
            // Clear the filter first, so the `Search("")` message that
            // `set_text` sends later has no effect.
            self.search.clear();
            self.ui.search.set_text("");
            self.ui.search_bar.set_search_mode(false);
        }
        self.refresh(sender);
    }

    /// Show a section at the page last seen there, or its first page.
    fn show_section(&mut self, id: &'static str, sender: &ComponentSender<Self>) {
        let Some(section) = nav::section_by_id(id) else {
            return;
        };
        if nav::section_of(self.page).id == id && !self.search_active() {
            return;
        }
        let page = self.last_pages.get(id).copied().unwrap_or(section.pages[0]);
        self.navigate(page, sender);
    }

    fn apply(&mut self, key: String, value: Value, sender: &ComponentSender<Self>, rebuild: bool) {
        self.start(
            Job::Set {
                key,
                value,
                rebuild,
            },
            sender,
        );
    }

    fn reset(&mut self, key: String, sender: &ComponentSender<Self>) {
        self.start(Job::Reset(key), sender);
    }

    /// Run a write on the worker thread. The window keeps drawing; a spinner
    /// appears if the write takes longer than 200 ms.
    fn start(&mut self, job: Job, sender: &ComponentSender<Self>) {
        self.busy.set(true);
        let busy = self.busy.clone();
        let spinner = self.ui.spinner.clone();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            if busy.get() {
                spinner.set_visible(true);
            }
        });
        let sender = sender.clone();
        jobs::run(self.session.clone(), job, move |finished| {
            sender.input(Msg::JobDone(finished));
        });
    }

    /// Take the session back from the worker, report, then run what waited.
    fn finish(&mut self, finished: jobs::Finished, sender: &ComponentSender<Self>) {
        let Some(outcome) = finished.take() else {
            return;
        };
        self.session = outcome.session;
        self.busy.set(false);
        self.ui.spinner.set_visible(false);
        match (outcome.job, outcome.result) {
            (Job::Set { rebuild, .. }, Ok(applied)) => {
                self.toast_undo(&applied.summary, sender);
                if rebuild {
                    self.refresh(sender);
                }
            }
            (Job::Set { .. }, Err(niri_settings_core::ApplyError::NoChange)) => {}
            (Job::Set { .. } | Job::Reset(_), Err(error)) => {
                self.show_error(format!("{error}"));
                self.refresh(sender);
            }
            (Job::Reset(_), Ok(applied)) => {
                self.toast_undo(&applied.summary, sender);
                self.refresh(sender);
            }
            (Job::Undo, Ok(applied)) => {
                self.notify(toasts::Event::Undone, &applied.summary, Some(sender));
                self.refresh(sender);
            }
            (Job::Redo, Ok(applied)) => {
                self.notify(toasts::Event::Redone, &applied.summary, Some(sender));
                self.refresh(sender);
            }
            (Job::Undo | Job::Redo, Err(error)) => self.show_error(format!("{error}")),
        }
        while !self.busy.get()
            && let Some(message) = self.queue.pop_front()
        {
            self.update_inner(message, sender.clone());
        }
    }

    /// Messages that only read the session or change the view. Everything
    /// else waits while a write runs, so no change is made on an old copy.
    fn runs_while_busy(message: &Msg) -> bool {
        matches!(
            message,
            Msg::Navigate(_)
                | Msg::Section(_)
                | Msg::Search(_)
                | Msg::ShortcutSearch(_)
                | Msg::Toast(_)
                | Msg::Error(_)
                | Msg::OpenHistory
                | Msg::OpenPreferences
                | Msg::ToggleNames(_)
                | Msg::WelcomeNext
                | Msg::WelcomeBack
                | Msg::WelcomeSkip
                | Msg::WelcomeClose
                | Msg::WelcomeSource(_)
                | Msg::WelcomePreset(_, _)
                | Msg::WelcomeLayout(_)
                | Msg::OpenAddBind
                | Msg::OpenEditBind(_)
                | Msg::OpenAddRule(_)
                | Msg::OpenEditRule(_)
                | Msg::OpenAddStartup
                | Msg::OpenAddEnv
                | Msg::OpenAddWorkspace
                | Msg::OpenImport
                | Msg::ConfigFile(_)
                | Msg::OpenConfig
                | Msg::OpenAbout
                | Msg::CopyNix
                | Msg::JobDone(_)
                | Msg::Checked(_)
        )
    }

    /// Copy `/etc/niri/config.kdl` to the user path and switch to it. niri
    /// prefers the user file, so it is asked to load the config again; the
    /// content is the same, so nothing on screen changes.
    fn make_personal_copy(&mut self, sender: &ComponentSender<Self>) {
        let niri_settings_core::Location::System { system, user } = &self.location else {
            return;
        };
        let (system, user) = (system.clone(), user.clone());
        if let Err(error) =
            niri_settings_core::make_personal_copy(&system, &user, self.session.niri_available)
        {
            self.show_error(error);
            return;
        }
        let location = niri_settings_core::Location::User(user.clone());
        match niri_settings_core::location::open(
            &location,
            niri_settings_core::SessionOptions::default(),
        ) {
            Ok(session) => {
                self.session = session;
                self.location = location;
                self.checked = None;
                self.ui.system_banner.set_revealed(false);
                niri::reload_config();
                self.toast(format!(
                    "Made a personal copy at {}. niri uses it from now on.",
                    user.display()
                ));
                self.refresh(sender);
            }
            Err(error) => self.show_error(error.to_string()),
        }
    }

    /// Run the setup checks in the background when the config changed.
    fn maybe_check(&mut self, sender: &ComponentSender<Self>) {
        if self.checking || self.busy.get() {
            return;
        }
        let current = jobs::fingerprint(&self.session.tree);
        if self.checked.as_ref().map(|checked| checked.fingerprint) == Some(current) {
            return;
        }
        self.checking = true;
        let sender = sender.clone();
        jobs::check(self.session.tree.clone(), move |checked| {
            sender.input(Msg::Checked(checked));
        });
    }

    fn checks_done(&mut self, checked: jobs::Checked, sender: &ComponentSender<Self>) {
        self.checking = false;
        if checked.fingerprint != jobs::fingerprint(&self.session.tree) {
            // The config changed meanwhile; `update` starts a new check.
            return;
        }
        let count = checked
            .problems
            .iter()
            .filter(|problem| problem.severity != niri_settings_core::lint::Severity::Info)
            .count();
        self.checked = Some(checked);
        if let Some(child) = self.ui.stack.child_by_name("advanced") {
            let page = self.ui.stack.page(&child);
            page.set_badge_number(count as u32);
            page.set_needs_attention(count > 0);
        }
        if matches!(self.page, Page::Home | Page::Health) {
            self.refresh(sender);
        }
    }
}

impl SimpleComponent for AppModel {
    type Init = Init;
    type Input = Msg;
    type Output = ();
    type Root = adw::ApplicationWindow;
    type Widgets = ();

    fn init_root() -> Self::Root {
        adw::ApplicationWindow::builder().build()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let session = init.session;
        root.set_title(Some("niri settings"));
        root.set_default_size(1040, 760);

        let selecting = Rc::new(Cell::new(false));

        // Top bar: search, section switcher, menu.
        let stack = adw::ViewStack::new();
        let mut sections = Vec::new();
        for section in nav::SECTIONS {
            let section_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let toggles = (section.pages.len() > 1).then(|| {
                let group = adw::ToggleGroup::builder()
                    .halign(gtk::Align::Center)
                    .margin_top(12)
                    .margin_bottom(6)
                    .margin_start(12)
                    .margin_end(12)
                    .build();
                // The binding does not list Accessible; the widget has it.
                group.upcast_ref::<gtk::Widget>().update_property(&[
                    gtk::accessible::Property::Label(crate::i18n::tr(section.title)),
                ]);
                for page in section.pages {
                    let info = page.info();
                    let label = gtk::Label::new(Some(&crate::i18n::tr_owned(info.title)));
                    // Shorten long names in a narrow window instead of
                    // pushing the last button off the edge.
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    label.set_width_chars(4);
                    // Room inside each page button, so labels do not run into
                    // the next button.
                    label.set_margin_start(12);
                    label.set_margin_end(12);
                    let toggle = adw::Toggle::builder().name(info.id).build();
                    toggle.set_child(Some(&label));
                    toggle.set_tooltip(&crate::i18n::tr_owned(info.subtitle));
                    group.add(toggle);
                }
                let sender = sender.clone();
                let selecting = selecting.clone();
                group.connect_active_name_notify(move |group| {
                    if selecting.get() {
                        return;
                    }
                    if let Some(page) = group.active_name().as_deref().and_then(Page::from_id) {
                        sender.input(Msg::Navigate(page));
                    }
                });
                // Scroll sideways instead of widening the window.
                let scroll = gtk::ScrolledWindow::builder()
                    .child(&group)
                    .hscrollbar_policy(gtk::PolicyType::Automatic)
                    .vscrollbar_policy(gtk::PolicyType::Never)
                    .propagate_natural_height(true)
                    .build();
                section_box.append(&scroll);
                group
            });
            let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
            holder.set_vexpand(true);
            section_box.append(&holder);
            stack.add_titled_with_icon(
                &section_box,
                Some(section.id),
                crate::i18n::tr(section.title),
                section.icon,
            );
            sections.push(SectionUi {
                id: section.id,
                toggles,
                holder,
            });
        }

        let header = adw::HeaderBar::new();
        let switcher = adw::ViewSwitcher::builder()
            .stack(&stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();
        let window_title = adw::WindowTitle::new(crate::i18n::tr("Home"), "");

        let search = gtk::SearchEntry::builder()
            .placeholder_text(crate::i18n::tr("Search settings"))
            .hexpand(true)
            .build();
        let search_clamp = adw::Clamp::builder()
            .maximum_size(480)
            .child(&search)
            .build();
        let search_bar = gtk::SearchBar::builder()
            .child(&search_clamp)
            // Escape or the search button closes it; GTK's own close
            // button has no accessible name.
            .show_close_button(false)
            .build();
        search_bar.connect_entry(&search);
        // Search and the menu sit beside the centered tabs, not in the
        // window corners. niri tiles windows with gaps, so a corner is not a
        // screen edge; on a wide window it is just far from the content
        // (Fitts's law). Below the breakpoint the tabs move to the bottom and
        // a second pair of buttons takes the usual edge places.
        let search_button = |visible: bool| {
            let button = gtk::ToggleButton::builder()
                .icon_name("system-search-symbolic")
                .tooltip_text(crate::i18n::tr("Search settings"))
                .visible(visible)
                .build();
            button.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr(
                "Search settings",
            ))]);
            button
                .bind_property("active", &search_bar, "search-mode-enabled")
                .bidirectional()
                .sync_create()
                .build();
            button
        };
        let center_search = search_button(true);
        let edge_search = search_button(false);
        header.pack_start(&edge_search);

        let menu = gio::Menu::new();
        for section in MENU_SECTIONS {
            let group = gio::Menu::new();
            for (label, action) in *section {
                group.append(Some(crate::i18n::tr(label)), Some(action));
            }
            menu.append_section(None, &group);
        }
        let menu_button = |visible: bool| {
            let button = gtk::MenuButton::builder()
                .icon_name("open-menu-symbolic")
                .menu_model(&menu)
                .tooltip_text(crate::i18n::tr("Main menu"))
                .visible(visible)
                // F10 opens the menu that is on screen.
                .primary(visible)
                .build();
            button.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr(
                "Main menu",
            ))]);
            button
        };
        let center_menu = menu_button(true);
        let edge_menu = menu_button(false);
        header.pack_end(&edge_menu);

        // [search]  tabs  [menu], centered as one group. The extra margin
        // keeps search from reading as another tab.
        center_search.set_margin_end(12);
        center_menu.set_margin_start(12);
        let center = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        center.append(&center_search);
        center.append(&switcher);
        center.append(&center_menu);
        header.set_title_widget(Some(&center));
        let spinner = adw::Spinner::builder()
            .visible(false)
            .tooltip_text(crate::i18n::tr("Saving"))
            .build();
        header.pack_end(&spinner);

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&stack));
        let switcher_bar = adw::ViewSwitcherBar::builder().stack(&stack).build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.add_top_bar(&search_bar);
        let system_banner = adw::Banner::builder()
            .title(crate::i18n::tr(
                "This is the system-wide config. Make a personal copy to change it.",
            ))
            .button_label(crate::i18n::tr("Make a personal copy"))
            .revealed(init.location.is_system())
            .build();
        {
            let sender = sender.clone();
            system_banner.connect_button_clicked(move |_| sender.input(Msg::MakePersonalCopy));
        }
        toolbar.add_top_bar(&system_banner);
        toolbar.set_content(Some(&toast_overlay));
        toolbar.add_bottom_bar(&switcher_bar);
        root.set_content(Some(&toolbar));

        // On narrow windows the sections move to a bar at the bottom and the
        // header shows the page title (design section 5.1). Pages must stay
        // under 360 px; check with `NIRI_SETTINGS_MEASURE=1`.
        root.set_size_request(360, 300);
        // The tabs are all as wide as the widest label, which depends on the
        // language and text size. Switch layouts where the centered group
        // stops fitting with full labels, measured now, never below 600 sp.
        // The header's natural width centers the group perfectly, with equal
        // room on both sides. It still fits off-center: the group plus the
        // window controls, which is the average of the two natural widths.
        let (_, header_natural, _, _) = header.measure(gtk::Orientation::Horizontal, -1);
        let (_, center_natural, _, _) = center.measure(gtk::Orientation::Horizontal, -1);
        let fits = (header_natural + center_natural) / 2;
        let condition = format!("max-width: {}px or max-width: 600sp", fits.max(1) - 1);
        let breakpoint = adw::Breakpoint::new(
            adw::BreakpointCondition::parse(&condition).expect("valid breakpoint"),
        );
        breakpoint.add_setter(&switcher_bar, "reveal", Some(&true.to_value()));
        breakpoint.add_setter(&header, "title-widget", Some(&window_title.to_value()));
        breakpoint.add_setter(&edge_search, "visible", Some(&true.to_value()));
        breakpoint.add_setter(&edge_menu, "visible", Some(&true.to_value()));
        breakpoint.add_setter(&edge_menu, "primary", Some(&true.to_value()));
        breakpoint.add_setter(&center_menu, "primary", Some(&false.to_value()));
        root.add_breakpoint(breakpoint);

        let app = relm4::main_adw_application();
        add_action(&app, "undo", sender.clone(), Msg::Undo);
        add_action(&app, "redo", sender.clone(), Msg::Redo);
        add_action(&app, "history", sender.clone(), Msg::OpenHistory);
        add_action(&app, "preferences", sender.clone(), Msg::OpenPreferences);
        add_action(&app, "about", sender.clone(), Msg::OpenAbout);

        let sender_stack = sender.clone();
        let selecting_stack = selecting.clone();
        stack.connect_visible_child_name_notify(move |stack| {
            if selecting_stack.get() {
                return;
            }
            if let Some(section) = stack
                .visible_child_name()
                .as_deref()
                .and_then(nav::section_by_id)
            {
                sender_stack.input(Msg::Section(section.id));
            }
        });
        let sender_search = sender.clone();
        search.connect_search_changed(move |entry| {
            sender_search.input(Msg::Search(entry.text().to_string()));
        });
        // Closing the search bar clears the search.
        let search_clear = search.clone();
        search_bar.connect_search_mode_enabled_notify(move |bar| {
            if !bar.is_search_mode() && !search_clear.text().is_empty() {
                search_clear.set_text("");
            }
        });

        let prefs = Prefs::load();
        let first_run = !prefs.welcome_seen && init.page.is_none();
        let page = init
            .page
            .as_deref()
            .and_then(Page::from_id)
            .unwrap_or(Page::Home);
        let welcome = Welcome::start(first_run);
        let watched_paths = session.watched_paths();
        let watcher = watch::watch(&watched_paths);

        let model = AppModel {
            session,
            page,
            search: String::new(),
            shortcut_search: String::new(),
            config_file: 0,
            prefs,
            welcome,
            ui: Ui {
                stack,
                sections,
                search,
                search_bar,
                window_title,
                header,
                spinner,
                system_banner,
                notice: Rc::new(std::cell::RefCell::new(None)),
                notice_id: Cell::new(0),
                toast_overlay,
                selecting,
            },
            root: root.clone(),
            last_pages: std::collections::HashMap::from([(nav::section_of(page).id, page)]),
            shown: None,
            location: init.location,
            busy: Rc::new(Cell::new(false)),
            queue: std::collections::VecDeque::new(),
            checked: None,
            checking: false,
            _watch: watcher,
            watched_paths,
            last_watch_error: None,
            display_guard: Rc::new(Cell::new(false)),
        };

        let widgets = ();
        let mut model = model;
        if let Some(message) = displays::recover_pending_output() {
            model.toast(message);
        }
        model.refresh(&sender);
        model.maybe_check(&sender);
        if std::env::var_os("NIRI_SETTINGS_MEASURE").is_some() {
            model.print_page_widths(&sender);
            std::process::exit(0);
        }

        crate::shot::schedule(&model.root);
        let sender_watch = sender.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(600), move || {
            sender_watch.input(Msg::PollExternal);
            gtk::glib::ControlFlow::Continue
        });

        // Keyboard shortcuts on the window.
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let sender_keys = sender.clone();
        let search_widget = model.ui.search.clone();
        let search_bar_keys = model.ui.search_bar.clone();
        controller.connect_key_pressed(move |_, key, _, state| {
            let control = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if control && key == gtk::gdk::Key::z {
                if shift {
                    sender_keys.input(Msg::Redo);
                } else {
                    sender_keys.input(Msg::Undo);
                }
                return gtk::glib::Propagation::Stop;
            }
            if control && key == gtk::gdk::Key::f {
                search_bar_keys.set_search_mode(true);
                search_widget.grab_focus();
                return gtk::glib::Propagation::Stop;
            }
            if control && key == gtk::gdk::Key::comma {
                sender_keys.input(Msg::OpenPreferences);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(controller);

        root.present();
        if let Some(recovery) = model.session.recovery.take() {
            if recovery.conflicts.is_empty() {
                model.toast(recovery.message());
            } else {
                model.show_error(recovery.message());
            }
        }
        if std::env::var_os("NIRI_SETTINGS_A11Y").is_some() {
            crate::a11y::print_report(&root);
        }
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        let trace = std::env::var_os("NIRI_SETTINGS_TRACE").is_some();
        let label = if trace && !matches!(msg, Msg::PollExternal) {
            Some(format!("{msg:?}"))
        } else {
            None
        };
        let started = std::time::Instant::now();
        self.update_inner(msg, sender.clone());
        self.maybe_check(&sender);
        if let Some(label) = label {
            eprintln!("trace: {label} took {:?}", started.elapsed());
        }
    }
}

impl AppModel {
    fn update_inner(&mut self, msg: Msg, sender: ComponentSender<Self>) {
        if self.busy.get() && !Self::runs_while_busy(&msg) {
            // Our own write changes the file; checking for outside edits
            // now would mistake it for one. The next poll sees it settled.
            if !matches!(msg, Msg::PollExternal) {
                jobs::enqueue(&mut self.queue, msg, |queued| match queued {
                    Msg::Set(key, _) | Msg::SetRebuild(key, _) => Some(key.as_str()),
                    _ => None,
                });
            }
            return;
        }
        match msg {
            Msg::Navigate(page) => self.navigate(page, &sender),
            Msg::Section(id) => self.show_section(id, &sender),
            Msg::JobDone(finished) => self.finish(finished, &sender),
            Msg::MakePersonalCopy => self.make_personal_copy(&sender),
            Msg::Checked(checked) => self.checks_done(checked, &sender),
            Msg::Search(text) => {
                if text == self.search {
                    return;
                }
                self.search = text;
                if self.search_active() {
                    self.page = Page::All;
                }
                self.refresh(&sender);
            }
            Msg::ShortcutSearch(text) => {
                self.shortcut_search = text;
                self.refresh(&sender);
            }
            Msg::Set(key, value) => self.apply(key, value, &sender, false),
            Msg::SetRebuild(key, value) => self.apply(key, value, &sender, true),
            Msg::Reset(key) => self.reset(key, &sender),
            Msg::Undo => self.start(Job::Undo, &sender),
            Msg::Redo => self.start(Job::Redo, &sender),
            Msg::Reload => {
                if let Err(error) = self.session.reload() {
                    self.show_error(format!("{error}"));
                }
                self.refresh(&sender);
            }
            Msg::Toast(text) => self.toast(text),
            Msg::Error(text) => self.show_error(text),
            Msg::OpenHistory => self.show_history(&sender),
            Msg::Restore(id) => match self.session.restore(id) {
                Ok(applied) => {
                    self.toast(&applied.summary);
                    self.refresh(&sender);
                }
                Err(error) => self.show_error(format!("{error}")),
            },
            Msg::OpenPreferences => self.show_preferences(&sender),
            Msg::ToggleNames(value) => {
                self.prefs.show_config_names = value;
                self.prefs.save();
                self.refresh(&sender);
            }
            Msg::WelcomeNext => {
                self.welcome.next();
                self.refresh(&sender);
            }
            Msg::WelcomeBack => {
                self.welcome.back();
                self.refresh(&sender);
            }
            Msg::WelcomeSkip => {
                if self.welcome.step == welcome::Step::Done {
                    self.close_welcome(&sender);
                } else {
                    self.welcome.next();
                    self.refresh(&sender);
                }
            }
            Msg::WelcomeClose => self.close_welcome(&sender),
            Msg::WelcomeSource(source) => {
                self.welcome.set_source(source);
                self.refresh(&sender);
            }
            Msg::WelcomePreset(group, id) => {
                self.welcome.set_preset(group, id);
                self.refresh(&sender);
            }
            // Typing must not rebuild the page, or the entry loses focus.
            Msg::WelcomeLayout(text) => self.welcome.set_layout(text),
            Msg::WelcomeApply => self.welcome_apply(&sender),
            Msg::OpenAddBind => self.show_add_bind(&sender, None),
            Msg::ApplyScheme(id) => self.apply_scheme(id, &sender),
            Msg::SchemeConfirmed(id) => self.scheme_confirmed(id, &sender),
            Msg::OpenEditBind(key) => self.show_add_bind(&sender, Some(key)),
            Msg::AddBind(key, action) => self.add_bind(key, action, &sender),
            Msg::ReplaceBind(old, new, action) => self.replace_bind(old, new, action, &sender),
            Msg::RemoveBind(key) => self.remove_bind(key, &sender),
            Msg::RemoveBindConfirmed(key) => self.remove_bind_confirmed(key, &sender),
            Msg::OpenAddRule(app) => self.show_add_rule(&sender, app),
            Msg::OpenEditRule(index) => self.show_edit_rule(&sender, index),
            Msg::EditRule(index, body) => self.edit_rule(index, body, &sender),
            Msg::AddRule(body) => self.add_rule(body, &sender),
            Msg::RemoveRule(index) => self.remove_rule(index, &sender),
            Msg::PickWindow => Self::pick_window(&sender),
            Msg::AddRuleFor(app_id, _title) => self.show_add_rule(&sender, Some(app_id)),
            Msg::OpenAddStartup => self.open_add_startup(&sender),
            Msg::AddStartup(command) => self.add_startup(command, &sender),
            Msg::RemoveStartup(index) => self.remove_startup(index, &sender),
            Msg::OpenAddEnv => self.show_env_prompt(&sender),
            Msg::SetEnv(name, value) => self.set_env(name, value, &sender),
            Msg::RemoveEnv(name) => self.remove_env(name, &sender),
            Msg::OpenAddWorkspace => self.show_workspace_prompt(&sender),
            Msg::AddWorkspace(name, output) => self.add_workspace(name, output, &sender),
            Msg::RemoveWorkspace(name) => self.remove_workspace(name, &sender),
            Msg::ApplyFix(fix) => {
                let result = match fix {
                    Fix::Set { key, value } => self.session.set(&key, value).map(|a| a.summary),
                    Fix::Reset { key } => self.session.reset(&key).map(|a| a.summary),
                    Fix::RemoveBind { key } => self.session.remove_bind(&key).map(|a| a.summary),
                    Fix::AddBind { key, action } => {
                        self.session.add_bind(&key, &action).map(|a| a.summary)
                    }
                };
                match result {
                    Ok(summary) => {
                        self.toast_undo(summary, &sender);
                        self.refresh(&sender);
                    }
                    Err(error) => self.show_error(format!("{error}")),
                }
            }
            Msg::ApplyPreset(id) => {
                let Some(preset) = niri_settings_core::presets::PRESETS
                    .iter()
                    .find(|preset| preset.id == id)
                else {
                    return;
                };
                match self.session.set_many(preset.changes, preset.label) {
                    Ok(applied) => {
                        self.toast_undo(&applied.summary, &sender);
                        self.refresh(&sender);
                    }
                    Err(niri_settings_core::ApplyError::NoChange) => {}
                    Err(error) => self.show_error(format!("{error}")),
                }
            }
            Msg::OpenImport => self.show_import(&sender),
            Msg::ImportPreview(result, path) => self.show_import_preview(&sender, &result, &path),
            Msg::ImportApply(result) => match self.session.apply_import(&result) {
                Ok(applied) => {
                    self.toast_undo(applied.summary, &sender);
                    self.refresh(&sender);
                }
                Err(niri_settings_core::ApplyError::NoChange) => {
                    self.toast("Nothing to import");
                }
                Err(error) => self.show_error(format!("{error}")),
            },
            Msg::ConfigFile(index) => {
                self.config_file = index;
                self.refresh(&sender);
            }
            Msg::WriteRaw(path, text) => {
                match self
                    .session
                    .write_raw(&path, &text, "Edited the config file")
                {
                    Ok(applied) => {
                        self.toast_undo(applied.summary, &sender);
                        self.refresh(&sender);
                    }
                    Err(error) => self.show_error(format!("{error}")),
                }
            }
            Msg::OpenConfig => {
                let path = self
                    .session
                    .files()
                    .get(self.config_file)
                    .cloned()
                    .unwrap_or_else(|| self.session.real_path.clone());
                let uri = gio::File::for_path(&path).uri();
                let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
            }
            Msg::OpenAbout => {
                let dialog = adw::AboutDialog::builder()
                    .application_name("niri settings")
                    .application_icon("io.github.niri_settings.NiriSettings")
                    .developer_name("The niri settings authors")
                    .version(env!("CARGO_PKG_VERSION"))
                    .comments(
                        "A safe graphical settings app for the niri compositor. \
                         It keeps your comments, validates every change and checks each file before writing.",
                    )
                    .license_type(gtk::License::Gpl30)
                    .website("https://github.com/pantarune/niri-settings")
                    .build();
                dialog.present(Some(&self.root));
            }
            Msg::CopyNix => {
                let snippet = self.session.nix_snippet();
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&snippet);
                }
                self.toast("Copied the Nix settings");
            }
            Msg::DisplayChange { name, field, value } => {
                self.display_change(name, field, value, &sender)
            }
            Msg::DisplayKeep { name, field, value } => self.display_keep(name, field, value),
            Msg::DisplayReverted => self.refresh(&sender),
            Msg::PollExternal => {
                let Some(watcher) = &self._watch else {
                    return;
                };
                if !watcher.changed() {
                    return;
                }
                let changed = self.session.tree.files.iter().any(|file| {
                    std::fs::read_to_string(&file.path).ok().as_deref() != Some(file.text.as_str())
                });
                if !changed {
                    self.last_watch_error = None;
                    return;
                }
                match self.session.reload() {
                    Ok(()) => {
                        self.last_watch_error = None;
                        self.toast("Reloaded the config from disk");
                        self.refresh(&sender);
                    }
                    Err(error) => {
                        let text = format!("{error}");
                        if self.last_watch_error.as_deref() != Some(text.as_str()) {
                            self.show_error(format!("The config on disk is not valid yet: {text}"));
                            self.last_watch_error = Some(text);
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn transform_name(transform: niri_ipc::Transform) -> &'static str {
    match transform {
        niri_ipc::Transform::Normal => "normal",
        niri_ipc::Transform::_90 => "90",
        niri_ipc::Transform::_180 => "180",
        niri_ipc::Transform::_270 => "270",
        niri_ipc::Transform::Flipped => "flipped",
        niri_ipc::Transform::Flipped90 => "flipped-90",
        niri_ipc::Transform::Flipped180 => "flipped-180",
        niri_ipc::Transform::Flipped270 => "flipped-270",
    }
}

fn add_action(app: &adw::Application, name: &str, sender: ComponentSender<AppModel>, msg: Msg) {
    let action = gio::SimpleAction::new(name, None);
    let sender = sender.clone();
    action.connect_activate(move |_, _| {
        sender.input(msg.clone());
    });
    app.add_action(&action);
}

#[cfg(test)]
mod tests {
    use super::MENU_SECTIONS;

    #[test]
    fn the_main_menu_holds_only_app_level_commands() {
        let actions: Vec<&str> = MENU_SECTIONS
            .iter()
            .flat_map(|section| section.iter())
            .map(|(_, action)| *action)
            .collect();
        assert_eq!(
            actions,
            [
                "app.undo",
                "app.redo",
                "app.history",
                "app.preferences",
                "app.about",
            ]
        );
    }
}
