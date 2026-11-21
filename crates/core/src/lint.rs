//! Checks that find real problems in a config, with a fix when one is safe.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::doc::{NodeRef, Tree};
use crate::niri;
use crate::schema;
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fix {
    Set { key: String, value: Value },
    Reset { key: String },
    RemoveBind { key: String },
    AddBind { key: String, action: String },
}

#[derive(Debug, Clone)]
pub struct Problem {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub fix: Option<Fix>,
}

impl Problem {
    fn new(
        id: impl Into<String>,
        severity: Severity,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            severity,
            title: title.into(),
            detail: detail.into(),
            fix: None,
        }
    }

    fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

/// Data from the running compositor. Empty when niri is not running.
#[derive(Debug, Default, Clone)]
pub struct LintContext {
    pub outputs: Vec<niri_ipc::Output>,
    pub windows: Vec<niri_ipc::Window>,
    pub managed: Vec<String>,
}

impl LintContext {
    pub fn from_system() -> Self {
        Self {
            outputs: niri::outputs(),
            windows: niri::windows(),
            managed: Vec::new(),
        }
    }
}

/// Run every check.
pub fn check(tree: &Tree, context: &LintContext) -> Vec<Problem> {
    let mut problems = Vec::new();
    let binds = collect_binds(tree);

    check_duplicate_binds(&binds, &mod_key(tree), &mut problems);
    check_terminal(&binds, &mut problems);
    check_startup(tree, &mut problems);
    check_includes(tree, &mut problems);
    check_managed_overrides(tree, &mut problems);
    check_outputs(tree, context, &mut problems);
    check_rules(tree, context, &mut problems);
    check_validation(tree, &mut problems);

    problems.sort_by_key(|problem| match problem.severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    });
    problems
}

/// One key binding.
#[derive(Debug, Clone)]
pub struct Bind {
    pub key: String,
    pub action: String,
    pub args: Vec<String>,
    pub file: usize,
}

impl Bind {
    pub fn action_label(&self) -> String {
        action_label(&self.action)
    }
}

pub fn collect_binds(tree: &Tree) -> Vec<Bind> {
    let mut out = Vec::new();
    for node in tree.all_binds() {
        if let Some(bind) = parse_bind(tree, node) {
            out.push(bind);
        }
    }
    out
}

fn parse_bind(tree: &Tree, node: &NodeRef) -> Option<Bind> {
    let text = &tree.files[node.file].text[node.node_span.clone()];
    let doc = kdl::KdlDocument::parse_v1(text).ok()?;
    let parsed = doc.nodes().first()?;
    let child = parsed.children()?.nodes().first()?;
    let action = child.name().value().to_string();
    let args = child
        .entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .filter_map(|entry| entry.value().as_string().map(str::to_string))
        .collect();
    Some(Bind {
        key: node.name.clone(),
        action,
        args,
        file: node.file,
    })
}

fn normalize_key(key: &str, mod_key: &str) -> String {
    let parts: Vec<&str> = key.split('+').collect();
    if parts.len() <= 1 {
        return key.to_string();
    }
    let (modifiers, rest) = parts.split_at(parts.len() - 1);
    let mut mods: Vec<String> = modifiers
        .iter()
        .map(|m| match *m {
            "Mod" => mod_key.to_string(),
            other => other.to_string(),
        })
        .collect();
    mods.sort();
    mods.push(rest[0].to_string());
    mods.join("+")
}

pub fn mod_key(tree: &Tree) -> String {
    let value = tree
        .winner(&["input".to_string(), "mod-key".to_string()])
        .and_then(|node| node.entry(None).map(|e| e.value.to_string()))
        .unwrap_or_else(|| "Mod".to_string());
    // Mod means Super on a TTY, which is where niri runs.
    if value == "Mod" {
        "Super".to_string()
    } else {
        value
    }
}

