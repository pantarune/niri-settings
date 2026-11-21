//! Format-preserving view of the niri config and its includes.
//!
//! The KDL parser gives byte spans. Every edit is a splice of the original
//! text, so comments, whitespace and unknown nodes survive. Reading a file and
//! writing it back with no change gives the same bytes.

use std::collections::HashSet;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use kdl::{KdlDocument, KdlNode};
use thiserror::Error;

use crate::schema::{Control, OptionInfo, ToggleStyle};
use crate::value::Value;

/// Why a file must not be edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Managed {
    User,
    /// A desktop shell rewrites the file.
    Shell(String),
    /// The file comes from the Nix store.
    Nix,
    ReadOnly,
    /// `/etc/niri/config.kdl`, used because the user has no config of their
    /// own. It belongs to the system; changes need a personal copy.
    System,
}

impl Managed {
    pub fn is_writable(&self) -> bool {
        matches!(self, Self::User)
    }

    pub fn reason(&self) -> Option<String> {
        match self {
            Self::User => None,
            Self::Shell(name) => Some(format!("Set by {name}")),
            Self::Nix => Some("Set by Nix".to_string()),
            Self::ReadOnly => Some("Read-only file".to_string()),
            Self::System => Some("System-wide config".to_string()),
        }
    }
}

/// One file in the include closure.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    pub doc: KdlDocument,
    pub managed: Managed,
    pub is_main: bool,
}

/// One value carrier in the merged config.
#[derive(Debug, Clone)]
pub struct EntryRef {
    pub name: Option<String>,
    pub value: Value,
    /// Byte span of the value literal. For a property this is the part after
    /// `=`.
    pub span: Range<usize>,
}

/// One KDL node, with its place in the merged config.
#[derive(Debug, Clone)]
pub struct NodeRef {
    pub path: Vec<String>,
    pub file: usize,
    pub name: String,
    pub node_span: Range<usize>,
    pub name_span: Range<usize>,
    pub children_span: Option<Range<usize>>,
    pub entries: Vec<EntryRef>,
    /// The whole line or lines that hold the node, for clean deletion.
    pub full_span: Range<usize>,
}

impl NodeRef {
    pub fn is_section(&self) -> bool {
        self.children_span.is_some()
    }

    pub fn entry(&self, property: Option<&str>) -> Option<&EntryRef> {
        match property {
            Some(name) => self
                .entries
                .iter()
                .find(|e| e.name.as_deref() == Some(name)),
            None => self.entries.iter().find(|e| e.name.is_none()),
        }
    }

    /// The effective scalar. A bare node means true.
    pub fn scalar(&self, property: Option<&str>) -> Value {
        self.entry(property)
            .map(|e| e.value.clone())
            .unwrap_or(Value::Bool(true))
    }
}

/// An include line.
#[derive(Debug, Clone)]
pub struct IncludeRef {
    pub file: usize,
    pub node_span: Range<usize>,
    /// Byte span of the path argument in the including file.
    pub path_span: Range<usize>,
    pub path: PathBuf,
    pub optional: bool,
    pub exists: bool,
}

/// The loaded config.
#[derive(Debug, Clone)]
pub struct Tree {
    pub main: PathBuf,
    pub files: Vec<SourceFile>,
    pub nodes: Vec<NodeRef>,
    pub includes: Vec<IncludeRef>,
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("include loop at {0}")]
    IncludeLoop(PathBuf),
    #[error("includes are nested more than 10 levels deep at {0}")]
    TooDeep(PathBuf),
}

/// Read a config file and every file that it includes.
pub fn load(main: &Path) -> Result<Tree, LoadError> {
    let main = resolve_path(main);
    let mut tree = Tree {
        main: main.clone(),
        files: Vec::new(),
        nodes: Vec::new(),
        includes: Vec::new(),
    };
    let mut stack = HashSet::new();
    load_file(&mut tree, &main, 0, true, &mut stack)?;
    Ok(tree)
}

