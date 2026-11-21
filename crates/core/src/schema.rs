//! The option catalogue.
//!
//! Every entry names a real niri config path. The label and help text are
//! written in plain words; the path stays visible so people can learn the
//! config. The catalogue is hand-checked against niri 26.04. The `All options`
//! page and the config editor cover options that are not listed here, so no
//! setting is out of reach.

/// A settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Home,
    Appearance,
    Layout,
    Apps,
    Keyboard,
    Shortcuts,
    Mouse,
    Displays,
    Workspaces,
    Animations,
    Startup,
    Health,
    All,
    Config,
}

/// Page metadata for the navigation.
#[derive(Debug, Clone, Copy)]
pub struct PageInfo {
    pub page: Page,
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub icon: &'static str,
}

pub const PAGES: &[PageInfo] = &[
    PageInfo {
        page: Page::Home,
        id: "home",
        title: "Home",
        subtitle: "Your setup at a glance",
        icon: "go-home-symbolic",
    },
    PageInfo {
        page: Page::Appearance,
        id: "appearance",
        title: "Appearance",
        subtitle: "Gaps, borders, colors and shadows",
        icon: "preferences-desktop-theme-symbolic",
    },
    PageInfo {
        page: Page::Layout,
        id: "layout",
        title: "Windows and layout",
        subtitle: "How windows open and move",
        icon: "view-grid-symbolic",
    },
    PageInfo {
        page: Page::Apps,
        id: "apps",
        title: "Apps",
        subtitle: "Rules for single apps",
        icon: "application-x-executable-symbolic",
    },
    PageInfo {
        page: Page::Keyboard,
        id: "keyboard",
        title: "Keyboard",
        subtitle: "Layout, repeat speed and Num Lock",
        icon: "input-keyboard-symbolic",
    },
    PageInfo {
        page: Page::Shortcuts,
        id: "shortcuts",
        title: "Shortcuts",
        subtitle: "Keys that run actions",
        icon: "preferences-desktop-keyboard-symbolic",
    },
    PageInfo {
        page: Page::Mouse,
        id: "mouse",
        title: "Mouse and touchpad",
        subtitle: "Speed, scrolling and gestures",
        icon: "input-mouse-symbolic",
    },
    PageInfo {
        page: Page::Displays,
        id: "displays",
        title: "Displays",
        subtitle: "Arrangement, mode and scale",
        icon: "video-display-symbolic",
    },
    PageInfo {
        page: Page::Workspaces,
        id: "workspaces",
        title: "Workspaces",
        subtitle: "Names and displays",
        icon: "view-paged-symbolic",
    },
    PageInfo {
        page: Page::Animations,
        id: "animations",
        title: "Animations",
        subtitle: "Motion and speed",
        icon: "media-playback-start-symbolic",
    },
    PageInfo {
        page: Page::Startup,
        id: "startup",
        title: "Startup",
        subtitle: "Apps and environment variables",
        icon: "system-run-symbolic",
    },
    PageInfo {
        page: Page::Health,
        id: "health",
        title: "Check my setup",
        subtitle: "Find problems and fix them",
        icon: "emblem-ok-symbolic",
    },
    PageInfo {
        page: Page::All,
        id: "all",
        title: "All options",
        subtitle: "Every option in the catalog",
        icon: "view-list-symbolic",
    },
    PageInfo {
        page: Page::Config,
        id: "config",
        title: "Config file",
        subtitle: "The file itself",
        icon: "text-x-generic-symbolic",
    },
];

impl Page {
    pub fn info(self) -> &'static PageInfo {
        PAGES
            .iter()
            .find(|info| info.page == self)
            .expect("every page has metadata")
    }

    pub fn from_id(id: &str) -> Option<Self> {
        PAGES
            .iter()
            .find(|info| info.id == id)
            .map(|info| info.page)
    }
}

/// How a true or false value appears in the KDL file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleStyle {
    /// A bare node means true: `numlock`.
    Presence,
    /// The value is an `on` or `off` child of a section, for example
    /// `layout { border { on } }`.
    Section,
    /// The section only accepts `off`. Turning it back on removes `off`.
    OffOnly,
}

/// A unit shown next to a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    None,
    Px,
    Percent,
    Ms,
    Hz,
    Times,
}

impl Unit {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Px => " px",
            Self::Percent => " %",
            Self::Ms => " ms",
            Self::Hz => " Hz",
            Self::Times => "×",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}

/// The control that edits an option.
#[derive(Debug, Clone, Copy)]
pub enum Control {
    Toggle {
        default: bool,
        style: ToggleStyle,
    },
    Number {
        default: f64,
        min: f64,
        max: f64,
        step: f64,
        unit: Unit,
        integer: bool,
    },
    Text {
        default: &'static str,
        placeholder: &'static str,
    },
    Color {
        default: &'static str,
    },
    Choice {
        default: &'static str,
        choices: &'static [Choice],
    },
}