fn check_duplicate_binds(binds: &[Bind], mod_key: &str, problems: &mut Vec<Problem>) {
    let mut seen: HashMap<String, &Bind> = HashMap::new();
    for bind in binds {
        let normalized = normalize_key(&bind.key, mod_key);
        if let Some(first) = seen.get(&normalized) {
            problems.push(
                Problem::new(
                    format!("duplicate-bind-{}", bind.key),
                    Severity::Warning,
                    format!("{} is bound twice", bind.key),
                    format!(
                        "{} and {} use the same keys. niri uses the second one.",
                        first.key, bind.key
                    ),
                )
                .with_fix(Fix::RemoveBind {
                    key: first.key.clone(),
                }),
            );
        } else {
            seen.insert(normalized, bind);
        }
    }
}

pub fn is_terminal(action: &str, args: &[String]) -> bool {
    if action != "spawn" && action != "spawn-sh" {
        return false;
    }
    const TERMS: &[&str] = &[
        "foot",
        "alacritty",
        "kitty",
        "wezterm",
        "ghostty",
        "gnome-terminal",
        "konsole",
        "xterm",
        "st",
        "urxvt",
        "termite",
        "tilix",
        "sakura",
        "rio",
        "warp",
    ];
    const LAUNCHERS: &[&str] = &[
        "fuzzel",
        "wofi",
        "rofi",
        "anyrun",
        "vicinae",
        "walker",
        "launcher",
        "noctalia msg panel-toggle launcher",
        "noctalia msg launcher",
    ];
    let text = args.join(" ");
    TERMS.iter().any(|term| text.contains(term)) || LAUNCHERS.iter().any(|l| text.contains(l))
}

fn check_terminal(binds: &[Bind], problems: &mut Vec<Problem>) {
    let has = binds
        .iter()
        .any(|bind| is_terminal(&bind.action, &bind.args));
    if !has {
        problems.push(
            Problem::new(
                "no-terminal",
                Severity::Warning,
                "No shortcut opens a terminal or launcher",
                "If your config stops working, you need a terminal to repair it. Add the default shortcut.",
            )
            .with_fix(Fix::AddBind {
                key: "Mod+T".to_string(),
                action: "spawn \"foot\"".to_string(),
            }),
        );
    }
}

fn check_startup(tree: &Tree, problems: &mut Vec<Problem>) {
    for node in tree.top_level("spawn-at-startup") {
        let Some(entry) = node.entry(None) else {
            continue;
        };
        let Some(program) = entry.value.as_str() else {
            continue;
        };
        if program.trim().is_empty() {
            continue;
        }
        if !command_exists(program) {
            problems.push(Problem::new(
                format!("missing-program-{program}"),
                Severity::Warning,
                format!("{program} is not installed"),
                "niri starts this program at startup, but the program is not in PATH. Remove it or install it.",
            ));
        }
    }
}

fn command_exists(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).exists();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

fn check_includes(tree: &Tree, problems: &mut Vec<Problem>) {
    for include in &tree.includes {
        if !include.exists && !include.optional {
            problems.push(Problem::new(
                format!("missing-include-{}", include.path.display()),
                Severity::Error,
                "An included file is missing",
                format!("niri cannot load {}.", include.path.display()),
            ));
        }
    }
}

fn check_managed_overrides(tree: &Tree, problems: &mut Vec<Problem>) {
    for option in schema::OPTIONS {
        let occurrences = tree.occurrences(&option.node_path());
        if occurrences.len() < 2 {
            continue;
        }
        let Some(last) = occurrences.last() else {
            continue;
        };
        let file = &tree.files[last.file];
        let Some(reason) = file.managed.reason() else {
            continue;
        };
        let earlier = occurrences
            .iter()
            .any(|node| node.file != last.file && tree.files[node.file].managed.is_writable());
        if earlier {
            problems.push(Problem::new(
                format!("override-{}", option.key),
                Severity::Info,
                format!("{} is set twice", option.label),
                format!(
                    "{} also sets this value, later in the file order. niri uses that value, so your value has no effect.",
                    reason.trim_start_matches("Set by ")
                ),
            ));
        }
    }
}

