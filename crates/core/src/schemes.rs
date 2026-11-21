//! Shortcut schemes. A scheme adds or replaces shortcuts in the override
//! file, so the user's own file keeps its comments and layout.

#[derive(Debug, Clone, Copy)]
pub struct Scheme {
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub binds: &'static [(&'static str, &'static str)],
}

pub const SCHEMES: &[Scheme] = &[
    Scheme {
        id: "gnome",
        label: "GNOME-like",
        detail: "Terminal on Mod+Return, launcher on Mod+D, close on Mod+Q",
        binds: &[
            ("Mod+Return", "spawn \"foot\""),
            ("Mod+D", "spawn-sh \"fuzzel\""),
            ("Mod+Q", "close-window"),
            ("Mod+Shift+Q", "quit"),
            ("Mod+Tab", "toggle-overview"),
        ],
    },
    Scheme {
        id: "macos",
        label: "macOS-like",
        detail: "Close on Mod+W, quit on Mod+Q, launcher on Mod+Space",
        binds: &[
            ("Mod+W", "close-window"),
            ("Mod+Q", "quit"),
            ("Mod+Space", "spawn-sh \"fuzzel\""),
            ("Mod+Tab", "toggle-overview"),
            ("Mod+Shift+3", "screenshot"),
        ],
    },
    Scheme {
        id: "vim",
        label: "Vim-style",
        detail: "H J K L focus and move, numbers switch workspaces",
        binds: &[
            ("Mod+H", "focus-column-left"),
            ("Mod+J", "focus-window-down"),
            ("Mod+K", "focus-window-up"),
            ("Mod+L", "focus-column-right"),
            ("Mod+Shift+H", "move-column-left"),
            ("Mod+Shift+J", "move-window-down"),
            ("Mod+Shift+K", "move-window-up"),
            ("Mod+Shift+L", "move-column-right"),
            ("Mod+1", "focus-workspace 1"),
            ("Mod+2", "focus-workspace 2"),
            ("Mod+3", "focus-workspace 3"),
            ("Mod+4", "focus-workspace 4"),
            ("Mod+5", "focus-workspace 5"),
            ("Mod+6", "focus-workspace 6"),
            ("Mod+7", "focus-workspace 7"),
            ("Mod+8", "focus-workspace 8"),
            ("Mod+9", "focus-workspace 9"),
        ],
    },
];

pub fn scheme(id: &str) -> Option<&'static Scheme> {
    SCHEMES.iter().find(|scheme| scheme.id == id)
}