/// One option in the catalogue.
#[derive(Debug, Clone, Copy)]
pub struct OptionInfo {
    /// Canonical path shown to the user, for example `layout.gaps`.
    pub key: &'static str,
    /// Parent sections, for example `["layout", "focus-ring"]`.
    pub parent: &'static [&'static str],
    /// The node that holds the value, for example `width`.
    pub node: &'static str,
    /// The property that holds the value, when the value is a KDL property.
    pub property: Option<&'static str>,
    pub label: &'static str,
    pub help: &'static str,
    pub control: Control,
    pub page: Page,
    /// Extra words for search, space separated.
    pub keywords: &'static str,
}

impl OptionInfo {
    pub fn default_bool(&self) -> Option<bool> {
        match self.control {
            Control::Toggle { default, .. } => Some(default),
            _ => None,
        }
    }

    pub fn default_f64(&self) -> Option<f64> {
        match self.control {
            Control::Number { default, .. } => Some(default),
            _ => None,
        }
    }

    pub fn default_str(&self) -> Option<&'static str> {
        match self.control {
            Control::Text { default, .. }
            | Control::Color { default }
            | Control::Choice { default, .. } => Some(default),
            _ => None,
        }
    }

    pub fn is_toggle(&self) -> bool {
        matches!(self.control, Control::Toggle { .. })
    }

    /// The full node path that the editor must read or write.
    pub fn node_path(&self) -> Vec<String> {
        let mut out: Vec<String> = self.parent.iter().map(|s| s.to_string()).collect();
        out.push(self.node.to_string());
        out
    }

    /// True when the plain words contain the query.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        self.label.to_lowercase().contains(&q)
            || self.help.to_lowercase().contains(&q)
            || self.key.to_lowercase().contains(&q)
            || self.keywords.to_lowercase().contains(&q)
    }
}

const CENTER_CHOICES: &[Choice] = &[
    Choice {
        value: "never",
        label: "Never",
    },
    Choice {
        value: "always",
        label: "Always",
    },
    Choice {
        value: "on-overflow",
        label: "When the column overflows",
    },
];

const COLUMN_DISPLAY_CHOICES: &[Choice] = &[
    Choice {
        value: "normal",
        label: "Split into columns",
    },
    Choice {
        value: "tabbed",
        label: "Tabbed",
    },
];

const POSITION_CHOICES: &[Choice] = &[
    Choice {
        value: "left",
        label: "Left edge",
    },
    Choice {
        value: "right",
        label: "Right edge",
    },
    Choice {
        value: "top",
        label: "Top edge",
    },
    Choice {
        value: "bottom",
        label: "Bottom edge",
    },
];

const ACCEL_PROFILE_CHOICES: &[Choice] = &[
    Choice {
        value: "adaptive",
        label: "Adaptive (more movement with speed)",
    },
    Choice {
        value: "flat",
        label: "Flat (1:1 movement)",
    },
];

const SCROLL_METHOD_CHOICES: &[Choice] = &[
    Choice {
        value: "no-scroll",
        label: "No scrolling",
    },
    Choice {
        value: "two-finger",
        label: "Two fingers",
    },
    Choice {
        value: "edge",
        label: "Edge scrolling",
    },
    Choice {
        value: "on-button-down",
        label: "Hold a button and move",
    },
];

const CLICK_METHOD_CHOICES: &[Choice] = &[
    Choice {
        value: "button-areas",
        label: "Button areas",
    },
    Choice {
        value: "clickfinger",
        label: "Click with fingers",
    },
];

const TAP_MAP_CHOICES: &[Choice] = &[
    Choice {
        value: "left-right-middle",
        label: "Left, right, middle",
    },
    Choice {
        value: "left-middle-right",
        label: "Left, middle, right",
    },
];

const TRACK_LAYOUT_CHOICES: &[Choice] = &[
    Choice {
        value: "global",
        label: "Global layout",
    },
    Choice {
        value: "window",
        label: "Layout per window",
    },
];

const MOD_KEY_CHOICES: &[Choice] = &[
    Choice {
        value: "Mod",
        label: "Mod (Super on a TTY)",
    },
    Choice {
        value: "Super",
        label: "Super",
    },
    Choice {
        value: "Alt",
        label: "Alt",
    },
];

pub const VRR_CHOICES: &[Choice] = &[
    Choice {
        value: "off",
        label: "Off",
    },
    Choice {
        value: "on",
        label: "Always on",
    },
    Choice {
        value: "on-demand",
        label: "On demand",
    },
];

pub const TRANSFORM_CHOICES: &[Choice] = &[
    Choice {
        value: "normal",
        label: "Normal",
    },
    Choice {
        value: "90",
        label: "90° clockwise",
    },
    Choice {
        value: "180",
        label: "180°",
    },
    Choice {
        value: "270",
        label: "270° clockwise",
    },
    Choice {
        value: "flipped",
        label: "Flipped",
    },
    Choice {
        value: "flipped-90",
        label: "Flipped, 90°",
    },
    Choice {
        value: "flipped-180",
        label: "Flipped, 180°",
    },
    Choice {
        value: "flipped-270",
        label: "Flipped, 270°",
    },
];