fn resolve_path(path: &Path) -> PathBuf {
    if path.exists() {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

fn load_file(
    tree: &mut Tree,
    path: &Path,
    depth: usize,
    is_main: bool,
    stack: &mut HashSet<PathBuf>,
) -> Result<(), LoadError> {
    if depth >= 10 {
        return Err(LoadError::TooDeep(path.to_path_buf()));
    }
    let path = resolve_path(path);
    if !stack.insert(path.clone()) {
        return Err(LoadError::IncludeLoop(path));
    }
    let text = fs::read_to_string(&path).map_err(|source| LoadError::Read {
        path: path.clone(),
        source,
    })?;
    let doc = KdlDocument::parse_v1(&text).map_err(|error| LoadError::Parse {
        path: path.clone(),
        message: format!("{error}"),
    })?;
    let idx = tree.files.len();
    let managed = classify(&path, &text, is_main);
    tree.files.push(SourceFile {
        path: path.clone(),
        text: text.clone(),
        doc,
        managed,
        is_main,
    });

    walk_children(tree, idx, &text, depth, stack)?;
    stack.remove(&path);
    Ok(())
}

fn classify(path: &Path, text: &str, is_main: bool) -> Managed {
    let path_text = path.to_string_lossy();
    if path_text.starts_with("/nix/store") {
        return Managed::Nix;
    }
    if is_main {
        return Managed::User;
    }
    let lower = path_text.to_lowercase();
    for (name, display) in [
        ("noctalia", "Noctalia"),
        ("dank", "DankMaterialShell"),
        ("dms", "DankMaterialShell"),
        ("stylix", "Stylix"),
        ("matugen", "matugen"),
        ("caelestia", "Caelestia"),
    ] {
        if lower.contains(name) {
            return Managed::Shell(display.to_string());
        }
    }
    let head: String = text
        .lines()
        .take(20)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    for phrase in [
        "managed by",
        "do not edit",
        "automatically generated",
        "auto-generated",
    ] {
        if head.contains(phrase) {
            return Managed::Shell(shell_name_from_text(&head));
        }
    }
    if fs::metadata(path)
        .map(|m| m.permissions().readonly())
        .unwrap_or(false)
    {
        return Managed::ReadOnly;
    }
    Managed::User
}

fn shell_name_from_text(head: &str) -> String {
    for (needle, name) in [
        ("noctalia", "Noctalia"),
        ("dank", "DankMaterialShell"),
        ("stylix", "Stylix"),
        ("matugen", "matugen"),
        ("caelestia", "Caelestia"),
    ] {
        if head.contains(needle) {
            return name.to_string();
        }
    }
    "another program".to_string()
}

fn walk_children(
    tree: &mut Tree,
    file: usize,
    text: &str,
    depth: usize,
    stack: &mut HashSet<PathBuf>,
) -> Result<(), LoadError> {
    let doc = tree.files[file].doc.clone();
    let base = tree.files[file]
        .path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    for node in doc.nodes() {
        let name = node.name().value().to_string();
        if name == "include"
            && let Some((relative, optional, path_span)) = read_include(node, text)
        {
            let target = if relative.is_absolute() {
                relative.clone()
            } else {
                base.join(&relative)
            };
            let exists = target.exists();
            tree.includes.push(IncludeRef {
                file,
                node_span: range(node.span()),
                path_span,
                path: target.clone(),
                optional,
                exists,
            });
            if exists {
                load_file(tree, &target, depth + 1, false, stack)?;
            } else if !optional {
                return Err(LoadError::Read {
                    path: target,
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "included file does not exist",
                    ),
                });
            }
            continue;
        }

        let path = vec![name.clone()];
        tree.nodes
            .push(make_node_ref(file, node, path.clone(), text));
        if let Some(children) = node.children() {
            if name == "binds" {
                for bind in children.nodes() {
                    let mut bind_path = path.clone();
                    bind_path.push(bind.name().value().to_string());
                    tree.nodes.push(make_node_ref(file, bind, bind_path, text));
                }
            } else {
                walk_nested(tree, file, children, &path, text);
            }
        }
    }
    Ok(())
}

fn walk_nested(tree: &mut Tree, file: usize, doc: &KdlDocument, parent: &[String], text: &str) {
    for node in doc.nodes() {
        let mut path = parent.to_vec();
        path.push(node.name().value().to_string());
        tree.nodes
            .push(make_node_ref(file, node, path.clone(), text));
        if let Some(children) = node.children() {
            walk_nested(tree, file, children, &path, text);
        }
    }
}

/// Sections where niri replaces the whole section instead of merging it.
pub fn is_non_merging(path: &[String]) -> bool {
    let joined = path.join(".");
    joined == "layout.struts"
        || joined == "layout.preset-column-widths"
        || joined == "layout.preset-window-heights"
        || matches!(
            joined.as_str(),
            "input.keyboard"
                | "input.touchpad"
                | "input.mouse"
                | "input.trackpoint"
                | "input.trackball"
                | "input.tablet"
                | "input.touch"
        )
        || (path.first().map(String::as_str) == Some("animations") && path.len() >= 2)
}

fn make_node_ref(file: usize, node: &KdlNode, path: Vec<String>, text: &str) -> NodeRef {
    let node_span = range(node.span());
    let children_span = node.children().map(|c| range(c.span()));
    let name_span = range(node.name().span());
    let mut entries = Vec::new();
    for entry in node.entries() {
        let span = range(entry.span());
        let value_span = if entry.name().is_some() {
            match text[span.clone()].find('=') {
                Some(eq) => {
                    let start = span.start + eq + 1;
                    let value = &text[start..span.end];
                    let trimmed = value.trim_start();
                    start + (value.len() - trimmed.len())..span.end
                }
                None => span.clone(),
            }
        } else {
            span.clone()
        };
        entries.push(EntryRef {
            name: entry.name().map(|n| n.value().to_string()),
            value: Value::from_kdl(entry.value()),
            span: value_span,
        });
    }
    let (full_start, full_end) = line_span(text, node_span.clone());
    NodeRef {
        path,
        file,
        name: node.name().value().to_string(),
        node_span,
        name_span,
        children_span,
        entries,
        full_span: full_start..full_end,
    }
}

fn read_include(node: &KdlNode, _text: &str) -> Option<(PathBuf, bool, Range<usize>)> {
    let mut path = None;
    let mut path_span = None;
    for entry in node.entries() {
        if entry.name().is_none()
            && let Some(value) = entry.value().as_string()
        {
            path = Some(expand_home(PathBuf::from(value)));
            path_span = Some(range(entry.span()));
            break;
        }
    }
    let path = path?;
    let path_span = path_span?;
    let mut optional = false;
    for entry in node.entries() {
        if entry.name().map(|n| n.value()) == Some("optional") {
            optional = entry.value().as_bool().unwrap_or(false);
        }
    }
    Some((path, optional, path_span))
}

fn expand_home(path: PathBuf) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    path
}