fn check_outputs(tree: &Tree, context: &LintContext, problems: &mut Vec<Problem>) {
    if context.outputs.is_empty() {
        return;
    }
    let connected: Vec<&str> = context
        .outputs
        .iter()
        .map(|output| output.name.as_str())
        .collect();
    for node in tree.top_level("output") {
        let Some(name) = node
            .entry(None)
            .and_then(|entry| entry.value.as_str().map(str::to_string))
        else {
            continue;
        };
        if !connected.iter().any(|connected| *connected == name) {
            problems.push(Problem::new(
                format!("disconnected-output-{name}"),
                Severity::Info,
                format!("Display {name} is not connected"),
                "The config contains settings for this display. They apply when you connect the display again.",
            ));
        }
    }
}

fn check_rules(tree: &Tree, context: &LintContext, problems: &mut Vec<Problem>) {
    if context.windows.is_empty() {
        return;
    }
    let ids: Vec<&str> = context
        .windows
        .iter()
        .filter_map(|window| window.app_id.as_deref())
        .collect();
    for node in tree.top_level("window-rule") {
        let Some(text) = tree.files[node.file].text.get(node.node_span.clone()) else {
            continue;
        };
        let Some(start) = text.find("app-id=") else {
            continue;
        };
        let rest = &text[start + 7..];
        let Some(quoted) = rest.strip_prefix('"') else {
            continue;
        };
        let Some(end) = quoted.find('"') else {
            continue;
        };
        let pattern = &quoted[..end];
        let pattern = pattern.trim_start_matches('^').trim_end_matches('$');
        if pattern.is_empty() || pattern.starts_with('(') {
            continue;
        }
        if !ids.iter().any(|id| id.contains(pattern)) {
            problems.push(Problem::new(
                format!("unused-rule-{pattern}"),
                Severity::Info,
                format!("No open window matches {pattern}"),
                "The rule is valid. This is only a hint. The app is not running now.",
            ));
        }
    }
}

fn check_validation(tree: &Tree, problems: &mut Vec<Problem>) {
    let output = std::process::Command::new("niri")
        .arg("validate")
        .arg("-c")
        .arg(&tree.main)
        .output();
    let Ok(output) = output else {
        return;
    };
    if output.status.success() {
        return;
    }
    let raw = String::from_utf8_lossy(&output.stderr);
    let ansi = regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").expect("valid regex");
    let message = ansi.replace_all(&raw, "").trim().to_string();
    problems.push(Problem::new(
        "invalid-config",
        Severity::Error,
        "niri does not accept this config",
        if message.is_empty() {
            "Open the config file and look for the reported line.".to_string()
        } else {
            message
        },
    ));
}

/// A plain-words name for a bind action.
pub fn action_label(action: &str) -> String {
    let words = action.replace('-', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

/// Human text for a fix button.
pub fn fix_label(fix: &Fix) -> String {
    match fix {
        Fix::Set { .. } | Fix::Reset { .. } => "Apply".to_string(),
        Fix::RemoveBind { .. } => "Remove the first shortcut".to_string(),
        Fix::AddBind { .. } => "Add the default shortcut".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc;

    fn tree(text: &str) -> (tempfile::TempDir, Tree) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, text).unwrap();
        let tree = doc::load(&path).unwrap();
        (dir, tree)
    }

    #[test]
    fn finds_duplicate_binds() {
        let (_dir, tree) = tree("binds {\n  Mod+Q { close-window; }\n  Mod+Q { quit; }\n}\n");
        let problems = check(&tree, &LintContext::default());
        assert!(problems.iter().any(|p| p.id.starts_with("duplicate-bind")));
    }

    #[test]
    fn finds_a_missing_terminal() {
        let (_dir, tree) = tree("binds {\n  Mod+Q { close-window; }\n}\n");
        let problems = check(&tree, &LintContext::default());
        assert!(problems.iter().any(|p| p.id == "no-terminal"));
    }

    #[test]
    fn a_terminal_bind_is_enough() {
        let (_dir, tree) =
            tree("binds {\n  Mod+T { spawn \"foot\"; }\n  Mod+Q { close-window; }\n}\n");
        let problems = check(&tree, &LintContext::default());
        assert!(!problems.iter().any(|p| p.id == "no-terminal"));
    }
}