/// Every curated option, in page order.
pub const OPTIONS: &[OptionInfo] = &[
    // ── Appearance ─────────────────────────────────────────────────────
    OptionInfo {
        key: "layout.gaps",
        parent: &["layout"],
        node: "gaps",
        property: None,
        label: "Space between windows",
        help: "The empty space around and between windows.",
        control: Control::Number {
            default: 16.0,
            min: 0.0,
            max: 128.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "gaps padding spacing",
    },
    OptionInfo {
        key: "layout.focus-ring.off",
        parent: &["layout", "focus-ring"],
        node: "off",
        property: None,
        label: "Focus ring",
        help: "Draw a colored ring around the focused window.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::Section,
        },
        page: Page::Appearance,
        keywords: "focus ring highlight active",
    },
    OptionInfo {
        key: "layout.focus-ring.width",
        parent: &["layout", "focus-ring"],
        node: "width",
        property: None,
        label: "Ring width",
        help: "How thick the focus ring is.",
        control: Control::Number {
            default: 4.0,
            min: 0.0,
            max: 64.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "focus ring width",
    },
    OptionInfo {
        key: "layout.focus-ring.active-color",
        parent: &["layout", "focus-ring"],
        node: "active-color",
        property: None,
        label: "Focused color",
        help: "The ring color of the focused window.",
        control: Control::Color { default: "#7fc8ff" },
        page: Page::Appearance,
        keywords: "focus ring color color active",
    },
    OptionInfo {
        key: "layout.focus-ring.inactive-color",
        parent: &["layout", "focus-ring"],
        node: "inactive-color",
        property: None,
        label: "Other windows",
        help: "The ring color of windows that are not focused.",
        control: Control::Color { default: "#505050" },
        page: Page::Appearance,
        keywords: "focus ring color color inactive",
    },
    OptionInfo {
        key: "layout.focus-ring.urgent-color",
        parent: &["layout", "focus-ring"],
        node: "urgent-color",
        property: None,
        label: "Urgent color",
        help: "The ring color of a window that needs attention.",
        control: Control::Color { default: "#9b0000" },
        page: Page::Appearance,
        keywords: "focus ring urgent color color",
    },
    OptionInfo {
        key: "layout.border.off",
        parent: &["layout", "border"],
        node: "off",
        property: None,
        label: "Window border",
        help: "Draw a second ring outside the focus ring.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Section,
        },
        page: Page::Appearance,
        keywords: "border frame outline",
    },
    OptionInfo {
        key: "layout.border.width",
        parent: &["layout", "border"],
        node: "width",
        property: None,
        label: "Border width",
        help: "How thick the window border is.",
        control: Control::Number {
            default: 4.0,
            min: 0.0,
            max: 64.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "border width",
    },
    OptionInfo {
        key: "layout.border.active-color",
        parent: &["layout", "border"],
        node: "active-color",
        property: None,
        label: "Border color",
        help: "The border color of the focused window.",
        control: Control::Color { default: "#ffc87f" },
        page: Page::Appearance,
        keywords: "border color color active",
    },
    OptionInfo {
        key: "layout.shadow.on",
        parent: &["layout", "shadow"],
        node: "on",
        property: None,
        label: "Window shadows",
        help: "Draw a soft shadow behind windows.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Section,
        },
        page: Page::Appearance,
        keywords: "shadow drop",
    },
    OptionInfo {
        key: "layout.shadow.softness",
        parent: &["layout", "shadow"],
        node: "softness",
        property: None,
        label: "Shadow softness",
        help: "How far the shadow fades out.",
        control: Control::Number {
            default: 30.0,
            min: 0.0,
            max: 256.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "shadow blur softness",
    },
    OptionInfo {
        key: "layout.shadow.spread",
        parent: &["layout", "shadow"],
        node: "spread",
        property: None,
        label: "Shadow spread",
        help: "How far the shadow reaches beyond the window.",
        control: Control::Number {
            default: 5.0,
            min: -64.0,
            max: 128.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "shadow spread size",
    },
    OptionInfo {
        key: "layout.shadow.offset.x",
        parent: &["layout", "shadow"],
        node: "offset",
        property: Some("x"),
        label: "Shadow offset, horizontal",
        help: "Move the shadow left or right.",
        control: Control::Number {
            default: 0.0,
            min: -128.0,
            max: 128.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "shadow offset x horizontal",
    },
    OptionInfo {
        key: "layout.shadow.offset.y",
        parent: &["layout", "shadow"],
        node: "offset",
        property: Some("y"),
        label: "Shadow offset, vertical",
        help: "Move the shadow up or down.",
        control: Control::Number {
            default: 5.0,
            min: -128.0,
            max: 128.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "shadow offset y vertical",
    },
    OptionInfo {
        key: "layout.shadow.color",
        parent: &["layout", "shadow"],
        node: "color",
        property: None,
        label: "Shadow color",
        help: "The color of the window shadow.",
        control: Control::Color {
            default: "#00000077",
        },
        page: Page::Appearance,
        keywords: "shadow color color",
    },
    OptionInfo {
        key: "layout.tab-indicator.off",
        parent: &["layout", "tab-indicator"],
        node: "off",
        property: None,
        label: "Tab indicator",
        help: "Show a small bar for tabbed columns.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::Section,
        },
        page: Page::Appearance,
        keywords: "tab indicator bar",
    },
    OptionInfo {
        key: "layout.tab-indicator.width",
        parent: &["layout", "tab-indicator"],
        node: "width",
        property: None,
        label: "Tab bar width",
        help: "How thick the tab indicator is.",
        control: Control::Number {
            default: 4.0,
            min: 0.0,
            max: 64.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "tab indicator width",
    },
    OptionInfo {
        key: "layout.tab-indicator.position",
        parent: &["layout", "tab-indicator"],
        node: "position",
        property: None,
        label: "Tab bar position",
        help: "The edge of the column where the tab indicator is.",
        control: Control::Choice {
            default: "left",
            choices: POSITION_CHOICES,
        },
        page: Page::Appearance,
        keywords: "tab indicator position edge",
    },
    OptionInfo {
        key: "layout.background-color",
        parent: &["layout"],
        node: "background-color",
        property: None,
        label: "Workspace background",
        help: "The color behind windows. Use transparent to show the wallpaper.",
        control: Control::Color {
            default: "#00000000",
        },
        page: Page::Appearance,
        keywords: "background color color wallpaper",
    },
    OptionInfo {
        key: "overview.zoom",
        parent: &["overview"],
        node: "zoom",
        property: None,
        label: "Overview zoom",
        help: "How small workspaces look in the overview.",
        control: Control::Number {
            default: 0.5,
            min: 0.1,
            max: 1.0,
            step: 0.05,
            unit: Unit::None,
            integer: false,
        },
        page: Page::Appearance,
        keywords: "overview zoom scale",
    },
    OptionInfo {
        key: "overview.backdrop-color",
        parent: &["overview"],
        node: "backdrop-color",
        property: None,
        label: "Overview backdrop",
        help: "The color behind workspaces in the overview.",
        control: Control::Color {
            default: "#00000000",
        },
        page: Page::Appearance,
        keywords: "overview backdrop color color",
    },
    OptionInfo {
        key: "cursor.xcursor-size",
        parent: &["cursor"],
        node: "xcursor-size",
        property: None,
        label: "Cursor size",
        help: "The size of the mouse pointer.",
        control: Control::Number {
            default: 24.0,
            min: 8.0,
            max: 256.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Appearance,
        keywords: "cursor mouse pointer size",
    },
    OptionInfo {
        key: "cursor.xcursor-theme",
        parent: &["cursor"],
        node: "xcursor-theme",
        property: None,
        label: "Cursor theme",
        help: "The name of the pointer theme.",
        control: Control::Text {
            default: "default",
            placeholder: "Theme name",
        },
        page: Page::Appearance,
        keywords: "cursor mouse pointer theme",
    },
    // ── Windows and layout ─────────────────────────────────────────────
    OptionInfo {
        key: "layout.center-focused-column",
        parent: &["layout"],
        node: "center-focused-column",
        property: None,
        label: "Center the focused column",
        help: "Move the focused column to the middle of the screen.",
        control: Control::Choice {
            default: "never",
            choices: CENTER_CHOICES,
        },
        page: Page::Layout,
        keywords: "center center focus column",
    },
    OptionInfo {
        key: "layout.always-center-single-column",
        parent: &["layout"],
        node: "always-center-single-column",
        property: None,
        label: "Center a single column",
        help: "When only one column is open, put it in the middle.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Layout,
        keywords: "center center single column",
    },
    OptionInfo {
        key: "layout.empty-workspace-above-first",
        parent: &["layout"],
        node: "empty-workspace-above-first",
        property: None,
        label: "Empty workspace above the first",
        help: "Keep one empty workspace above the topmost workspace.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Layout,
        keywords: "empty workspace above first",
    },
    OptionInfo {
        key: "layout.default-column-width.proportion",
        parent: &["layout", "default-column-width"],
        node: "proportion",
        property: None,
        label: "Default column width",
        help: "The part of the screen that a new column uses.",
        control: Control::Number {
            default: 0.5,
            min: 0.1,
            max: 1.0,
            step: 0.05,
            unit: Unit::None,
            integer: false,
        },
        page: Page::Layout,
        keywords: "default column width proportion size",
    },
    OptionInfo {
        key: "layout.default-column-display",
        parent: &["layout"],
        node: "default-column-display",
        property: None,
        label: "New columns are",
        help: "Show windows as normal columns or as tabs.",
        control: Control::Choice {
            default: "normal",
            choices: COLUMN_DISPLAY_CHOICES,
        },
        page: Page::Layout,
        keywords: "column display tabbed tabs",
    },
    OptionInfo {
        key: "layout.struts.left",
        parent: &["layout", "struts"],
        node: "left",
        property: None,
        label: "Reserved space, left",
        help: "Keep this much space free for a bar or panel.",
        control: Control::Number {
            default: 0.0,
            min: 0.0,
            max: 512.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Layout,
        keywords: "struts reserved space left bar",
    },
    OptionInfo {
        key: "layout.struts.right",
        parent: &["layout", "struts"],
        node: "right",
        property: None,
        label: "Reserved space, right",
        help: "Keep this much space free for a bar or panel.",
        control: Control::Number {
            default: 0.0,
            min: 0.0,
            max: 512.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Layout,
        keywords: "struts reserved space right bar",
    },
    OptionInfo {
        key: "layout.struts.top",
        parent: &["layout", "struts"],
        node: "top",
        property: None,
        label: "Reserved space, top",
        help: "Keep this much space free for a bar or panel.",
        control: Control::Number {
            default: 0.0,
            min: 0.0,
            max: 512.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Layout,
        keywords: "struts reserved space top bar",
    },
    OptionInfo {
        key: "layout.struts.bottom",
        parent: &["layout", "struts"],
        node: "bottom",
        property: None,
        label: "Reserved space, bottom",
        help: "Keep this much space free for a bar or panel.",
        control: Control::Number {
            default: 0.0,
            min: 0.0,
            max: 512.0,
            step: 1.0,
            unit: Unit::Px,
            integer: true,
        },
        page: Page::Layout,
        keywords: "struts reserved space bottom bar",
    },
    // ── Keyboard ───────────────────────────────────────────────────────
    OptionInfo {
        key: "input.keyboard.xkb.layout",
        parent: &["input", "keyboard", "xkb"],
        node: "layout",
        property: None,
        label: "Keyboard layout",
        help: "One or more layout codes, separated by commas. For example us,de.",
        control: Control::Text {
            default: "us",
            placeholder: "us",
        },
        page: Page::Keyboard,
        keywords: "keyboard layout xkb language",
    },
    OptionInfo {
        key: "input.keyboard.xkb.variant",
        parent: &["input", "keyboard", "xkb"],
        node: "variant",
        property: None,
        label: "Layout variant",
        help: "A variant for each layout, separated by commas.",
        control: Control::Text {
            default: "",
            placeholder: "e.g. colemak",
        },
        page: Page::Keyboard,
        keywords: "keyboard variant xkb",
    },
    OptionInfo {
        key: "input.keyboard.xkb.options",
        parent: &["input", "keyboard", "xkb"],
        node: "options",
        property: None,
        label: "Layout options",
        help: "Extra XKB options, separated by commas.",
        control: Control::Text {
            default: "",
            placeholder: "e.g. compose:caps",
        },
        page: Page::Keyboard,
        keywords: "keyboard options xkb caps compose",
    },
    OptionInfo {
        key: "input.keyboard.repeat-delay",
        parent: &["input", "keyboard"],
        node: "repeat-delay",
        property: None,
        label: "Repeat delay",
        help: "How long to hold a key before it starts to repeat.",
        control: Control::Number {
            default: 600.0,
            min: 100.0,
            max: 2000.0,
            step: 10.0,
            unit: Unit::Ms,
            integer: true,
        },
        page: Page::Keyboard,
        keywords: "keyboard repeat delay",
    },
    OptionInfo {
        key: "input.keyboard.repeat-rate",
        parent: &["input", "keyboard"],
        node: "repeat-rate",
        property: None,
        label: "Repeat rate",
        help: "How many times a second a held key repeats.",
        control: Control::Number {
            default: 25.0,
            min: 1.0,
            max: 250.0,
            step: 1.0,
            unit: Unit::Hz,
            integer: true,
        },
        page: Page::Keyboard,
        keywords: "keyboard repeat rate speed",
    },
    OptionInfo {
        key: "input.keyboard.numlock",
        parent: &["input", "keyboard"],
        node: "numlock",
        property: None,
        label: "Num Lock on start",
        help: "Turn Num Lock on when niri starts.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Keyboard,
        keywords: "keyboard numlock numeric",
    },
    OptionInfo {
        key: "input.keyboard.track-layout",
        parent: &["input", "keyboard"],
        node: "track-layout",
        property: None,
        label: "Remember layout per window",
        help: "Keep a separate keyboard layout for each window.",
        control: Control::Choice {
            default: "global",
            choices: TRACK_LAYOUT_CHOICES,
        },
        page: Page::Keyboard,
        keywords: "keyboard layout track window global",
    },
    OptionInfo {
        key: "input.mod-key",
        parent: &["input"],
        node: "mod-key",
        property: None,
        label: "Mod key",
        help: "The key that Mod means in shortcuts.",
        control: Control::Choice {
            default: "Mod",
            choices: MOD_KEY_CHOICES,
        },
        page: Page::Keyboard,
        keywords: "mod key super alt shortcut",
    },
    // ── Mouse and touchpad ─────────────────────────────────────────────
    OptionInfo {
        key: "input.mouse.accel-speed",
        parent: &["input", "mouse"],
        node: "accel-speed",
        property: None,
        label: "Mouse speed",
        help: "Negative is slower, positive is faster.",
        control: Control::Number {
            default: 0.0,
            min: -1.0,
            max: 1.0,
            step: 0.05,
            unit: Unit::None,
            integer: false,
        },
        page: Page::Mouse,
        keywords: "mouse speed acceleration",
    },
    OptionInfo {
        key: "input.mouse.accel-profile",
        parent: &["input", "mouse"],
        node: "accel-profile",
        property: None,
        label: "Mouse acceleration",
        help: "Adaptive: a fast movement moves the pointer farther. Flat: the pointer speed does not change.",
        control: Control::Choice {
            default: "adaptive",
            choices: ACCEL_PROFILE_CHOICES,
        },
        page: Page::Mouse,
        keywords: "mouse acceleration profile adaptive flat",
    },
    OptionInfo {
        key: "input.mouse.natural-scroll",
        parent: &["input", "mouse"],
        node: "natural-scroll",
        property: None,
        label: "Natural scrolling",
        help: "The content moves in the same direction as the wheel.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Mouse,
        keywords: "mouse natural scroll inverse",
    },
    OptionInfo {
        key: "input.mouse.left-handed",
        parent: &["input", "mouse"],
        node: "left-handed",
        property: None,
        label: "Left-handed buttons",
        help: "Swap the left and right mouse buttons.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Mouse,
        keywords: "mouse left handed swap buttons",
    },
    OptionInfo {
        key: "input.mouse.scroll-method",
        parent: &["input", "mouse"],
        node: "scroll-method",
        property: None,
        label: "Mouse scroll method",
        help: "How the mouse scrolls.",
        control: Control::Choice {
            default: "no-scroll",
            choices: SCROLL_METHOD_CHOICES,
        },
        page: Page::Mouse,
        keywords: "mouse scroll method",
    },
    OptionInfo {
        key: "input.touchpad.tap",
        parent: &["input", "touchpad"],
        node: "tap",
        property: None,
        label: "Tap to click",
        help: "Tap the touchpad to click.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Mouse,
        keywords: "touchpad tap click",
    },
    OptionInfo {
        key: "input.touchpad.dwt",
        parent: &["input", "touchpad"],
        node: "dwt",
        property: None,
        label: "Disable while typing",
        help: "Ignore the touchpad while you type.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Mouse,
        keywords: "touchpad disable typing dwt",
    },
    OptionInfo {
        key: "input.touchpad.natural-scroll",
        parent: &["input", "touchpad"],
        node: "natural-scroll",
        property: None,
        label: "Natural scrolling",
        help: "The content moves in the same direction as your fingers.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Mouse,
        keywords: "touchpad natural scroll inverse",
    },
    OptionInfo {
        key: "input.touchpad.accel-speed",
        parent: &["input", "touchpad"],
        node: "accel-speed",
        property: None,
        label: "Touchpad speed",
        help: "Negative is slower, positive is faster.",
        control: Control::Number {
            default: 0.0,
            min: -1.0,
            max: 1.0,
            step: 0.05,
            unit: Unit::None,
            integer: false,
        },
        page: Page::Mouse,
        keywords: "touchpad speed acceleration",
    },
    OptionInfo {
        key: "input.touchpad.accel-profile",
        parent: &["input", "touchpad"],
        node: "accel-profile",
        property: None,
        label: "Touchpad acceleration",
        help: "Adaptive: a fast movement moves the pointer farther. Flat: the pointer speed does not change.",
        control: Control::Choice {
            default: "adaptive",
            choices: ACCEL_PROFILE_CHOICES,
        },
        page: Page::Mouse,
        keywords: "touchpad acceleration profile",
    },
    OptionInfo {
        key: "input.touchpad.click-method",
        parent: &["input", "touchpad"],
        node: "click-method",
        property: None,
        label: "Click method",
        help: "Use button areas or click with several fingers.",
        control: Control::Choice {
            default: "button-areas",
            choices: CLICK_METHOD_CHOICES,
        },
        page: Page::Mouse,
        keywords: "touchpad click method button areas clickfinger",
    },
    OptionInfo {
        key: "input.touchpad.tap-button-map",
        parent: &["input", "touchpad"],
        node: "tap-button-map",
        property: None,
        label: "Tap button order",
        help: "The button that niri sends when you tap with three fingers.",
        control: Control::Choice {
            default: "left-right-middle",
            choices: TAP_MAP_CHOICES,
        },
        page: Page::Mouse,
        keywords: "touchpad tap button map",
    },
    OptionInfo {
        key: "input.touchpad.scroll-method",
        parent: &["input", "touchpad"],
        node: "scroll-method",
        property: None,
        label: "Touchpad scroll method",
        help: "How the touchpad scrolls.",
        control: Control::Choice {
            default: "two-finger",
            choices: SCROLL_METHOD_CHOICES,
        },
        page: Page::Mouse,
        keywords: "touchpad scroll method",
    },
    OptionInfo {
        key: "input.focus-follows-mouse.max-scroll-amount",
        parent: &["input"],
        node: "focus-follows-mouse",
        property: Some("max-scroll-amount"),
        label: "Focus follows mouse",
        help: "Hover over a window to focus it. The percentage is the part of the screen that must be visible.",
        control: Control::Text {
            default: "0%",
            placeholder: "0%",
        },
        page: Page::Mouse,
        keywords: "focus follows mouse hover",
    },
    OptionInfo {
        key: "input.warp-mouse-to-focus.mode",
        parent: &["input"],
        node: "warp-mouse-to-focus",
        property: Some("mode"),
        label: "Warp the mouse to focus",
        help: "Move the pointer to the focused window. Use center-xy or center-xy-always.",
        control: Control::Text {
            default: "center-xy",
            placeholder: "center-xy",
        },
        page: Page::Mouse,
        keywords: "warp mouse focus center",
    },
    OptionInfo {
        key: "input.workspace-auto-back-and-forth",
        parent: &["input"],
        node: "workspace-auto-back-and-forth",
        property: None,
        label: "Go back to the last workspace",
        help: "Press the same workspace key twice to return.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::Mouse,
        keywords: "workspace back forth toggle",
    },
    // ── Animations ─────────────────────────────────────────────────────
    OptionInfo {
        key: "animations.off",
        parent: &["animations"],
        node: "off",
        property: None,
        label: "Animations",
        help: "Animate windows, workspaces and the overview.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::Section,
        },
        page: Page::Animations,
        keywords: "animations motion effects",
    },
    OptionInfo {
        key: "animations.slowdown",
        parent: &["animations"],
        node: "slowdown",
        property: None,
        label: "Animation speed",
        help: "1 is normal. Higher numbers are slower.",
        control: Control::Number {
            default: 1.0,
            min: 0.1,
            max: 10.0,
            step: 0.1,
            unit: Unit::Times,
            integer: false,
        },
        page: Page::Animations,
        keywords: "animation slowdown speed",
    },
    OptionInfo {
        key: "animations.workspace-switch.off",
        parent: &["animations", "workspace-switch"],
        node: "off",
        property: None,
        label: "Workspace switch",
        help: "Motion when the view moves to another workspace.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::OffOnly,
        },
        page: Page::Animations,
        keywords: "animation workspace switch motion",
    },
    OptionInfo {
        key: "animations.window-open.off",
        parent: &["animations", "window-open"],
        node: "off",
        property: None,
        label: "Window open",
        help: "Motion when a window opens.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::OffOnly,
        },
        page: Page::Animations,
        keywords: "animation window open motion",
    },
    OptionInfo {
        key: "animations.window-close.off",
        parent: &["animations", "window-close"],
        node: "off",
        property: None,
        label: "Window close",
        help: "Motion when a window closes.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::OffOnly,
        },
        page: Page::Animations,
        keywords: "animation window close motion",
    },
    OptionInfo {
        key: "animations.window-movement.off",
        parent: &["animations", "window-movement"],
        node: "off",
        property: None,
        label: "Window movement",
        help: "Motion when a window changes position.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::OffOnly,
        },
        page: Page::Animations,
        keywords: "animation window movement motion",
    },
    OptionInfo {
        key: "animations.window-resize.off",
        parent: &["animations", "window-resize"],
        node: "off",
        property: None,
        label: "Window resize",
        help: "Motion when a window changes size.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::OffOnly,
        },
        page: Page::Animations,
        keywords: "animation window resize motion",
    },
    OptionInfo {
        key: "animations.overview-open-close.off",
        parent: &["animations", "overview-open-close"],
        node: "off",
        property: None,
        label: "Overview open and close",
        help: "Motion when the overview opens or closes.",
        control: Control::Toggle {
            default: true,
            style: ToggleStyle::OffOnly,
        },
        page: Page::Animations,
        keywords: "animation overview open close motion",
    },
    // ── Misc ───────────────────────────────────────────────────────────
    OptionInfo {
        key: "prefer-no-csd",
        parent: &[],
        node: "prefer-no-csd",
        property: None,
        label: "Prefer no title bars",
        help: "Ask apps not to draw their own title bars. Restart apps after a change.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::All,
        keywords: "client side decorations csd title bar",
    },
    OptionInfo {
        key: "screenshot-path",
        parent: &[],
        node: "screenshot-path",
        property: None,
        label: "Where screenshots go",
        help: "A pattern for the file name. Date codes start with %.",
        control: Control::Text {
            default: "~/Pictures/Screenshots/Screenshot from %Y-%m-%d %H-%M-%S.png",
            placeholder: "~/Pictures/Screenshots/Screenshot %Y-%m-%d.png",
        },
        page: Page::All,
        keywords: "screenshot path save",
    },
    OptionInfo {
        key: "hotkey-overlay.skip-at-startup",
        parent: &["hotkey-overlay"],
        node: "skip-at-startup",
        property: None,
        label: "Do not show the shortcut list at start",
        help: "Skip the shortcut list that niri shows after it starts.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::All,
        keywords: "hotkey overlay startup shortcut list",
    },
    OptionInfo {
        key: "cursor.hide-when-typing",
        parent: &["cursor"],
        node: "hide-when-typing",
        property: None,
        label: "Hide the cursor while typing",
        help: "Hide the pointer when you type on the keyboard.",
        control: Control::Toggle {
            default: false,
            style: ToggleStyle::Presence,
        },
        page: Page::All,
        keywords: "cursor hide typing",
    },
];

/// Parse a value for an option from text. Used by the CLI and D-Bus.
pub fn parse_value(option: &OptionInfo, raw: &str) -> Result<crate::Value, String> {
    match option.control {
        Control::Toggle { .. } => match raw.trim().to_lowercase().as_str() {
            "true" | "on" | "yes" | "1" => Ok(crate::Value::Bool(true)),
            "false" | "off" | "no" | "0" => Ok(crate::Value::Bool(false)),
            other => Err(format!("{other} is not true or false")),
        },
        Control::Number { integer, .. } => {
            if integer {
                raw.trim()
                    .parse::<i64>()
                    .map(crate::Value::Int)
                    .map_err(|_| format!("{raw} is not a whole number"))
            } else {
                raw.trim()
                    .parse::<f64>()
                    .map(crate::Value::Float)
                    .map_err(|_| format!("{raw} is not a number"))
            }
        }
        Control::Choice { choices, .. } => {
            let raw = raw.trim();
            if choices.iter().any(|choice| choice.value == raw) {
                Ok(crate::Value::String(raw.to_string()))
            } else {
                let allowed: Vec<&str> = choices.iter().map(|choice| choice.value).collect();
                Err(format!("{raw} is not one of: {}", allowed.join(", ")))
            }
        }
        Control::Color { .. } | Control::Text { .. } => Ok(crate::Value::String(raw.to_string())),
    }
}

/// Look up an option by its canonical key.
pub fn option(key: &str) -> Option<&'static OptionInfo> {
    OPTIONS.iter().find(|info| info.key == key)
}

/// All options on a page, in catalogue order.
pub fn options_for(page: Page) -> impl Iterator<Item = &'static OptionInfo> {
    OPTIONS.iter().filter(move |info| info.page == page)
}