pub fn range(span: miette::SourceSpan) -> Range<usize> {
    span.offset()..span.offset() + span.len()
}

fn line_span(text: &str, span: Range<usize>) -> (usize, usize) {
    let start = text[..span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = text[span.end..]
        .find('\n')
        .map(|i| span.end + i + 1)
        .unwrap_or(text.len());
    (start, end)
}

impl Tree {
    pub fn file(&self, index: usize) -> &SourceFile {
        &self.files[index]
    }

    pub fn main_index(&self) -> usize {
        self.files
            .iter()
            .position(|f| f.is_main)
            .unwrap_or_default()
    }

    pub fn file_index(&self, path: &Path) -> Option<usize> {
        let path = resolve_path(path);
        self.files.iter().position(|f| f.path == path)
    }

    /// All nodes that define a path, in merge order.
    pub fn occurrences(&self, path: &[String]) -> Vec<&NodeRef> {
        let mut found: Vec<&NodeRef> = self.nodes.iter().filter(|node| node.path == path).collect();

        let mut cut: Option<&NodeRef> = None;
        for i in (1..path.len()).rev() {
            let prefix = &path[..i];
            if is_non_merging(prefix) {
                cut = self.nodes.iter().rev().find(|node| node.path == prefix);
                break;
            }
        }
        if let Some(section) = cut {
            found.retain(|node| {
                node.file == section.file
                    && node.node_span.start >= section.node_span.start
                    && node.node_span.end <= section.node_span.end
            });
        }
        found
    }

    pub fn winner(&self, path: &[String]) -> Option<&NodeRef> {
        self.occurrences(path).into_iter().last()
    }

    /// The winning section node for a path, if any.
    pub fn section(&self, path: &[String]) -> Option<&NodeRef> {
        self.nodes
            .iter()
            .rev()
            .find(|node| node.path == path && node.is_section())
    }

    /// The winning section node in one file.
    pub fn section_in(&self, path: &[String], file: usize) -> Option<&NodeRef> {
        self.nodes
            .iter()
            .rev()
            .find(|node| node.path == path && node.is_section() && node.file == file)
    }

    /// Every bind node, in merge order, including overridden keys.
    pub fn all_binds(&self) -> Vec<&NodeRef> {
        self.nodes
            .iter()
            .filter(|node| node.path.len() == 2 && node.path[0] == "binds")
            .collect()
    }

    /// Every bind node, in merge order. A later bind with the same key wins.
    pub fn binds(&self) -> Vec<&NodeRef> {
        let mut out: Vec<&NodeRef> = Vec::new();
        for node in &self.nodes {
            if node.path.len() == 2 && node.path[0] == "binds" {
                let key = node.name.clone();
                out.retain(|existing| existing.name != key);
                out.push(node);
            }
        }
        out
    }

    pub fn binds_in_file(&self, file: usize) -> Vec<&NodeRef> {
        self.binds()
            .into_iter()
            .filter(|n| n.file == file)
            .collect()
    }

    /// All nodes with a given top-level name, in order.
    pub fn top_level(&self, name: &str) -> Vec<&NodeRef> {
        self.nodes
            .iter()
            .filter(|node| node.path.len() == 1 && node.name == name)
            .collect()
    }

    pub fn children_of<'a>(&'a self, parent: &NodeRef) -> Vec<&'a NodeRef> {
        self.nodes
            .iter()
            .filter(|node| {
                node.file == parent.file
                    && node.path.len() == parent.path.len() + 1
                    && node.node_span.start >= parent.node_span.start
                    && node.node_span.end <= parent.node_span.end
            })
            .collect()
    }

    /// Read the raw scalar of an option, without applying control defaults.
    pub fn raw(&self, option: &OptionInfo) -> Option<Value> {
        let path = option.node_path();
        self.winner(&path).map(|node| node.scalar(option.property))
    }

    /// Read an option using its control type.
    pub fn read(&self, option: &OptionInfo) -> Value {
        match option.control {
            Control::Toggle {
                default,
                style: ToggleStyle::Section | ToggleStyle::OffOnly,
            } => Value::Bool(self.section_enabled(option).unwrap_or(default)),
            _ => match self.raw(option) {
                None => default_value(option),
                Some(value) => normalize(option, value),
            },
        }
    }

    /// Whether a section toggle (`on` or `off` child) is on. The last flag in
    /// merge order wins.
    pub fn section_enabled(&self, option: &OptionInfo) -> Option<bool> {
        let section_path = option.parent.to_vec();
        let mut state = None;
        for node in &self.nodes {
            if node.path.len() != section_path.len() + 1
                || node.path[..section_path.len()] != section_path[..]
            {
                continue;
            }
            if node.name == "on" {
                state = Some(true);
            } else if node.name == "off" {
                state = Some(false);
            }
        }
        state
    }

    pub fn is_set(&self, option: &OptionInfo) -> bool {
        match option.control {
            Control::Toggle {
                style: ToggleStyle::Section | ToggleStyle::OffOnly,
                ..
            } => self.section_enabled(option).is_some(),
            _ => !self.occurrences(&option.node_path()).is_empty(),
        }
    }

    pub fn read_string(&self, option: &OptionInfo) -> String {
        match self.read(option) {
            Value::String(v) => v,
            Value::Bool(v) => v.to_string(),
            Value::Int(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Null => String::new(),
        }
    }
}

