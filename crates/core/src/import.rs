//! Import shortcuts and looks from Hyprland or Sway.
//!
//! The translators are pure functions. The app shows the result before it
//! writes anything, and the whole import is one undo step.

use serde::Serialize;

use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Source {
    Hyprland,
    Sway,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hyprland => "Hyprland",
            Self::Sway => "Sway",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BindChange {
    pub key: String,
    /// The body of the bind, for example `spawn "foot"`.
    pub action: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ImportResult {
    pub source: Option<Source>,
    pub changes: Vec<(String, Value)>,
    pub binds: Vec<BindChange>,
    pub startup: Vec<String>,
    pub window_rules: Vec<String>,
    pub notes: Vec<String>,
}

impl ImportResult {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
            && self.binds.is_empty()
            && self.startup.is_empty()
            && self.window_rules.is_empty()
    }
}

/// Guess the source from a file name or its first lines.
pub fn detect(path: &std::path::Path, text: &str) -> Option<Source> {
    let name = path.to_string_lossy().to_lowercase();
    if name.contains("hypr") {
        return Some(Source::Hyprland);
    }
    if name.contains("sway") || name.ends_with("i3/config") {
        return Some(Source::Sway);
    }
    if text.contains("bind = ") || text.contains("exec-once") {
        return Some(Source::Hyprland);
    }
    if text.contains("bindsym ") {
        return Some(Source::Sway);
    }
    None
}

pub fn parse(text: &str, source: Source) -> ImportResult {
    match source {
        Source::Hyprland => parse_hyprland(text),
        Source::Sway => parse_sway(text),
    }
}

fn strip_comment(line: &str, marker: char) -> &str {
    let mut in_quote = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            _ if ch == marker && !in_quote => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_hyprland(text: &str) -> ImportResult {
    let mut result = ImportResult {
        source: Some(Source::Hyprland),
        ..Default::default()
    };
    let mut mod_key = "SUPER".to_string();
    let mut section = String::new();
    let mut general: Vec<(String, String)> = Vec::new();
    let mut input: Vec<(String, String)> = Vec::new();
    let mut decoration: Vec<(String, String)> = Vec::new();

    for raw in text.lines() {
        let line = strip_comment(raw, '#').trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('$') {
            if let Some((name, value)) = rest.split_once('=')
                && name.trim() == "mod"
            {
                mod_key = value.trim().to_string();
            }
            continue;
        }
        if let Some(open) = line.strip_suffix('{') {
            section = open.trim().to_string();
            continue;
        }
        if line == "}" {
            section.clear();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_end_matches(';').trim();
        match section.as_str() {
            "general" => general.push((key.to_string(), value.to_string())),
            "input" => input.push((key.to_string(), value.to_string())),
            "decoration" => decoration.push((key.to_string(), value.to_string())),
            _ => match key {
                "bind" | "binde" => {
                    if let Some(bind) = hypr_bind(value, &mod_key) {
                        result.binds.push(bind);
                    }
                }
                "bindl" | "bindm" | "bindr" => {
                    result.notes.push(format!("Skipped mouse bind: {value}"));
                }
                "exec-once" | "exec" => {
                    if !value.is_empty() {
                        result.startup.push(value.to_string());
                    }
                }
                "monitor" => result
                    .notes
                    .push(format!("Display setup needs a manual check: {value}")),
                "env" => {
                    if let Some((name, val)) = value.split_once(',') {
                        result.notes.push(format!(
                            "Environment variable {name} was not imported. Set it on the Startup page."
                        ));
                        let _ = val;
                    }
                }
                _ => {}
            },
        }
    }

    for (key, value) in &general {
        match key.as_str() {
            "gaps_out" => push_number(&mut result, "layout.gaps", value),
            "gaps_in" => {
                if !general.iter().any(|(k, _)| k == "gaps_out") {
                    push_number(&mut result, "layout.gaps", value);
                }
            }
            "border_size" => {
                if let Ok(size) = value.parse::<f64>() {
                    result
                        .changes
                        .push(("layout.border.off".to_string(), Value::Bool(size > 0.0)));
                    result
                        .changes
                        .push(("layout.border.width".to_string(), Value::Float(size)));
                }
            }
            "col.active_border" => {
                if let Some(color) = hypr_color(value) {
                    result.changes.push((
                        "layout.focus-ring.active-color".to_string(),
                        Value::from(color),
                    ));
                }
            }
            _ => {}
        }
    }

    for (key, value) in &input {
        match key.as_str() {
            "kb_layout" => result.changes.push((
                "input.keyboard.xkb.layout".to_string(),
                Value::from(value.as_str()),
            )),
            "kb_variant" => result.changes.push((
                "input.keyboard.xkb.variant".to_string(),
                Value::from(value.as_str()),
            )),
            "kb_options" => result.changes.push((
                "input.keyboard.xkb.options".to_string(),
                Value::from(value.as_str()),
            )),
            "follow_mouse" => {
                result.changes.push((
                    "input.focus-follows-mouse.max-scroll-amount".to_string(),
                    Value::from(if value == "0" { "0%" } else { "100%" }),
                ));
            }
            "sensitivity" => {
                if let Some(number) = hypr_number(value) {
                    result.changes.push((
                        "input.mouse.accel-speed".to_string(),
                        Value::Float((number / 10.0).clamp(-1.0, 1.0)),
                    ));
                }
            }
            "natural_scroll" => push_flag(&mut result, "input.mouse.natural-scroll", value),
            "tap-to-click" => push_flag(&mut result, "input.touchpad.tap", value),
            "accel_profile" => result.changes.push((
                "input.mouse.accel-profile".to_string(),
                Value::from(if value == "flat" { "flat" } else { "adaptive" }),
            )),
            _ => {}
        }
    }

    for (key, value) in &decoration {
        if key == "rounding"
            && let Ok(radius) = value.parse::<f64>()
        {
            result.window_rules.push(format!(
                "geometry-corner-radius {radius}\nclip-to-geometry true"
            ));
        }
    }

    result
}

fn parse_sway(text: &str) -> ImportResult {
    let mut result = ImportResult {
        source: Some(Source::Sway),
        ..Default::default()
    };
    let mut mod_key = "Mod4".to_string();
    let mut gaps_inner: Option<f64> = None;
    let mut gaps_outer: Option<f64> = None;
    for raw in text.lines() {
        let line = strip_comment(raw, '#').trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let Some(first) = words.next() else {
            continue;
        };
        match first {
            "set" => {
                if let (Some(name), Some(value)) = (words.next(), words.next())
                    && name == "$mod"
                {
                    mod_key = value.to_string();
                }
            }
            "gaps" => {
                let kind = words.next();
                if let Some(value) = words.next()
                    && let Ok(value) = value.trim_end_matches("px").parse::<f64>()
                {
                    if kind == Some("outer") {
                        gaps_outer = Some(value);
                    } else if kind == Some("inner") {
                        gaps_inner = Some(value);
                    }
                }
            }
            "default_border" => {
                let _style = words.next();
                if let Some(value) = words.next()
                    && let Ok(size) = value.parse::<f64>()
                {
                    result
                        .changes
                        .push(("layout.border.off".to_string(), Value::Bool(size > 0.0)));
                    result
                        .changes
                        .push(("layout.border.width".to_string(), Value::Float(size)));
                }
            }
            "bindsym" => {
                let rest: Vec<&str> = words.collect();
                let mut index = 0;
                while index < rest.len() && rest[index].starts_with("--") {
                    index += 1;
                }
                if index + 1 < rest.len() {
                    let key = translate_keys(rest[index], &mod_key);
                    let action = sway_action(&rest[index + 1..]);
                    if let Some(action) = action {
                        result.binds.push(BindChange { key, action });
                    } else {
                        result.notes.push(format!(
                            "No niri equivalent for: {}",
                            rest[index + 1..].join(" ")
                        ));
                    }
                }
            }
            "exec" | "exec_always" => {
                let command = words.collect::<Vec<_>>().join(" ");
                if !command.is_empty() {
                    result.startup.push(command);
                }
            }
            "input" => {
                let rest: Vec<&str> = words.collect();
                let mut index = 0;
                while index < rest.len() {
                    match rest[index] {
                        "xkb_layout" if index + 1 < rest.len() => {
                            result.changes.push((
                                "input.keyboard.xkb.layout".to_string(),
                                Value::from(rest[index + 1]),
                            ));
                            index += 2;
                        }
                        "xkb_variant" if index + 1 < rest.len() => {
                            result.changes.push((
                                "input.keyboard.xkb.variant".to_string(),
                                Value::from(rest[index + 1]),
                            ));
                            index += 2;
                        }
                        "xkb_options" if index + 1 < rest.len() => {
                            result.changes.push((
                                "input.keyboard.xkb.options".to_string(),
                                Value::from(rest[index + 1]),
                            ));
                            index += 2;
                        }
                        "natural_scroll" if index + 1 < rest.len() => {
                            push_flag(
                                &mut result,
                                "input.touchpad.natural-scroll",
                                rest[index + 1],
                            );
                            index += 2;
                        }
                        "tap" if index + 1 < rest.len() => {
                            push_flag(&mut result, "input.touchpad.tap", rest[index + 1]);
                            index += 2;
                        }
                        _ => index += 1,
                    }
                }
            }
            "output" => result
                .notes
                .push(format!("Display setup needs a manual check: {line}")),
            _ => {}
        }
    }
    if let Some(value) = gaps_outer.or(gaps_inner) {
        result
            .changes
            .push(("layout.gaps".to_string(), Value::Float(value)));
    }
    result
}

fn push_number(result: &mut ImportResult, key: &str, raw: &str) {
    if let Ok(value) = raw.trim_end_matches("px").parse::<f64>() {
        result.changes.push((key.to_string(), Value::Float(value)));
    }
}

fn push_flag(result: &mut ImportResult, key: &str, raw: &str) {
    let value = matches!(raw, "1" | "true" | "yes" | "on");
    result.changes.push((key.to_string(), Value::Bool(value)));
}

fn hypr_number(raw: &str) -> Option<f64> {
    raw.trim().parse::<f64>().ok()
}

fn hypr_color(raw: &str) -> Option<String> {
    let value = raw.trim();
    if let Some(inner) = value
        .strip_prefix("rgba(")
        .and_then(|v| v.strip_suffix(')'))
    {
        let rgba = inner.trim_start_matches("0x").trim_start_matches('#');
        return Some(format!("#{rgba}"));
    }
    if let Some(inner) = value.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
        let rgb = inner.trim_start_matches("0x").trim_start_matches('#');
        return Some(format!("#{rgb}"));
    }
    if let Some(inner) = value.strip_prefix("0x") {
        return Some(format!("#{inner}"));
    }
    if value.starts_with('#') {
        return Some(value.to_string());
    }
    None
}

fn hypr_bind(value: &str, mod_key: &str) -> Option<BindChange> {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let key = translate_keys(&parts[0..2].join("+"), mod_key);
    let dispatcher = parts[2];
    let args = parts.get(3).copied().unwrap_or_default();
    let action = hypr_action(dispatcher, args)?;
    Some(BindChange { key, action })
}

fn hypr_action(dispatcher: &str, args: &str) -> Option<String> {
    let mapped = match dispatcher {
        "exec" => return Some(format!("spawn-sh {}", quote(args))),
        "killactive" => "close-window".to_string(),
        "fullscreen" => "fullscreen-window".to_string(),
        "togglefloating" => "toggle-window-floating".to_string(),
        "workspace" => format!("focus-workspace {}", workspace_index(args)),
        "movetoworkspace" | "movetoworkspacesilent" => {
            format!("move-column-to-workspace {}", workspace_index(args))
        }
        "movefocus" => match args {
            "l" => "focus-column-left".to_string(),
            "r" => "focus-column-right".to_string(),
            "u" => "focus-window-up".to_string(),
            "d" => "focus-window-down".to_string(),
            _ => return None,
        },
        "movewindow" => match args {
            "l" => "move-column-left".to_string(),
            "r" => "move-column-right".to_string(),
            "u" => "move-window-up".to_string(),
            "d" => "move-window-down".to_string(),
            _ => return None,
        },
        "focusmonitor" => match args {
            "l" => "focus-monitor-left".to_string(),
            "r" => "focus-monitor-right".to_string(),
            "u" => "focus-monitor-up".to_string(),
            "d" => "focus-monitor-down".to_string(),
            _ => return None,
        },
        "exit" => "quit".to_string(),
        "pseudo" => "toggle-window-floating".to_string(),
        "swapwindow" => return None,
        _ => return None,
    };
    Some(mapped)
}

fn sway_action(args: &[&str]) -> Option<String> {
    match args.first().copied()? {
        "exec" => Some(format!("spawn-sh {}", quote(&args[1..].join(" ")))),
        "kill" => Some("close-window".to_string()),
        "fullscreen" => Some("fullscreen-window".to_string()),
        "floating" if args.get(1) == Some(&"toggle") => Some("toggle-window-floating".to_string()),
        "focus" => match args.get(1).copied()? {
            "left" => Some("focus-column-left".to_string()),
            "right" => Some("focus-column-right".to_string()),
            "up" => Some("focus-window-up".to_string()),
            "down" => Some("focus-window-down".to_string()),
            _ => None,
        },
        "move" if args.get(1) == Some(&"container") => {
            let rest: Vec<&str> = args[2..].to_vec();
            if rest.first() == Some(&"to") && rest.get(1) == Some(&"workspace") {
                return rest
                    .get(2)
                    .map(|ws| format!("move-column-to-workspace {ws}"));
            }
            match rest.first().copied()? {
                "left" => Some("move-column-left".to_string()),
                "right" => Some("move-column-right".to_string()),
                "up" => Some("move-window-up".to_string()),
                "down" => Some("move-window-down".to_string()),
                _ => None,
            }
        }
        "workspace" => args.get(1).map(|ws| format!("focus-workspace {ws}")),
        "reload" => Some("load-config-file".to_string()),
        "exit" => Some("quit".to_string()),
        _ => None,
    }
}

fn workspace_index(raw: &str) -> String {
    raw.split(',')
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_start_matches("name:")
        .to_string()
}

fn quote(value: &str) -> String {
    Value::from(value).literal()
}

/// Translate a key combination from Hyprland or Sway syntax to niri syntax.
pub fn translate_keys(raw: &str, mod_key: &str) -> String {
    let raw = raw.replace(' ', "+");
    let mut out = Vec::new();
    for part in raw.split('+') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let lower = part.to_lowercase();
        let translated = match lower.as_str() {
            "$mod" | "mod4" | "super" | "meta" | "win" => "Mod".to_string(),
            "mod1" | "alt" => "Alt".to_string(),
            "control" | "ctrl" => "Ctrl".to_string(),
            "shift" => "Shift".to_string(),
            "mod2" | "mod3" | "mod5" => continue,
            "return" | "enter" => "Return".to_string(),
            "space" => "Space".to_string(),
            "escape" | "esc" => "Escape".to_string(),
            "tab" => "Tab".to_string(),
            "backspace" => "BackSpace".to_string(),
            "delete" | "del" => "Delete".to_string(),
            "insert" | "ins" => "Insert".to_string(),
            "home" => "Home".to_string(),
            "end" => "End".to_string(),
            "page_up" | "prior" | "pageup" => "Page_Up".to_string(),
            "page_down" | "next" | "pagedown" => "Page_Down".to_string(),
            "left" => "Left".to_string(),
            "right" => "Right".to_string(),
            "up" => "Up".to_string(),
            "down" => "Down".to_string(),
            "print" | "printscreen" => "Print".to_string(),
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
            other if other.starts_with("xf86") => {
                format!("XF86{}", &part[4..])
            }
            other if other.len() == 1 => other.to_uppercase(),
            _ => part.to_string(),
        };
        let _ = mod_key;
        out.push(translated);
    }
    out.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprland_basics() {
        let text = r#"
$mod = SUPER
general {
    gaps_in = 5
    gaps_out = 20
    border_size = 2
    col.active_border = rgba(33ccffee)
}
input {
    kb_layout = us,tr
    follow_mouse = 1
}
bind = $mod, Return, exec, foot
bind = $mod, Q, killactive,
bind = $mod, 1, workspace, 1
exec-once = waybar
"#;
        let result = parse(text, Source::Hyprland);
        assert_eq!(result.source, Some(Source::Hyprland));
        assert!(
            result
                .changes
                .iter()
                .any(|(key, value)| key == "layout.gaps" && value.as_f64() == Some(20.0))
        );
        assert!(
            result
                .binds
                .iter()
                .any(|bind| bind.key == "Mod+Return" && bind.action == "spawn-sh \"foot\"")
        );
        assert!(result.binds.iter().any(|bind| bind.key == "Mod+Q"));
        assert_eq!(result.startup, vec!["waybar"]);
    }

    #[test]
    fn sway_basics() {
        let text = r#"
set $mod Mod4
gaps inner 5
gaps outer 10
bindsym $mod+Return exec foot
bindsym $mod+Shift+q kill
bindsym $mod+1 workspace 1
exec waybar
"#;
        let result = parse(text, Source::Sway);
        assert!(
            result
                .binds
                .iter()
                .any(|bind| bind.key == "Mod+Return" && bind.action == "spawn-sh \"foot\"")
        );
        assert!(result.binds.iter().any(|bind| bind.key == "Mod+Shift+Q"));
        assert_eq!(
            result
                .changes
                .iter()
                .filter(|(key, _)| key == "layout.gaps")
                .count(),
            1
        );
        assert!(
            result
                .changes
                .iter()
                .any(|(key, value)| key == "layout.gaps" && value.as_f64() == Some(10.0))
        );
        assert_eq!(result.startup, vec!["waybar"]);
    }

    #[test]
    fn keys_translate() {
        assert_eq!(
            translate_keys("$mod+Shift+Return", "Super"),
            "Mod+Shift+Return"
        );
        assert_eq!(translate_keys("Mod4+1", "Super"), "Mod+1");
        assert_eq!(translate_keys("Control+alt+t", "Super"), "Ctrl+Alt+T");
    }
}