/// All options grouped by their first parent section.
pub fn groups() -> Vec<(&'static str, Vec<&'static OptionInfo>)> {
    let order = [
        "layout",
        "overview",
        "cursor",
        "input",
        "animations",
        "hotkey-overlay",
        "",
    ];
    let mut out: Vec<(&'static str, Vec<&'static OptionInfo>)> = Vec::new();
    for name in order {
        let items: Vec<_> = OPTIONS
            .iter()
            .filter(|info| info.parent.first().copied().unwrap_or("") == name)
            .collect();
        if !items.is_empty() {
            out.push((name, items));
        }
    }
    out
}

/// The section switches that must be on for an option to have an effect.
///
/// A section switch is a toggle named `<section>.off` or `<section>.on`, such
/// as `layout.focus-ring.off`. Its value is true when the section is on. Every
/// other option under `<section>.` depends on it, including nested sections:
/// `animations.window-open.off` depends on `animations.off`.
pub fn section_switches(key: &str) -> impl Iterator<Item = &'static OptionInfo> + '_ {
    OPTIONS.iter().filter(move |option| {
        let is_switch = matches!(
            option.control,
            Control::Toggle {
                style: ToggleStyle::Section | ToggleStyle::OffOnly,
                ..
            }
        );
        let section = option
            .key
            .strip_suffix(".off")
            .or_else(|| option.key.strip_suffix(".on"));
        is_switch
            && option.key != key
            && section.is_some_and(|section| {
                key.strip_prefix(section)
                    .is_some_and(|rest| rest.starts_with('.'))
            })
    })
}