/// Parse a standalone KDL document for editing. Includes are not followed.
/// The returned tree points at a temporary path, so use it in memory only.
pub fn parse_text(text: &str) -> Option<Tree> {
    let dir = tempfile::tempdir().ok()?;
    let path = dir.path().join("config.kdl");
    fs::write(&path, text).ok()?;
    load(&path).ok()
}

pub fn default_value(option: &OptionInfo) -> Value {
    match option.control {
        Control::Toggle { default, .. } => Value::Bool(default),
        Control::Number { default, .. } => Value::Float(default),
        Control::Text { default, .. }
        | Control::Color { default }
        | Control::Choice { default, .. } => Value::String(default.to_string()),
    }
}

fn normalize(option: &OptionInfo, value: Value) -> Value {
    match option.control {
        Control::Toggle { style, .. } => {
            let present = value.as_bool().unwrap_or(true);
            Value::Bool(match style {
                ToggleStyle::Presence => present,
                ToggleStyle::Section | ToggleStyle::OffOnly => present,
            })
        }
        Control::Number { .. } => Value::Float(value.as_f64().unwrap_or_default()),
        Control::Text { .. } | Control::Color { .. } | Control::Choice { .. } => match value {
            Value::String(s) => Value::String(s),
            Value::Null => Value::String(String::new()),
            other => Value::String(other.to_string()),
        },
    }
}

/// A text edit for one file, before validation.
#[derive(Debug, Clone)]
pub struct FileEdit {
    pub file: usize,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct EditError {
    pub message: String,
}

/// Change an option in one file.
pub fn set_option(
    tree: &Tree,
    option: &OptionInfo,
    value: &Value,
    file: usize,
) -> Result<FileEdit, EditError> {
    let before = tree.file(file).text.clone();
    let after = match option.control {
        Control::Toggle {
            style: ToggleStyle::Section | ToggleStyle::OffOnly,
            ..
        } => set_section_toggle(tree, option, value.as_bool().unwrap_or(false), file)?,
        Control::Toggle {
            style: ToggleStyle::Presence,
            ..
        } => set_presence_toggle(tree, option, value.as_bool().unwrap_or(false), file)?,
        _ => set_scalar(tree, option, value, file)?,
    };
    Ok(FileEdit {
        file,
        before,
        after,
    })
}

/// Remove an option so that niri's default applies again.
pub fn reset_option(tree: &Tree, option: &OptionInfo, file: usize) -> Result<FileEdit, EditError> {
    let before = tree.file(file).text.clone();
    let mut text = before.clone();
    match option.control {
        Control::Toggle {
            style: ToggleStyle::Section,
            ..
        } => {
            let section_path = option.parent.to_vec();
            let flags: Vec<&NodeRef> = tree
                .nodes
                .iter()
                .filter(|node| {
                    node.file == file
                        && node.path.len() == section_path.len() + 1
                        && node.path[..section_path.len()] == section_path[..]
                        && (node.name == "on" || node.name == "off")
                })
                .collect();
            for node in flags.iter().rev() {
                text = remove_node(&text, node);
            }
        }
        _ => {
            if let Some(node) = tree
                .occurrences(&option.node_path())
                .into_iter()
                .rfind(|node| node.file == file)
            {
                if let Some(property) = option.property {
                    if let Some(entry) = node.entry(Some(property)) {
                        // Remove ` prop=value`, including the leading space.
                        let mut start = entry.span.start;
                        while start > node.name_span.end && text.as_bytes()[start - 1] == b' ' {
                            start -= 1;
                        }
                        text = splice(&text, start..entry.span.end, "");
                    }
                } else {
                    text = remove_node(&text, node);
                }
            }
        }
    }
    Ok(FileEdit {
        file,
        before,
        after: text,
    })
}

fn set_presence_toggle(
    tree: &Tree,
    option: &OptionInfo,
    enabled: bool,
    file: usize,
) -> Result<String, EditError> {
    let path = option.node_path();
    let node = tree
        .occurrences(&path)
        .into_iter()
        .rfind(|node| node.file == file);
    let text = &tree.file(file).text;
    if let Some(node) = node {
        match node.entry(None) {
            Some(entry) if enabled => {
                // `flag false` becomes a bare `flag`.
                let mut start = entry.span.start;
                while start > node.name_span.end && text.as_bytes()[start - 1] == b' ' {
                    start -= 1;
                }
                Ok(splice(text, start..entry.span.end, ""))
            }
            Some(_) => Ok(remove_node(text, node)),
            None if enabled => Ok(text.clone()),
            None => {
                if option.parent.is_empty() {
                    Ok(splice(
                        text,
                        node.node_span.end..node.node_span.end,
                        " false",
                    ))
                } else {
                    Err(EditError {
                        message: format!(
                            "An included file sets {}. To turn it off, edit that file.",
                            option.label
                        ),
                    })
                }
            }
        }
    } else if enabled {
        insert_node(text, tree, file, &path, None, &Value::Bool(true))
    } else if option.parent.is_empty() {
        insert_node(text, tree, file, &path, None, &Value::Bool(false))
    } else {
        Err(EditError {
            message: format!(
                "An included file sets {}. To turn it off, edit that file.",
                option.label
            ),
        })
    }
}

fn set_section_toggle(
    tree: &Tree,
    option: &OptionInfo,
    enabled: bool,
    file: usize,
) -> Result<String, EditError> {
    let section_path: Vec<String> = option.parent.iter().map(|s| s.to_string()).collect();
    let mut text = tree.file(file).text.clone();
    let off_only = matches!(
        option.control,
        Control::Toggle {
            style: ToggleStyle::OffOnly,
            ..
        }
    );
    let wanted = if enabled { "on" } else { "off" };
    let unwanted = if enabled { "off" } else { "on" };

    // Remove the opposite flag in this file. Niri gives `on` priority over
    // `off` inside one section, but writing both is confusing.
    let opposite: Vec<&NodeRef> = tree
        .nodes
        .iter()
        .filter(|node| {
            node.file == file
                && node.name == unwanted
                && node.path.len() == section_path.len() + 1
                && node.path[..section_path.len()] == section_path[..]
        })
        .collect();
    for node in opposite.iter().rev() {
        text = remove_node(&text, node);
    }

    if off_only && enabled {
        return Ok(text);
    }
    let flag_path: Vec<String> = section_path
        .iter()
        .cloned()
        .chain(std::iter::once(wanted.to_string()))
        .collect();
    // Removal moves every later byte. Parse the new text before inserting.
    let fresh = if opposite.is_empty() {
        tree.clone()
    } else {
        match parse_text(&text) {
            Some(tree) => tree,
            None => return Ok(text),
        }
    };
    if fresh
        .nodes
        .iter()
        .any(|node| node.file == file && node.path == flag_path && !node.is_section())
    {
        return Ok(text);
    }
    insert_node(&text, &fresh, file, &flag_path, None, &Value::Bool(true))
}

fn set_scalar(
    tree: &Tree,
    option: &OptionInfo,
    value: &Value,
    file: usize,
) -> Result<String, EditError> {
    let path = option.node_path();
    let node = tree
        .occurrences(&path)
        .into_iter()
        .rfind(|node| node.file == file);
    let text = &tree.file(file).text;
    if let Some(node) = node {
        if let Some(entry) = node.entry(option.property) {
            return Ok(splice(text, entry.span.clone(), &value.literal()));
        }
        if let Some(property) = option.property {
            return Ok(insert_entry(text, node, Some(property), value));
        }
        return Ok(insert_entry(text, node, None, value));
    }
    insert_node(text, tree, file, &path, option.property, value)
}

fn insert_entry(text: &str, node: &NodeRef, property: Option<&str>, value: &Value) -> String {
    let add = match property {
        Some(name) => format!(" {name}={}", value.literal()),
        None => format!(" {}", value.literal()),
    };
    splice(text, node.node_span.end..node.node_span.end, &add)
}

fn insert_node(
    text: &str,
    tree: &Tree,
    file: usize,
    path: &[String],
    property: Option<&str>,
    value: &Value,
) -> Result<String, EditError> {
    // Find the deepest existing section in this file.
    let mut deepest: Option<&NodeRef> = None;
    for cut in (0..path.len()).rev() {
        let prefix = &path[..cut];
        if let Some(section) = tree.section_in(prefix, file) {
            deepest = Some(section);
            break;
        }
    }
    if let Some(section) = deepest {
        let depth = section.path.len();
        let parent_indent = line_indent(text, section.node_span.start);
        let child_indent = format!("{}    ", parent_indent);
        let tail = &path[depth..];
        let rendered = render_chain(tail, property, value, &child_indent);
        return Ok(insert_child(text, section, &rendered, &parent_indent));
    }

    let rendered = render_chain(path, property, value, "");
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(&rendered);
    out.push('\n');
    Ok(out)
}

/// Append a raw KDL node at the top level of a file. The caller supplies
/// indentation and newlines as needed; the node is separated by one blank line.
pub fn append_node(text: &str, raw: &str) -> String {
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(raw);
    if !raw.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Insert a raw node inside the last section with `parent` in this file.
/// Creates the section chain when needed.
pub fn insert_raw_node(
    text: &str,
    tree: &Tree,
    file: usize,
    parent: &[String],
    raw: &str,
) -> Result<String, EditError> {
    if parent.is_empty() {
        return Ok(append_node(text, raw));
    }
    if let Some(section) = tree.section_in(parent, file) {
        let parent_indent = line_indent(text, section.node_span.start);
        let child_indent = format!("{parent_indent}    ");
        let indented = indent_block(raw, &child_indent);
        return Ok(insert_child(text, section, &indented, &parent_indent));
    }
    let mut built = String::new();
    for (depth, name) in parent.iter().enumerate() {
        let indent = "    ".repeat(depth);
        built.push_str(&format!("{indent}{name} {{\n"));
    }
    let depth = parent.len();
    built.push_str(&indent_block(raw, &"    ".repeat(depth)));
    built.push('\n');
    for depth in (0..parent.len()).rev() {
        built.push_str(&"    ".repeat(depth));
        built.push_str("}\n");
    }
    Ok(append_node(text, built.trim_end()))
}

fn indent_block(raw: &str, indent: &str) -> String {
    raw.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_chain(path: &[String], property: Option<&str>, value: &Value, indent: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if path.len() == 1 {
        let name = &path[0];
        match (property, value) {
            (Some(prop), value) => format!("{indent}{name} {prop}={}", value.literal()),
            (None, Value::Bool(true)) => format!("{indent}{name}"),
            (None, value) => format!("{indent}{name} {}", value.literal()),
        }
    } else {
        let inner_indent = format!("{indent}    ");
        let inner = render_chain(&path[1..], property, value, &inner_indent);
        format!("{indent}{} {{\n{inner}\n{indent}}}", path[0])
    }
}

fn insert_child(text: &str, section: &NodeRef, rendered: &str, parent_indent: &str) -> String {
    let Some(children) = section.children_span.clone() else {
        let indent = line_indent(text, section.node_span.start);
        let block = format!(" {{\n{rendered}\n{indent}}}");
        return splice(text, section.node_span.end..section.node_span.end, &block);
    };

    let close = children.end;
    let line_start = text[..close].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_prefix = &text[line_start..close];
    if line_prefix.trim().is_empty() {
        // The closing brace sits on its own line.
        splice(text, line_start..line_start, &format!("{rendered}\n"))
    } else {
        let insert = format!("\n{rendered}\n{parent_indent}");
        splice(text, close..close, &insert)
    }
}

/// Insert a raw node inside a specific node.
pub fn insert_raw_in_node(text: &str, node: &NodeRef, raw: &str) -> String {
    let parent_indent = line_indent(text, node.node_span.start);
    let child_indent = format!("{parent_indent}    ");
    let indented = indent_block(raw, &child_indent);
    insert_child(text, node, &indented, &parent_indent)
}

/// Remove a node and the line or lines that it sits on.
pub fn remove_node(text: &str, node: &NodeRef) -> String {
    let line = &text[node.full_span.clone()];
    let before = &text[node.full_span.start..node.node_span.start];
    let after = &text[node.node_span.end..node.full_span.end];
    // Remove whole lines only when the node owns them. A trailing comment is
    // fine to remove with the node.
    let owns_lines = before.trim().is_empty()
        && (after.trim().is_empty()
            || after.trim_start().starts_with("//")
            || after.trim_start().starts_with("/*"));
    if !owns_lines || line.trim().is_empty() {
        return splice(text, node.node_span.clone(), "");
    }
    let mut start = node.full_span.start;
    let end = node.full_span.end;
    if end == text.len() && start > 0 && text.as_bytes()[start - 1] == b'\n' {
        start -= 1;
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&text[end..]);
    out
}

/// Replace a byte range.
pub fn splice(text: &str, span: Range<usize>, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..span.start]);
    out.push_str(replacement);
    out.push_str(&text[span.end..]);
    out
}

/// Add an include line for an override file when it is missing.
pub fn ensure_include(text: &str, file_name: &str) -> String {
    if text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("include") && line.contains(file_name)
    }) {
        return text.to_string();
    }
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n// niri uses these values instead of the values that desktop shells write.\n");
    out.push_str(&format!("include \"{file_name}\""));
    out.push('\n');
    out
}