#[cfg(test)]
mod tests {
    use super::section_switches;

    fn switches(key: &str) -> Vec<&'static str> {
        section_switches(key).map(|option| option.key).collect()
    }

    #[test]
    fn section_options_depend_on_their_switch() {
        assert_eq!(
            switches("layout.focus-ring.width"),
            ["layout.focus-ring.off"]
        );
        assert_eq!(
            switches("layout.border.active-color"),
            ["layout.border.off"]
        );
        assert_eq!(switches("layout.shadow.softness"), ["layout.shadow.on"]);
    }

    #[test]
    fn nested_sections_depend_on_every_switch_above() {
        assert_eq!(switches("animations.window-open.off"), ["animations.off"]);
    }

    #[test]
    fn a_switch_does_not_depend_on_itself_and_others_are_free() {
        assert!(switches("layout.focus-ring.off").is_empty());
        assert!(switches("layout.gaps").is_empty());
        assert!(switches("input.touchpad.tap").is_empty());
    }

    #[test]
    fn a_name_prefix_is_not_a_section() {
        // `layout.border` is not inside `layout.border-x`, and so on.
        for option in super::OPTIONS {
            for switch in section_switches(option.key) {
                let section = switch.key.rsplit_once('.').unwrap().0;
                assert!(option.key.starts_with(&format!("{section}.")));
            }
        }
    }

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn keys_are_unique() {
        let mut seen = HashSet::new();
        for option in OPTIONS {
            assert!(seen.insert(option.key), "duplicate key {}", option.key);
        }
    }

    #[test]
    fn every_page_has_a_route() {
        for page in PAGES {
            assert_eq!(Page::from_id(page.id), Some(page.page));
        }
    }

    #[test]
    fn key_matches_node_path() {
        for option in OPTIONS {
            let mut expected = option.node_path().join(".");
            if let Some(property) = option.property {
                expected.push('.');
                expected.push_str(property);
            }
            assert_eq!(expected, option.key);
        }
    }
}