fn line_indent(text: &str, at: usize) -> String {
    let start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    text[start..at]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Tree) {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            fs::write(dir.path().join(name), text).unwrap();
        }
        let tree = load(&dir.path().join("config.kdl")).unwrap();
        (dir, tree)
    }

    #[test]
    fn includes_merge_in_place() {
        let (_dir, tree) = tree_with(&[
            (
                "config.kdl",
                "layout {\n  gaps 8\n}\ninclude \"colors.kdl\"\nlayout {\n  gaps 12\n}\n",
            ),
            ("colors.kdl", "layout {\n  gaps 10\n}\n"),
        ]);
        let path = vec!["layout".to_string(), "gaps".to_string()];
        assert_eq!(tree.winner(&path).unwrap().scalar(None), Value::Int(12));
    }

    #[test]
    fn later_include_wins() {
        let (_dir, tree) = tree_with(&[
            (
                "config.kdl",
                "layout {\n  gaps 12\n}\ninclude \"colors.kdl\"\n",
            ),
            ("colors.kdl", "layout {\n  gaps 8\n}\n"),
        ]);
        let path = vec!["layout".to_string(), "gaps".to_string()];
        assert_eq!(tree.winner(&path).unwrap().scalar(None), Value::Int(8));
    }

    #[test]
    fn managed_shell_files_are_detected() {
        let (_dir, tree) = tree_with(&[
            ("config.kdl", "include \"noctalia.kdl\"\n"),
            (
                "noctalia.kdl",
                "// Managed by the Noctalia cursor plugin\ncursor { xcursor-size 20; }\n",
            ),
        ]);
        assert!(matches!(tree.files[1].managed, Managed::Shell(_)));
    }

    #[test]
    fn setting_a_value_keeps_comments_and_bytes() {
        let (_dir, tree) = tree_with(&[(
            "config.kdl",
            "// header\nlayout {\n    gaps 10   // keep me\n}\n",
        )]);
        let option = crate::schema::option("layout.gaps").unwrap();
        let edit = set_option(&tree, option, &Value::Int(16), 0).unwrap();
        assert_eq!(
            edit.after,
            "// header\nlayout {\n    gaps 16   // keep me\n}\n"
        );
        assert_eq!(edit.before, tree.files[0].text);
    }

    #[test]
    fn removed_line_leaves_no_blank_hole() {
        let (_dir, tree) = tree_with(&[(
            "config.kdl",
            "layout {\n    gaps 10\n    center-focused-column \"never\"\n}\n",
        )]);
        let option = crate::schema::option("layout.gaps").unwrap();
        let edit = reset_option(&tree, option, 0).unwrap();
        assert_eq!(
            edit.after,
            "layout {\n    center-focused-column \"never\"\n}\n"
        );
    }
}
