//! The session: one open config, its history, and every safe write.
//!
//! All writes follow the same path:
//! 1. Copy the include closure into a scratch directory.
//! 2. Apply the change there.
//! 3. Run `niri validate` on the scratch main file.
//! 4. Check that the real files did not change since they were loaded.
//! 5. Write atomically and record one history entry.
//!
//! A failed step leaves the real files untouched.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Serialize;
use tempfile::{NamedTempFile, TempDir};
use thiserror::Error;

use crate::doc::{self, FileEdit, LoadError, Managed, Tree};
use crate::history::{History, HistoryFile};
use crate::journal::{Journal, JournalError, PendingFile, PendingWrite, Recovery};
use crate::schema::{self, OptionInfo, ToggleStyle};
use crate::value::Value;

/// Options for opening a session.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Where history lives. Defaults to `$XDG_STATE_HOME/niri-settings`.
    pub state_dir: Option<PathBuf>,
    /// Run `niri validate` before a write. Tests turn this off.
    pub validate: bool,
    /// The file is the system-wide `/etc/niri/config.kdl`. It is read, never
    /// written (see `location::Location::System`).
    pub system: bool,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            state_dir: None,
            validate: true,
            system: false,
        }
    }
}

/// Where a value comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// niri's built-in default.
    Default,
    /// A file that the user owns.
    File(PathBuf),
    /// An included file that another program controls.
    Managed(PathBuf, String),
}

/// A value and its origin.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub value: Value,
    pub source: Source,
}

/// The outcome of a write.
#[derive(Debug, Clone)]
pub struct Applied {
    pub summary: String,
    pub files: usize,
    /// The history entry, when the write recorded one.
    pub entry: Option<u64>,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("the config file does not exist: {0}")]
    Missing(PathBuf),
    #[error("cannot load the config: {0}")]
    Load(#[from] LoadError),
    #[error("the config has no parent directory: {0}")]
    NoParent(PathBuf),
    #[error(
        "niri is not on PATH. Install niri before changing the config; the app will not write without validation."
    )]
    NiriMissing,
    #[error(transparent)]
    Journal(#[from] JournalError),
}

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("unknown option: {0}")]
    UnknownOption(String),
    #[error("nothing changes")]
    NoChange,
    #[error("{0} controls this value. Change it in that program instead.")]
    Managed(String),
    #[error(
        "the config is in the Nix store. Copy the Nix snippet into your system config instead."
    )]
    NixManaged,
    #[error(
        "{} is the system-wide config. Make a personal copy first; niri then uses the copy in your home folder.",
        .0.display()
    )]
    SystemConfig(PathBuf),
    #[error("{0} changed on disk. Reload the page and try again.")]
    Conflict(PathBuf),
    #[error("niri did not accept the change:\n{0}")]
    Validation(String),
    #[error("cannot write the config: {0}")]
    Io(String),
    #[error("{0}")]
    Edit(String),
}

/// One text change. `None` means the file does not exist (or must be removed).
#[derive(Debug, Clone)]
pub struct TextChange {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Mark the main file as the system-wide config.
fn mark_system(tree: &mut Tree) {
    let main = tree.main_index();
    if let Some(file) = tree.files.get_mut(main) {
        file.managed = Managed::System;
    }
}

/// An open config.
///
/// `Clone` and `Send` let the app write on a worker thread from a copy and
/// take the updated session back, so the window never waits on a write.
#[derive(Clone)]
pub struct Session {
    /// The path the user opened, before symlink resolution.
    pub config_path: PathBuf,
    /// The real file.
    pub real_path: PathBuf,
    pub tree: Tree,
    pub overrides_path: PathBuf,
    pub overrides_name: String,
    pub history: History,
    pub validate: bool,
    pub niri_available: bool,
    /// See `SessionOptions::system`.
    pub system: bool,
    /// A recovered journal from a previous run, for the front end to show.
    pub recovery: Option<Recovery>,
    journal: Journal,
    redo: Vec<crate::history::HistoryEntry>,
}

impl Session {
    pub fn open(path: &Path) -> Result<Self, SessionError> {
        Self::open_with(path, SessionOptions::default())
    }

    pub fn open_with(path: &Path, options: SessionOptions) -> Result<Self, SessionError> {
        let config_path = path.to_path_buf();
        if !config_path.exists() {
            return Err(SessionError::Missing(config_path));
        }
        let state_dir = options
            .state_dir
            .clone()
            .unwrap_or_else(crate::history::default_dir);
        let mut history = History::open(state_dir.clone());
        let journal = Journal::new(state_dir);
        // Recover before loading the tree, so the tree is the text that is
        // actually on disk after the replay.
        let recovery = journal.recover()?;
        if let Some(recovery) = &recovery
            && !recovery.committed
            && let Some(id) = recovery.history_id
        {
            // The change was rolled back, so its undo entry is not a real
            // change any more.
            history.discard(id);
        }
        let mut tree = doc::load(&config_path)?;
        if options.system {
            mark_system(&mut tree);
        }
        let real_path = tree.main.clone();
        let parent = real_path
            .parent()
            .ok_or_else(|| SessionError::NoParent(real_path.clone()))?
            .to_path_buf();
        let overrides_name = "niri-settings-overrides.kdl".to_string();
        let overrides_path = parent.join(&overrides_name);
        let niri_available = command_exists("niri");
        if options.validate && !niri_available {
            return Err(SessionError::NiriMissing);
        }
        Ok(Self {
            config_path,
            real_path,
            tree,
            overrides_path,
            overrides_name,
            history,
            validate: options.validate,
            niri_available,
            system: options.system,
            recovery,
            journal,
            redo: Vec::new(),
        })
    }

    pub fn reload(&mut self) -> Result<(), SessionError> {
        self.tree = doc::load(&self.real_path)?;
        if self.system {
            mark_system(&mut self.tree);
        }
        Ok(())
    }

    /// The error for a write to a main file that cannot be written.
    fn not_writable(&self) -> ApplyError {
        match self.main_managed() {
            Some(Managed::System) => ApplyError::SystemConfig(self.real_path.clone()),
            Some(Managed::Shell(name)) => ApplyError::Managed(name),
            _ => ApplyError::NixManaged,
        }
    }

    pub fn main_writable(&self) -> bool {
        self.tree
            .files
            .get(self.tree.main_index())
            .map(|f| f.managed.is_writable())
            .unwrap_or(false)
    }

    pub fn main_managed(&self) -> Option<Managed> {
        self.tree
            .files
            .get(self.tree.main_index())
            .map(|f| f.managed.clone())
    }

    /// Every file the app should watch: the loaded files and include targets,
    /// including optional targets that do not exist yet.
    pub fn watched_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self
            .tree
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        for include in &self.tree.includes {
            if !paths.contains(&include.path) {
                paths.push(include.path.clone());
            }
        }
        paths
    }

    /// Read an option. Unknown keys fall back to a raw node path.
    pub fn get(&self, key: &str) -> Option<Resolved> {
        if let Some(option) = schema::option(key) {
            return Some(self.get_option(option));
        }
        let path: Vec<String> = key.split('.').map(str::to_string).collect();
        let node = self.tree.winner(&path)?;
        let source = self.source_for_file(node.file);
        Some(Resolved {
            value: node.scalar(None),
            source,
        })
    }

    pub fn get_option(&self, option: &OptionInfo) -> Resolved {
        let value = self.tree.read(option);
        let file = match option.control {
            schema::Control::Toggle {
                style: ToggleStyle::Section,
                ..
            } => self.section_flag_file(option),
            _ => self.tree.winner(&option.node_path()).map(|node| node.file),
        };
        let source = match file {
            Some(file) => self.source_for_file(file),
            None => Source::Default,
        };
        Resolved { value, source }
    }

    fn section_flag_file(&self, option: &OptionInfo) -> Option<usize> {
        let section = option.parent.to_vec();
        self.tree
            .nodes
            .iter()
            .rev()
            .find(|node| {
                node.path.len() == section.len() + 1
                    && node.path[..section.len()] == section[..]
                    && (node.name == "on" || node.name == "off")
            })
            .map(|node| node.file)
    }

    fn source_for_file(&self, file: usize) -> Source {
        let source = &self.tree.files[file];
        match &source.managed {
            Managed::User => Source::File(source.path.clone()),
            Managed::Shell(name) => Source::Managed(source.path.clone(), name.clone()),
            Managed::Nix => Source::Managed(source.path.clone(), "Nix".to_string()),
            Managed::ReadOnly => Source::Managed(source.path.clone(), "read-only file".to_string()),
            Managed::System => {
                Source::Managed(source.path.clone(), "system-wide config".to_string())
            }
        }
    }

    pub fn is_set(&self, key: &str) -> bool {
        schema::option(key)
            .map(|option| self.tree.is_set(option))
            .unwrap_or(false)
    }

    /// Change one option.
    pub fn set(&mut self, key: &str, value: Value) -> Result<Applied, ApplyError> {
        let option =
            schema::option(key).ok_or_else(|| ApplyError::UnknownOption(key.to_string()))?;
        let value = coerce(option, value);
        let current = self.tree.read(option);
        if current.loose_eq(&value) {
            return Err(ApplyError::NoChange);
        }
        let summary = describe(option, &current, &value);
        self.apply_planned(&[Planned::Set(option, value)], summary)
    }

    /// Remove one option, so that niri's default applies again.
    pub fn reset(&mut self, key: &str) -> Result<Applied, ApplyError> {
        let option =
            schema::option(key).ok_or_else(|| ApplyError::UnknownOption(key.to_string()))?;
        if !self.tree.is_set(option) {
            return Err(ApplyError::NoChange);
        }
        let summary = format!("Reset {}", option.label.to_lowercase());
        self.apply_planned(&[Planned::Reset(option)], summary)
    }

    /// Change several options as one undo step. Used by presets and import.
    pub fn set_many(
        &mut self,
        changes: &[(&str, Value)],
        summary: &str,
    ) -> Result<Applied, ApplyError> {
        let mut planned = Vec::new();
        for (key, value) in changes {
            let option =
                schema::option(key).ok_or_else(|| ApplyError::UnknownOption(key.to_string()))?;
            planned.push(Planned::Set(option, coerce(option, value.clone())));
        }
        self.apply_planned(&planned, summary.to_string())
    }

    fn apply_planned(
        &mut self,
        planned: &[Planned],
        summary: String,
    ) -> Result<Applied, ApplyError> {
        if !self.main_writable() {
            return Err(self.not_writable());
        }

        let parent = self
            .real_path
            .parent()
            .ok_or_else(|| ApplyError::Io("config has no parent".to_string()))?;
        let needs_override = planned.iter().any(|change| {
            let option = change.option();
            matches!(self.target_for(&self.tree, option), Target::Override)
        });

        let mut scratch = Scratch::new(parent, &self.real_path)?;
        scratch.copy(&self.tree)?;
        if needs_override {
            scratch.ensure_override(&self.tree, &self.overrides_path, &self.overrides_name)?;
        }

        for change in planned {
            let option = change.option();
            let target = self.target_for(&scratch.tree, option);
            let file = match target {
                Target::File(index) => index,
                Target::Override => scratch
                    .tree
                    .file_index(&scratch.override_path())
                    .ok_or_else(|| {
                        ApplyError::Io("the override file is not included".to_string())
                    })?,
            };
            if !scratch.tree.files[file].managed.is_writable() {
                return Err(ApplyError::Managed(
                    scratch.tree.files[file]
                        .managed
                        .reason()
                        .unwrap_or_else(|| "another program".to_string()),
                ));
            }
            let edit = match change {
                Planned::Set(option, value) => doc::set_option(&scratch.tree, option, value, file),
                Planned::Reset(option) => doc::reset_option(&scratch.tree, option, file),
            }
            .map_err(|error| ApplyError::Edit(error.message))?;
            scratch.write_edit(&edit)?;
            scratch.tree = doc::load(&scratch.main_path)
                .map_err(|error| ApplyError::Edit(format!("{error}")))?;
        }

        self.validate_scratch(&scratch)?;
        let changes = scratch.changes(&self.tree)?;
        if changes.is_empty() {
            return Err(ApplyError::NoChange);
        }
        self.write_changes(&changes, &summary, true)
    }

    fn target_for(&self, tree: &Tree, option: &OptionInfo) -> Target {
        let main = tree.main_index();
        match option.control {
            schema::Control::Toggle {
                style: ToggleStyle::Section,
                ..
            } => {
                let section: Vec<String> = option.parent.iter().map(|s| s.to_string()).collect();
                match tree.section(&section) {
                    Some(section) if tree.files[section.file].managed.is_writable() => {
                        Target::File(section.file)
                    }
                    Some(_) => Target::Override,
                    None => Target::File(main),
                }
            }
            _ => match tree.winner(&option.node_path()) {
                Some(node) => {
                    if tree.files[node.file].managed.is_writable() {
                        Target::File(node.file)
                    } else {
                        Target::Override
                    }
                }
                None => {
                    // No value yet. Insert next to the last section that would
                    // hold it, so that a later include cannot override it.
                    let path = option.node_path();
                    for cut in (0..path.len()).rev() {
                        let prefix = &path[..cut];
                        if let Some(section) = tree.section(prefix) {
                            return if tree.files[section.file].managed.is_writable() {
                                Target::File(section.file)
                            } else {
                                Target::Override
                            };
                        }
                    }
                    Target::File(main)
                }
            },
        }
    }

    fn validate_scratch(&self, scratch: &Scratch) -> Result<(), ApplyError> {
        if !self.validate {
            return Ok(());
        }
        let (main, replacements) = scratch.prepare_validation()?;
        let output = std::process::Command::new("timeout")
            .arg("10")
            .arg("niri")
            .arg("validate")
            .arg("-c")
            .arg(&main)
            .output()
            .or_else(|_| {
                std::process::Command::new("niri")
                    .arg("validate")
                    .arg("-c")
                    .arg(&main)
                    .output()
            })
            .map_err(|error| ApplyError::Io(format!("cannot run niri: {error}")))?;
        if output.status.success() {
            return Ok(());
        }
        let raw = String::from_utf8_lossy(&output.stderr);
        let raw = if raw.trim().is_empty() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            raw.to_string()
        };
        let message = clean_validation(&raw, &replacements);
        Err(ApplyError::Validation(message))
    }

    fn write_changes(
        &mut self,
        changes: &[TextChange],
        summary: &str,
        record: bool,
    ) -> Result<Applied, ApplyError> {
        for change in changes {
            let current = fs::read_to_string(&change.path).ok();
            if current != change.before {
                return Err(ApplyError::Conflict(change.path.clone()));
            }
        }

        // Record before the renames. A change that cannot be recorded must
        // not be written, because it could not be undone.
        let entry = if record {
            let files: Vec<HistoryFile> = changes
                .iter()
                .filter_map(|change| {
                    change.after.as_ref().map(|after| HistoryFile {
                        path: change.path.clone(),
                        before: change.before.clone(),
                        after: after.clone(),
                    })
                })
                .collect();
            let entry = self
                .history
                .record(summary.to_string(), files)
                .map_err(|error| ApplyError::Io(format!("cannot save history: {error}")))?;
            Some(entry)
        } else {
            None
        };

        // The journal holds both sides of every file. It is written after
        // history (so an undo record exists) and before the first rename.
        let pending = PendingWrite {
            summary: summary.to_string(),
            history_id: entry.as_ref().map(|entry| entry.id),
            committed: false,
            files: changes
                .iter()
                .map(|change| PendingFile {
                    path: change.path.clone(),
                    before: change.before.clone(),
                    after: change.after.clone(),
                })
                .collect(),
        };
        if let Err(error) = self.journal.write(&pending) {
            if let Some(entry) = &entry {
                self.history.discard(entry.id);
            }
            return Err(ApplyError::Io(format!(
                "cannot save the recovery journal: {error}"
            )));
        }

        let order = write_order(changes, &self.tree, &self.overrides_path);
        if let Err(failure) = write_ordered(changes, &order) {
            // Discard the entry only when every file is back at its old text.
            // A failed rollback leaves a half-applied change, and its history
            // entry is the only copy of the old text that can restore it.
            if failure.rolled_back {
                let _ = self.journal.clear();
                if let Some(entry) = &entry {
                    self.history.discard(entry.id);
                }
            }
            return Err(failure.error);
        }

        // A committed journal is rolled forward on the next start. If this
        // marker cannot be written, the old journal still says "pending", so
        // the next start would undo a change that is already on disk. Roll
        // back now instead, and leave the journal for recovery if that fails.
        let mut committed = pending;
        committed.committed = true;
        if let Err(error) = self.journal.write(&committed) {
            let all: Vec<usize> = (0..changes.len()).collect();
            match rollback(changes, &all) {
                Ok(()) => {
                    let _ = self.journal.clear();
                    if let Some(entry) = &entry {
                        self.history.discard(entry.id);
                    }
                    return Err(ApplyError::Io(format!("cannot finish the change: {error}")));
                }
                Err(rollback) => {
                    return Err(ApplyError::Io(format!(
                        "cannot finish the change: {error}; rollback failed: {rollback}"
                    )));
                }
            }
        }
        let _ = self.journal.clear();

        self.redo.clear();
        self.reload()
            .map_err(|error| ApplyError::Io(format!("{error}")))?;
        Ok(Applied {
            summary: summary.to_string(),
            files: changes.len(),
            entry: entry.map(|entry| entry.id),
        })
    }

    /// Undo the last change.
    pub fn undo(&mut self) -> Result<Applied, ApplyError> {
        let entry = self.history.pop().ok_or(ApplyError::NoChange)?;
        let changes: Vec<TextChange> = entry
            .files
            .iter()
            .map(|file| TextChange {
                path: file.path.clone(),
                before: Some(file.after.clone()),
                after: file.before.clone(),
            })
            .collect();
        match self.apply_text_changes(changes, false) {
            Ok(mut applied) => {
                // The undo is on disk now; only then remove its record.
                self.history.forget(entry.id);
                applied.summary = format!("Undo: {}", entry.summary);
                self.redo.push(entry);
                Ok(applied)
            }
            Err(error) => {
                self.history.push_back(entry);
                Err(error)
            }
        }
    }

    /// Redo the last undone change.
    pub fn redo(&mut self) -> Result<Applied, ApplyError> {
        let entry = self.redo.pop().ok_or(ApplyError::NoChange)?;
        let changes: Vec<TextChange> = entry
            .files
            .iter()
            .map(|file| TextChange {
                path: file.path.clone(),
                before: file.before.clone(),
                after: Some(file.after.clone()),
            })
            .collect();
        let summary = format!("Redo: {}", entry.summary);
        match self.apply_text_changes(changes, true) {
            Ok(applied) => {
                let mut applied = applied;
                applied.summary = summary;
                Ok(applied)
            }
            Err(error) => {
                self.redo.push(entry);
                Err(error)
            }
        }
    }

    /// Restore a past state as a new change.
    pub fn restore(&mut self, id: u64) -> Result<Applied, ApplyError> {
        let entry = self
            .history
            .entries()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or(ApplyError::NoChange)?;
        let changes: Vec<TextChange> = entry
            .files
            .iter()
            .map(|file| {
                let before = self
                    .tree
                    .files
                    .iter()
                    .find(|source| source.path == file.path)
                    .map(|source| source.text.clone())
                    .or_else(|| fs::read_to_string(&file.path).ok());
                TextChange {
                    path: file.path.clone(),
                    before,
                    after: Some(file.after.clone()),
                }
            })
            .collect();
        let summary = format!("Restore: {}", entry.summary);
        let mut applied = self.apply_text_changes(changes, true)?;
        applied.summary = summary;
        Ok(applied)
    }

    fn apply_text_changes(
        &mut self,
        changes: Vec<TextChange>,
        record: bool,
    ) -> Result<Applied, ApplyError> {
        // Every write ends here. The system config is never written.
        if self.system {
            return Err(ApplyError::SystemConfig(self.real_path.clone()));
        }
        let parent = self
            .real_path
            .parent()
            .ok_or_else(|| ApplyError::Io("config has no parent".to_string()))?;
        let mut scratch = Scratch::new(parent, &self.real_path)?;
        scratch.copy(&self.tree)?;
        for change in &changes {
            scratch.write_change(change)?;
        }
        self.validate_scratch(&scratch)?;
        self.write_changes(&changes, "Change", record)
    }

    /// Apply a set of shortcuts as one change. The values are written to the
    /// override file, so they win over every include and keep the main file
    /// untouched.
    pub fn apply_binds(
        &mut self,
        binds: &[(&str, &str)],
        summary: &str,
    ) -> Result<Applied, ApplyError> {
        if !self.main_writable() {
            return Err(self.not_writable());
        }
        let changes = self.with_override_edit(|text| {
            let mut current = text.to_string();
            for (key, action) in binds {
                let raw = format!("{key} {{ {action}; }}");
                if let Some(tree) = doc::parse_text(&current) {
                    let path = vec!["binds".to_string(), (*key).to_string()];
                    if let Some(node) = tree.winner(&path).cloned() {
                        current = doc::splice(&current, node.node_span.clone(), &raw);
                        continue;
                    }
                    let parent = vec!["binds".to_string()];
                    current =
                        doc::insert_raw_node(&current, &tree, 0, &parent, &raw).unwrap_or(current);
                } else {
                    current = doc::append_node(&current, &format!("binds {{ {raw} }}"));
                }
            }
            current
        })?;
        let mut applied = self.apply_text_changes(changes, true)?;
        applied.summary = summary.to_string();
        Ok(applied)
    }

    /// Apply an import as one change. Values, binds, startup programs and
    /// window rules go to the override file, so one Undo removes the import.
    pub fn apply_import(
        &mut self,
        result: &crate::import::ImportResult,
    ) -> Result<Applied, ApplyError> {
        if !self.main_writable() {
            return Err(self.not_writable());
        }
        if result.is_empty() {
            return Err(ApplyError::NoChange);
        }
        let override_before = fs::read_to_string(&self.overrides_path).ok();
        let mut override_after = override_before.clone().unwrap_or_default();
        for (key, value) in &result.changes {
            let option =
                crate::schema::option(key).ok_or_else(|| ApplyError::UnknownOption(key.clone()))?;
            let tree = doc::parse_text(&override_after)
                .ok_or_else(|| ApplyError::Edit("cannot parse the override file".to_string()))?;
            let edit = doc::set_option(&tree, option, value, 0)
                .map_err(|error| ApplyError::Edit(error.message))?;
            override_after = edit.after;
        }
        for bind in &result.binds {
            let raw = format!("{} {{ {}; }}", bind.key, bind.action);
            let tree = doc::parse_text(&override_after)
                .ok_or_else(|| ApplyError::Edit("cannot parse the override file".to_string()))?;
            let path = vec!["binds".to_string(), bind.key.clone()];
            if let Some(node) = tree.winner(&path).cloned() {
                override_after = doc::splice(&override_after, node.node_span.clone(), &raw);
            } else {
                let parent = vec!["binds".to_string()];
                override_after = doc::insert_raw_node(&override_after, &tree, 0, &parent, &raw)
                    .map_err(|error| ApplyError::Edit(error.message))?;
            }
        }
        for program in &result.startup {
            let raw = format!(
                "spawn-at-startup {}",
                crate::value::Value::from(program.as_str()).literal()
            );
            override_after = doc::append_node(&override_after, &raw);
        }
        for rule in &result.window_rules {
            let raw = format!("window-rule {{\n{}\n}}", indent(rule, 1));
            override_after = doc::append_node(&override_after, &raw);
        }

        let main_before = fs::read_to_string(&self.real_path).ok();
        let main_text = main_before.clone().unwrap_or_default();
        let main_after = doc::ensure_include(&main_text, &self.overrides_name);
        let mut changes = Vec::new();
        if Some(&main_after) != main_before.as_ref() {
            changes.push(TextChange {
                path: self.real_path.clone(),
                before: main_before,
                after: Some(main_after),
            });
        }
        changes.push(TextChange {
            path: self.overrides_path.clone(),
            before: override_before.clone(),
            after: Some(override_after.clone()),
        });
        let mut applied = self.apply_text_changes(changes, true)?;
        applied.summary = format!(
            "Imported {} shortcuts and {} settings",
            result.binds.len(),
            result.changes.len()
        );
        Ok(applied)
    }

    /// Add a shortcut. The bind goes in the last `binds` section, so it wins
    /// over earlier definitions.
    pub fn add_bind(&mut self, key: &str, action: &str) -> Result<Applied, ApplyError> {
        let key = key.trim();
        if key.is_empty() || action.trim().is_empty() {
            return Err(ApplyError::Edit(
                "a shortcut needs keys and an action".to_string(),
            ));
        }
        let raw = format!("{key} {{ {action}; }}");
        let parent = vec!["binds".to_string()];
        let tree = &self.tree;
        let target = tree
            .section(&parent)
            .map(|node| node.file)
            .unwrap_or(tree.main_index());
        if tree.files[target].managed.is_writable() {
            let after = doc::insert_raw_node(&tree.files[target].text, tree, target, &parent, &raw)
                .map_err(|error| ApplyError::Edit(error.message))?;
            let change = TextChange {
                path: tree.files[target].path.clone(),
                before: Some(tree.files[target].text.clone()),
                after: Some(after),
            };
            let mut applied = self.apply_text_changes(vec![change], true)?;
            applied.summary = format!("Added shortcut {key}");
            return Ok(applied);
        }
        let changes = self.with_override_edit(|text| {
            let tree = doc::parse_text(text);
            match tree {
                Some(tree) => doc::insert_raw_node(text, &tree, 0, &parent, &raw)
                    .unwrap_or_else(|_| text.to_string()),
                None => doc::append_node(text, &format!("binds {{ {raw} }}")),
            }
        })?;
        let mut applied = self.apply_text_changes(changes, true)?;
        applied.summary = format!("Added shortcut {key}");
        Ok(applied)
    }

    /// Remove a shortcut that this user owns.
    pub fn remove_bind(&mut self, key: &str) -> Result<Applied, ApplyError> {
        let path = vec!["binds".to_string(), key.to_string()];
        let node = self
            .tree
            .winner(&path)
            .ok_or_else(|| ApplyError::Edit(format!("{key} is not bound")))?;
        if !self.tree.files[node.file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[node.file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let after = doc::remove_node(&self.tree.files[node.file].text, node);
        let change = TextChange {
            path: self.tree.files[node.file].path.clone(),
            before: Some(self.tree.files[node.file].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = format!("Removed shortcut {key}");
        Ok(applied)
    }

    /// Give an existing shortcut new keys.
    pub fn rename_bind(&mut self, old: &str, new: &str) -> Result<Applied, ApplyError> {
        let path = vec!["binds".to_string(), old.to_string()];
        let node = self
            .tree
            .winner(&path)
            .ok_or_else(|| ApplyError::Edit(format!("{old} is not bound")))?;
        if !self.tree.files[node.file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[node.file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let after = doc::splice(
            &self.tree.files[node.file].text,
            node.name_span.clone(),
            new,
        );
        let change = TextChange {
            path: self.tree.files[node.file].path.clone(),
            before: Some(self.tree.files[node.file].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = format!("Changed {old} to {new}");
        Ok(applied)
    }

    /// Add a window rule. The caller supplies the body of `window-rule { ... }`.
    pub fn add_window_rule(&mut self, body: &str) -> Result<Applied, ApplyError> {
        let raw = format!("window-rule {{\n{}\n}}", indent(body, 1));
        self.append_raw_main(&raw, "Added an app rule")
    }

    /// Replace the body of the window rule at `index`.
    pub fn edit_window_rule(&mut self, index: usize, body: &str) -> Result<Applied, ApplyError> {
        let node = self
            .rule_node(index)
            .ok_or_else(|| ApplyError::Edit("that rule no longer exists".to_string()))?;
        if !self.tree.files[node.file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[node.file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let raw = format!("window-rule {{\n{}\n}}", indent(body, 1));
        let after = doc::splice(
            &self.tree.files[node.file].text,
            node.node_span.clone(),
            &raw,
        );
        let change = TextChange {
            path: self.tree.files[node.file].path.clone(),
            before: Some(self.tree.files[node.file].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = "Changed an app rule".to_string();
        Ok(applied)
    }

    /// Remove the window rule at `index`.
    pub fn remove_window_rule(&mut self, index: usize) -> Result<Applied, ApplyError> {
        let node = self
            .rule_node(index)
            .ok_or_else(|| ApplyError::Edit("that rule no longer exists".to_string()))?;
        if !self.tree.files[node.file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[node.file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let after = doc::remove_node(&self.tree.files[node.file].text, node);
        let change = TextChange {
            path: self.tree.files[node.file].path.clone(),
            before: Some(self.tree.files[node.file].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = "Removed an app rule".to_string();
        Ok(applied)
    }

    fn rule_node(&self, index: usize) -> Option<&crate::doc::NodeRef> {
        self.tree.top_level("window-rule").into_iter().nth(index)
    }

    /// Add a program that starts with niri.
    pub fn add_startup(&mut self, command: &str) -> Result<Applied, ApplyError> {
        let raw = format!(
            "spawn-at-startup {}",
            crate::value::Value::from(command).literal()
        );
        self.append_raw_main(&raw, "Added a startup program")
    }

    /// Remove the startup program at `index`.
    pub fn remove_startup(&mut self, index: usize) -> Result<Applied, ApplyError> {
        let node = self
            .tree
            .top_level("spawn-at-startup")
            .into_iter()
            .nth(index)
            .ok_or_else(|| ApplyError::Edit("that startup entry no longer exists".to_string()))?;
        if !self.tree.files[node.file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[node.file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let after = doc::remove_node(&self.tree.files[node.file].text, node);
        let change = TextChange {
            path: self.tree.files[node.file].path.clone(),
            before: Some(self.tree.files[node.file].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = "Removed a startup program".to_string();
        Ok(applied)
    }

    /// Set an environment variable for apps started by niri.
    pub fn set_environment(&mut self, name: &str, value: &str) -> Result<Applied, ApplyError> {
        let parent = vec!["environment".to_string()];
        let raw = format!("{name} {}", crate::value::Value::from(value).literal());
        let tree = &self.tree;
        let target = tree
            .section(&parent)
            .map(|node| node.file)
            .unwrap_or(tree.main_index());
        if tree.files[target].managed.is_writable() {
            let after = doc::insert_raw_node(&tree.files[target].text, tree, target, &parent, &raw)
                .map_err(|error| ApplyError::Edit(error.message))?;
            let change = TextChange {
                path: tree.files[target].path.clone(),
                before: Some(tree.files[target].text.clone()),
                after: Some(after),
            };
            let mut applied = self.apply_text_changes(vec![change], true)?;
            applied.summary = format!("Set {name}");
            return Ok(applied);
        }
        Err(ApplyError::Managed(
            tree.files[target]
                .managed
                .reason()
                .unwrap_or_else(|| "another program".to_string()),
        ))
    }

    /// Remove an environment variable.
    pub fn remove_environment(&mut self, name: &str) -> Result<Applied, ApplyError> {
        let path = vec!["environment".to_string(), name.to_string()];
        let node = self
            .tree
            .winner(&path)
            .ok_or_else(|| ApplyError::Edit(format!("{name} is not set")))?;
        if !self.tree.files[node.file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[node.file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let after = doc::remove_node(&self.tree.files[node.file].text, node);
        let change = TextChange {
            path: self.tree.files[node.file].path.clone(),
            before: Some(self.tree.files[node.file].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = format!("Removed {name}");
        Ok(applied)
    }

    fn append_raw_main(&mut self, raw: &str, summary: &str) -> Result<Applied, ApplyError> {
        let main = self.tree.main_index();
        if !self.tree.files[main].managed.is_writable() {
            return Err(ApplyError::NixManaged);
        }
        let after = doc::append_node(&self.tree.files[main].text, raw);
        let change = TextChange {
            path: self.tree.files[main].path.clone(),
            before: Some(self.tree.files[main].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = summary.to_string();
        Ok(applied)
    }

    fn with_override_edit(
        &self,
        edit: impl FnOnce(&str) -> String,
    ) -> Result<Vec<TextChange>, ApplyError> {
        let main_before = fs::read_to_string(&self.real_path).ok();
        let main_text = main_before.clone().unwrap_or_default();
        let override_before = fs::read_to_string(&self.overrides_path).ok();
        let override_text = override_before.clone().unwrap_or_default();
        let override_after = edit(&override_text);
        let main_after = doc::ensure_include(&main_text, &self.overrides_name);
        let mut changes = Vec::new();
        if Some(&main_after) != main_before.as_ref() {
            changes.push(TextChange {
                path: self.real_path.clone(),
                before: main_before,
                after: Some(main_after),
            });
        }
        if Some(&override_after) != override_before.as_ref() {
            changes.push(TextChange {
                path: self.overrides_path.clone(),
                before: override_before,
                after: Some(override_after),
            });
        }
        Ok(changes)
    }

    /// Give an existing shortcut new keys and a new action.
    pub fn replace_bind(
        &mut self,
        old: &str,
        new: &str,
        action: &str,
    ) -> Result<Applied, ApplyError> {
        let path = vec!["binds".to_string(), old.to_string()];
        let node = self
            .tree
            .winner(&path)
            .ok_or_else(|| ApplyError::Edit(format!("{old} is not bound")))?;
        if !self.tree.files[node.file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[node.file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let raw = format!("{new} {{ {action}; }}");
        let after = doc::splice(
            &self.tree.files[node.file].text,
            node.node_span.clone(),
            &raw,
        );
        let change = TextChange {
            path: self.tree.files[node.file].path.clone(),
            before: Some(self.tree.files[node.file].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = format!("Changed {old} to {new}");
        Ok(applied)
    }

    /// Add a named workspace.
    pub fn add_workspace(&mut self, name: &str, output: &str) -> Result<Applied, ApplyError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApplyError::Edit("a workspace needs a name".to_string()));
        }
        let raw = if output.trim().is_empty() {
            format!("workspace {}", crate::value::Value::from(name).literal())
        } else {
            format!(
                "workspace {} {{\n    open-on-output {}\n}}",
                crate::value::Value::from(name).literal(),
                crate::value::Value::from(output.trim()).literal()
            )
        };
        self.append_raw_main(&raw, &format!("Added workspace {name}"))
    }

    /// Remove a named workspace.
    pub fn remove_workspace(&mut self, name: &str) -> Result<Applied, ApplyError> {
        let node = self
            .tree
            .top_level("workspace")
            .into_iter()
            .find(|node| {
                node.entry(None)
                    .and_then(|entry| entry.value.as_str().map(|s| s == name))
                    .unwrap_or(false)
            })
            .ok_or_else(|| ApplyError::Edit(format!("{name} is not a workspace")))?;
        if !self.tree.files[node.file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[node.file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let after = doc::remove_node(&self.tree.files[node.file].text, node);
        let change = TextChange {
            path: self.tree.files[node.file].path.clone(),
            before: Some(self.tree.files[node.file].text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = format!("Removed workspace {name}");
        Ok(applied)
    }

    /// Change one field of one output block in the config.
    ///
    /// This is used after the live display change was kept. The file gets the
    /// value that niri already applied, so a reload does not change the screen.
    pub fn set_output(
        &mut self,
        name: &str,
        field: &str,
        value: Value,
    ) -> Result<Applied, ApplyError> {
        let main = self.tree.main_index();
        if !self.tree.files[main].managed.is_writable() {
            return Err(ApplyError::NixManaged);
        }
        let output = self
            .tree
            .top_level("output")
            .into_iter()
            .find(|node| {
                node.entry(None)
                    .and_then(|entry| entry.value.as_str().map(|s| s == name))
                    .unwrap_or(false)
            })
            .cloned();
        let file = output.as_ref().map(|node| node.file).unwrap_or(main);
        if !self.tree.files[file].managed.is_writable() {
            return Err(ApplyError::Managed(
                self.tree.files[file]
                    .managed
                    .reason()
                    .unwrap_or_else(|| "another program".to_string()),
            ));
        }
        let text = &self.tree.files[file].text;
        let after = if let Some(node) = &output {
            let child = self
                .tree
                .children_of(node)
                .into_iter()
                .find(|child| child.name == field)
                .cloned();
            if field == "off" {
                let disabled = value.as_bool().unwrap_or(false);
                match (disabled, child) {
                    (true, None) => doc::insert_raw_in_node(text, node, "off"),
                    (false, Some(child)) => doc::remove_node(text, &child),
                    _ => return Err(ApplyError::NoChange),
                }
            } else if field == "position" {
                let (x, y) = parse_position(&value);
                let raw = format!("position x={x} y={y}");
                match child {
                    Some(child) => doc::splice(text, child.node_span.clone(), &raw),
                    None => doc::insert_raw_in_node(text, node, &raw),
                }
            } else if let Some(child) = child {
                let entry = child
                    .entry(None)
                    .ok_or_else(|| ApplyError::Edit(format!("{field} has no value")))?;
                doc::splice(text, entry.span.clone(), &value.literal())
            } else {
                let raw = format!("{field} {}", value.literal());
                doc::insert_raw_in_node(text, node, &raw)
            }
        } else {
            let raw = format!(
                "output {}\n{{\n    {field} {}\n}}",
                crate::value::Value::from(name).literal(),
                value.literal()
            );
            doc::append_node(text, &raw)
        };
        if after == *text {
            return Err(ApplyError::NoChange);
        }
        let change = TextChange {
            path: self.tree.files[file].path.clone(),
            before: Some(text.clone()),
            after: Some(after),
        };
        let mut applied = self.apply_text_changes(vec![change], true)?;
        applied.summary = format!("Updated display {name}");
        Ok(applied)
    }

    /// The changed values as a niri-flake `programs.niri.settings` block.
    ///
    /// Nix users cannot let the app write the generated file. This gives them
    /// the same values in the form that their system config needs.
    pub fn nix_snippet(&self) -> String {
        let mut lines = vec![
            "# Generated by niri-settings. Merge this into your Nix configuration.".to_string(),
            "programs.niri.settings = {".to_string(),
        ];
        for option in crate::schema::OPTIONS {
            if !self.tree.is_set(option) {
                continue;
            }
            let value = self.tree.read(option);
            let mut path = Vec::new();
            for segment in option.key.split('.') {
                let safe = segment
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                    && !segment
                        .chars()
                        .next()
                        .map(|ch| ch.is_ascii_digit())
                        .unwrap_or(false);
                if safe {
                    path.push(segment.to_string());
                } else {
                    path.push(format!("\"{segment}\""));
                }
            }
            lines.push(format!("  {} = {};", path.join("."), nix_value(&value)));
        }
        lines.push("};".to_string());
        lines.join("\n")
    }

    pub fn history(&self) -> &[crate::history::HistoryEntry] {
        self.history.entries()
    }

    pub fn can_undo(&self) -> bool {
        !self.history.entries().is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// All files that the app can read, for the config page.
    pub fn files(&self) -> Vec<PathBuf> {
        self.tree
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect()
    }

    /// Save raw text for one file, after validation. Used by the config editor.
    pub fn write_raw(
        &mut self,
        path: &Path,
        text: &str,
        summary: &str,
    ) -> Result<Applied, ApplyError> {
        let before = fs::read_to_string(path).ok();
        let change = TextChange {
            path: path.to_path_buf(),
            before,
            after: Some(text.to_string()),
        };
        self.apply_text_changes(vec![change], true)
            .map(|mut applied| {
                applied.summary = summary.to_string();
                applied
            })
    }
}

#[derive(Debug, Clone, Copy)]
enum Target {
    File(usize),
    Override,
}

enum Planned {
    Set(&'static OptionInfo, Value),
    Reset(&'static OptionInfo),
}

impl Planned {
    fn option(&self) -> &'static OptionInfo {
        match self {
            Self::Set(option, _) | Self::Reset(option) => option,
        }
    }
}

fn coerce(option: &OptionInfo, value: Value) -> Value {
    match option.control {
        schema::Control::Toggle { .. } => Value::Bool(value.as_bool().unwrap_or(false)),
        schema::Control::Number { integer: true, .. } => {
            Value::Int(value.as_f64().unwrap_or_default().round() as i64)
        }
        schema::Control::Number { integer: false, .. } => {
            Value::Float(value.as_f64().unwrap_or_default())
        }
        schema::Control::Text { .. }
        | schema::Control::Color { .. }
        | schema::Control::Choice { .. } => match value {
            Value::String(text) => Value::String(text),
            other => Value::String(other.to_string()),
        },
    }
}

fn describe(option: &OptionInfo, before: &Value, after: &Value) -> String {
    match &option.control {
        schema::Control::Toggle { .. } => {
            let on = after.as_bool().unwrap_or(false);
            if on {
                format!("{} on", option.label)
            } else {
                format!("{} off", option.label)
            }
        }
        schema::Control::Number { unit, .. } => format!(
            "{} {} → {}{}",
            option.label,
            before.as_f64().unwrap_or_default(),
            after.as_f64().unwrap_or_default(),
            unit.suffix()
        ),
        schema::Control::Color { .. } => format!("{} color changed", option.label),
        schema::Control::Choice { .. } => format!(
            "{} set to {}",
            option.label,
            after.as_str().unwrap_or_default()
        ),
        schema::Control::Text { .. } => format!("{} changed", option.label),
    }
}

/// A scratch copy of the include closure.
struct Scratch {
    _dir: TempDir,
    _validate: TempDir,
    root: PathBuf,
    main_path: PathBuf,
    /// real path → scratch path
    map: BTreeMap<PathBuf, PathBuf>,
    /// real path → the text that the edit was based on
    bases: BTreeMap<PathBuf, Option<String>>,
    tree: Tree,
    parent: PathBuf,
}

impl Scratch {
    fn new(parent: &Path, real_main: &Path) -> Result<Self, ApplyError> {
        let dir = tempfile::tempdir().map_err(|error| ApplyError::Io(error.to_string()))?;
        let validate = tempfile::tempdir().map_err(|error| ApplyError::Io(error.to_string()))?;
        let root = dir.path().to_path_buf();
        let main_name = real_main
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.kdl");
        let main_path = root.join(main_name);
        let tree = doc::load(real_main).map_err(|error| ApplyError::Io(error.to_string()))?;
        Ok(Self {
            _dir: dir,
            _validate: validate,
            root,
            main_path,
            map: BTreeMap::new(),
            bases: BTreeMap::new(),
            tree,
            parent: parent.to_path_buf(),
        })
    }

    fn copy(&mut self, tree: &Tree) -> Result<(), ApplyError> {
        for (index, file) in tree.files.iter().enumerate() {
            let target = if let Ok(relative) = file.path.strip_prefix(&self.parent) {
                self.root.join(relative)
            } else {
                let name = file
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("config.kdl");
                self.root
                    .join("_outside")
                    .join(index.to_string())
                    .join(name)
            };
            if let Some(dir) = target.parent() {
                fs::create_dir_all(dir).map_err(|error| ApplyError::Io(error.to_string()))?;
            }
            fs::write(&target, &file.text).map_err(|error| ApplyError::Io(error.to_string()))?;
            self.map.insert(file.path.clone(), target);
            self.bases
                .insert(file.path.clone(), Some(file.text.clone()));
        }
        Ok(())
    }

    fn override_path(&self) -> PathBuf {
        self.root.join("niri-settings-overrides.kdl")
    }

    fn ensure_override(
        &mut self,
        tree: &Tree,
        real_override: &Path,
        name: &str,
    ) -> Result<(), ApplyError> {
        let target = self.root.join(name);
        let existing = fs::read_to_string(real_override).unwrap_or_default();
        let existing_before = fs::read_to_string(real_override).ok();
        fs::write(&target, existing).map_err(|error| ApplyError::Io(error.to_string()))?;
        self.map.insert(real_override.to_path_buf(), target.clone());
        self.bases
            .insert(real_override.to_path_buf(), existing_before);

        // Add the include to the scratch main file when it is absent.
        let main = self
            .map
            .get(&tree.main)
            .cloned()
            .unwrap_or_else(|| self.main_path.clone());
        let text = fs::read_to_string(&main).map_err(|error| ApplyError::Io(error.to_string()))?;
        let included = tree.includes.iter().any(|include| {
            include
                .path
                .canonicalize()
                .map(|p| p == real_override.canonicalize().unwrap_or_default())
                .unwrap_or(false)
        });
        if !included {
            let updated = doc::ensure_include(&text, name);
            if updated != text {
                fs::write(&main, updated).map_err(|error| ApplyError::Io(error.to_string()))?;
            }
        }
        self.tree = doc::load(&self.main_path)
            .map_err(|error| ApplyError::Io(format!("scratch config: {error}")))?;
        Ok(())
    }

    fn write_edit(&mut self, edit: &FileEdit) -> Result<(), ApplyError> {
        let file_path = &self.tree.files[edit.file].path;
        let path = if let Ok(relative) = file_path.strip_prefix(&self.root) {
            self.root.join(relative)
        } else if let Some(scratch) = self.map.get(file_path) {
            scratch.clone()
        } else if let Ok(relative) = file_path.strip_prefix(&self.parent) {
            self.root.join(relative)
        } else {
            return Err(ApplyError::Io(
                "file is outside the config folder".to_string(),
            ));
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|error| ApplyError::Io(error.to_string()))?;
        }
        fs::write(&path, &edit.after).map_err(|error| ApplyError::Io(error.to_string()))?;
        Ok(())
    }

    fn write_change(&mut self, change: &TextChange) -> Result<(), ApplyError> {
        let path = self
            .map
            .get(&change.path)
            .cloned()
            .or_else(|| {
                change
                    .path
                    .strip_prefix(&self.parent)
                    .ok()
                    .map(|relative| self.root.join(relative))
            })
            .ok_or_else(|| ApplyError::Io("file is outside the config folder".to_string()))?;
        match &change.after {
            Some(after) => {
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir).map_err(|error| ApplyError::Io(error.to_string()))?;
                }
                fs::write(&path, after).map_err(|error| ApplyError::Io(error.to_string()))?;
            }
            None => {
                let _ = fs::remove_file(&path);
            }
        }
        Ok(())
    }

    /// Build a validation mirror where every include points at a copy. This
    /// makes changes to includes outside the config folder testable.
    fn prepare_validation(&self) -> Result<(PathBuf, Vec<(PathBuf, PathBuf)>), ApplyError> {
        let validate_root = self._validate.path();
        let mut path_map: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
        let mut replacements = Vec::new();
        for (index, (real, scratch)) in self.map.iter().enumerate() {
            let name = scratch
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("config.kdl");
            let target = validate_root.join(format!("{index}-{name}"));
            let text =
                fs::read_to_string(scratch).map_err(|error| ApplyError::Io(error.to_string()))?;
            fs::write(&target, text).map_err(|error| ApplyError::Io(error.to_string()))?;
            path_map.insert(real.clone(), target.clone());
            path_map.insert(scratch.clone(), target.clone());
            replacements.push((target, real.clone()));
        }
        let main = path_map
            .get(&self.main_path)
            .cloned()
            .ok_or_else(|| ApplyError::Io("the main config was not copied".to_string()))?;

        // Reload the tree from the edited scratch main when possible. A broken
        // text cannot be parsed; niri validate will report that safely.
        let tree = doc::load(&self.main_path).unwrap_or_else(|_| self.tree.clone());
        for (index, file) in tree.files.iter().enumerate() {
            let Some(target) = path_map.get(&file.path) else {
                continue;
            };
            let mut text =
                fs::read_to_string(target).map_err(|error| ApplyError::Io(error.to_string()))?;
            let mut includes: Vec<_> = tree
                .includes
                .iter()
                .filter(|include| include.file == index)
                .collect();
            includes.sort_by_key(|include| std::cmp::Reverse(include.path_span.start));
            for include in includes {
                if let Some(new_path) = path_map.get(&include.path) {
                    let literal =
                        crate::value::Value::from(new_path.to_string_lossy().as_ref()).literal();
                    text = doc::splice(&text, include.path_span.clone(), &literal);
                }
            }
            fs::write(target, text).map_err(|error| ApplyError::Io(error.to_string()))?;
        }
        Ok((main, replacements))
    }

    fn changes(&self, tree: &Tree) -> Result<Vec<TextChange>, ApplyError> {
        let mut out = Vec::new();
        for (real, scratch) in &self.map {
            let before = self.bases.get(real).cloned().flatten();
            let after = fs::read_to_string(scratch).ok();
            if before != after
                && let Some(after) = after
            {
                out.push(TextChange {
                    path: real.clone(),
                    before,
                    after: Some(after),
                });
            }
        }
        // A file that exists in the scratch tree but not in the map (created
        // outside the config folder) cannot be written safely.
        let _ = tree;
        Ok(out)
    }
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(name);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

/// Validate raw config text without writing the real file. The temp file
/// sits next to the target, so relative includes resolve the same way.
pub fn validate_text(path: &Path, text: &str) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(parent)
        .map_err(|error| format!("cannot create a temporary file: {error}"))?;
    temp.write_all(text.as_bytes())
        .map_err(|error| format!("cannot write the temporary file: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("cannot sync the temporary file: {error}"))?;
    let output = std::process::Command::new("timeout")
        .arg("10")
        .arg("niri")
        .arg("validate")
        .arg("-c")
        .arg(temp.path())
        .output()
        .or_else(|_| {
            std::process::Command::new("niri")
                .arg("validate")
                .arg("-c")
                .arg(temp.path())
                .output()
        })
        .map_err(|error| format!("cannot run niri: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let raw = String::from_utf8_lossy(&output.stderr);
    let raw = if raw.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        raw.to_string()
    };
    let replacements = vec![(temp.path().to_path_buf(), path.to_path_buf())];
    let message = clean_validation(&raw, &replacements);
    Err(message)
}

/// Write included files before the files that include them, and deletions
/// last. The overrides file goes first when it changes, so the main file never
/// references a file that is not on disk yet.
fn write_order(changes: &[TextChange], tree: &Tree, overrides: &Path) -> Vec<usize> {
    let find = |path: &Path| changes.iter().position(|change| change.path == path);
    let mut edges = Vec::new();

    if let Some(index) = find(overrides)
        && changes[index].after.is_some()
    {
        for other in 0..changes.len() {
            if other != index {
                edges.push((index, other));
            }
        }
    }

    // Old include edges: an included file that is being written must land
    // before the file that includes it.
    for include in &tree.includes {
        let Some(included) = find(&include.path) else {
            continue;
        };
        let Some(file) = tree.files.get(include.file) else {
            continue;
        };
        let Some(includer) = find(&file.path) else {
            continue;
        };
        if included != includer && changes[included].after.is_some() {
            edges.push((included, includer));
        }
    }

    // A file that is going away goes last, after the files that no longer
    // include it have been rewritten.
    for (index, change) in changes.iter().enumerate() {
        if change.after.is_none() {
            for (other, candidate) in changes.iter().enumerate() {
                if other != index && candidate.after.is_some() {
                    edges.push((other, index));
                }
            }
        }
    }

    edges.sort_unstable();
    edges.dedup();

    let mut incoming = vec![0usize; changes.len()];
    let mut outgoing = vec![Vec::new(); changes.len()];
    for (from, to) in edges {
        if from != to {
            outgoing[from].push(to);
            incoming[to] += 1;
        }
    }
    let mut ready: Vec<usize> = (0..changes.len())
        .filter(|index| incoming[*index] == 0)
        .collect();
    let mut order = Vec::with_capacity(changes.len());
    while let Some(index) = ready.first().copied() {
        ready.remove(0);
        order.push(index);
        for &next in &outgoing[index] {
            incoming[next] -= 1;
            if incoming[next] == 0 {
                ready.push(next);
            }
        }
    }
    if order.len() == changes.len() {
        order
    } else {
        // The old include graph disagrees with the new text. Keep the
        // original order instead of dropping a file.
        (0..changes.len()).collect()
    }
}

/// A write that failed, and whether the files written so far were restored.
struct WriteFailure {
    error: ApplyError,
    rolled_back: bool,
}

fn write_ordered(changes: &[TextChange], order: &[usize]) -> Result<(), WriteFailure> {
    let mut written = Vec::new();
    for &index in order {
        let change = &changes[index];
        let result = match &change.after {
            Some(after) => write_atomic(&change.path, after),
            None => {
                let result = fs::remove_file(&change.path);
                sync_parent(&change.path);
                result
            }
        };
        if let Err(error) = result {
            let rollback = rollback(changes, &written);
            let mut message = format!("{}: {error}", change.path.display());
            if let Err(rollback) = rollback {
                message.push_str(&format!("; rollback failed: {rollback}"));
                return Err(WriteFailure {
                    error: ApplyError::Io(message),
                    rolled_back: false,
                });
            }
            return Err(WriteFailure {
                error: ApplyError::Io(message),
                rolled_back: true,
            });
        }
        written.push(index);
    }
    Ok(())
}

/// Put back the files written so far, newest first. Used when a later rename
/// fails, so an I/O error does not leave a half-applied change.
fn rollback(changes: &[TextChange], written: &[usize]) -> std::io::Result<()> {
    let mut first = None;
    for &index in written.iter().rev() {
        let change = &changes[index];
        let result = match &change.before {
            Some(before) => write_atomic(&change.path, before),
            None => {
                let result = fs::remove_file(&change.path);
                sync_parent(&change.path);
                result
            }
        };
        if let Err(error) = result
            && first.is_none()
        {
            first = Some(error);
        }
    }
    match first {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Write text atomically, with the permissions of the old file when it exists.
pub fn write_text_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    write_atomic(path, text)
}

fn clean_validation(raw: &str, replacements: &[(PathBuf, PathBuf)]) -> String {
    let ansi = Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").expect("valid regex");
    let mut text = ansi.replace_all(raw, "").to_string();
    for (scratch, real_path) in replacements {
        text = text.replace(
            scratch.to_string_lossy().as_ref(),
            real_path.to_string_lossy().as_ref(),
        );
    }
    let text = text.trim();
    if text.is_empty() {
        "niri validate failed without a message".to_string()
    } else {
        text.to_string()
    }
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let permissions = fs::metadata(path).ok().map(|meta| meta.permissions());
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(text.as_bytes())?;
    temp.as_file().sync_all()?;
    if let Some(permissions) = permissions {
        let _ = temp.as_file().set_permissions(permissions);
    }
    temp.persist(path).map_err(|error| error.error)?;
    sync_parent(path);
    Ok(())
}

fn sync_parent(path: &Path) {
    if let Some(parent) = path.parent()
        && let Ok(dir) = fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }
}

fn parse_position(value: &Value) -> (String, String) {
    let text = value.as_str().unwrap_or("0,0");
    let mut parts = text.split(',');
    let x = parts.next().unwrap_or("0").trim().to_string();
    let y = parts.next().unwrap_or("0").trim().to_string();
    (x, y)
}

fn nix_value(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => {
            let mut out = format!("{value}");
            if !out.contains('.') && !out.contains('e') && !out.contains('E') {
                out.push_str(".0");
            }
            out
        }
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()),
        Value::Null => "null".to_string(),
    }
}

fn indent(text: &str, levels: usize) -> String {
    let prefix = "    ".repeat(levels);
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A machine-readable view of an option, for `--json`.
#[derive(Debug, Serialize)]
pub struct OptionJson {
    pub key: String,
    pub value: Value,
    pub source: String,
}

impl Value {
    pub fn json(&self) -> serde_json::Value {
        match self {
            Value::Bool(v) => serde_json::json!(v),
            Value::Int(v) => serde_json::json!(v),
            Value::Float(v) => serde_json::json!(v),
            Value::String(v) => serde_json::json!(v),
            Value::Null => serde_json::Value::Null,
        }
    }
}

// Keep the compiler honest about the unused import in release builds.
#[allow(dead_code)]
fn _assert_serde(_: &OptionJson) {}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    #[test]
    fn the_system_config_is_never_written() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        let text = "layout {\n    gaps 16\n}\n";
        fs::write(&path, text).unwrap();
        let state = tempfile::tempdir().unwrap();
        let mut session = Session::open_with(
            &path,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                system: true,
            },
        )
        .unwrap();

        assert!(!session.main_writable());
        assert_eq!(session.main_managed(), Some(Managed::System));
        let error = session.set("layout.gaps", Value::Float(20.0)).unwrap_err();
        assert!(matches!(error, ApplyError::SystemConfig(_)), "{error}");
        let error = session
            .write_raw(&path, "layout {\n    gaps 4\n}\n", "edit")
            .unwrap_err();
        assert!(matches!(error, ApplyError::SystemConfig(_)), "{error}");
        assert!(session.reload().is_ok());
        assert_eq!(session.main_managed(), Some(Managed::System));
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        assert!(!dir.path().join("niri-settings-overrides.kdl").exists());
    }

    #[test]
    fn watched_paths_include_includes() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let main = dir.path().join("config.kdl");
        let included = dir.path().join("colors.kdl");
        fs::write(&included, "layout {}\n").unwrap();
        fs::write(&main, "include \"colors.kdl\"\n").unwrap();
        let session = Session::open_with(
            &main,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                ..Default::default()
            },
        )
        .unwrap();
        let paths = session.watched_paths();
        assert!(paths.iter().any(|path| path == &included), "{paths:?}");
    }

    #[test]
    fn a_session_can_move_to_a_worker_thread() {
        fn assert_send_clone<T: Send + Clone + 'static>() {}
        assert_send_clone::<Session>();
    }

    #[test]
    fn a_missing_niri_fails_closed() {
        // Change PATH in a child process so tests that run in parallel keep
        // their own environment.
        if std::env::var_os("NIRI_SETTINGS_TEST_EMPTY_PATH").is_some() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.kdl");
            fs::write(&path, "layout {}\n").unwrap();
            let error = Session::open(&path).err().expect("open should fail");
            assert!(matches!(error, SessionError::NiriMissing), "{error}");
            return;
        }
        let exe = std::env::current_exe().unwrap();
        let empty = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(exe)
            .arg("session::tests::a_missing_niri_fails_closed")
            .arg("--exact")
            .arg("--quiet")
            .env("PATH", empty.path())
            .env("NIRI_SETTINGS_TEST_EMPTY_PATH", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }

    use super::*;

    fn open(text: &str) -> (tempfile::TempDir, Session) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        fs::write(&path, text).unwrap();
        let state = tempfile::tempdir().unwrap();
        let session = Session::open_with(
            &path,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                ..Default::default()
            },
        )
        .unwrap();
        (dir, session)
    }

    #[test]
    fn set_keeps_comments_and_undo_restores() {
        let (dir, mut session) = open("// header\nlayout {\n    gaps 10 // keep\n}\n");
        session.set("layout.gaps", Value::Int(16)).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("config.kdl")).unwrap(),
            "// header\nlayout {\n    gaps 16 // keep\n}\n"
        );
        session.undo().unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("config.kdl")).unwrap(),
            "// header\nlayout {\n    gaps 10 // keep\n}\n"
        );
    }

    #[test]
    fn reset_removes_the_value() {
        let (dir, mut session) = open("layout {\n    gaps 10\n}\n");
        session.reset("layout.gaps").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("config.kdl")).unwrap(),
            "layout {\n}\n"
        );
    }

    #[test]
    fn validates_raw_text() {
        if !command_exists("niri") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        assert!(validate_text(&path, "layout {\n    gaps 10\n}\n").is_ok());
        assert!(validate_text(&path, "layout {\n    gaps 10\n").is_err());
    }

    #[test]
    fn refuses_to_overwrite_an_external_change() {
        let (dir, mut session) = open("layout {\n    gaps 10\n}\n");
        fs::write(dir.path().join("config.kdl"), "layout {\n    gaps 99\n}\n").unwrap();
        let error = session.set("layout.gaps", Value::Int(20)).unwrap_err();
        assert!(matches!(error, ApplyError::Conflict(_)));
        assert!(
            fs::read_to_string(dir.path().join("config.kdl"))
                .unwrap()
                .contains("gaps 99")
        );
    }

    #[test]
    fn edits_a_writable_include() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("colors.kdl"), "layout {\n    gaps 8\n}\n").unwrap();
        let main = dir.path().join("config.kdl");
        fs::write(&main, "include \"colors.kdl\"\n").unwrap();
        let state = tempfile::tempdir().unwrap();
        let mut session = Session::open_with(
            &main,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                ..Default::default()
            },
        )
        .unwrap();
        session.set("layout.gaps", Value::Int(20)).unwrap();
        assert!(
            fs::read_to_string(dir.path().join("colors.kdl"))
                .unwrap()
                .contains("gaps 20")
        );
    }

    #[test]
    fn managed_include_gets_an_override_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("noctalia.kdl"),
            "// Managed by Noctalia\nlayout {\n    gaps 4\n}\n",
        )
        .unwrap();
        let main = dir.path().join("config.kdl");
        fs::write(&main, "include \"noctalia.kdl\"\n").unwrap();
        let state = tempfile::tempdir().unwrap();
        let mut session = Session::open_with(
            &main,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                ..Default::default()
            },
        )
        .unwrap();
        session.set("layout.gaps", Value::Int(24)).unwrap();
        let main_text = fs::read_to_string(&main).unwrap();
        assert!(main_text.contains("niri-settings-overrides.kdl"));
        let override_text =
            fs::read_to_string(dir.path().join("niri-settings-overrides.kdl")).unwrap();
        assert!(override_text.contains("gaps 24"));
        assert!(
            session
                .get("layout.gaps")
                .unwrap()
                .value
                .loose_eq(&Value::Int(24))
        );
    }

    #[test]
    fn animation_off_switch_uses_the_off_node() {
        let (dir, mut session) = open("animations {\n    window-open {\n        off\n    }\n}\n");
        assert!(
            !session
                .get("animations.window-open.off")
                .unwrap()
                .value
                .as_bool()
                .unwrap()
        );
        session
            .set("animations.window-open.off", Value::Bool(true))
            .unwrap();
        let text = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
        assert!(!text.contains("off"));
    }

    #[test]
    fn toggle_section_writes_on_and_off() {
        let (_dir, mut session) = open("layout {\n    border {\n        width 2\n    }\n}\n");
        session.set("layout.border.off", Value::Bool(true)).unwrap();
        assert!(
            session
                .get("layout.border.off")
                .unwrap()
                .value
                .as_bool()
                .unwrap()
        );
        session
            .set("layout.border.off", Value::Bool(false))
            .unwrap();
        assert!(
            !session
                .get("layout.border.off")
                .unwrap()
                .value
                .as_bool()
                .unwrap()
        );
    }

    #[test]
    fn applies_a_shortcut_scheme_to_the_override_file() {
        let (dir, mut session) = open("binds {\n    Mod+Q { close-window; }\n}\n");
        session
            .apply_binds(
                &[("Mod+Q", "quit"), ("Mod+D", "spawn \"fuzzel\"")],
                "Test scheme",
            )
            .unwrap();
        let main = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
        assert!(main.contains("niri-settings-overrides.kdl"));
        let overrides = fs::read_to_string(dir.path().join("niri-settings-overrides.kdl")).unwrap();
        assert!(overrides.contains("Mod+Q { quit; }"));
        assert!(overrides.contains("Mod+D"));
    }

    #[test]
    fn import_is_one_undo_step() {
        let (dir, mut session) =
            open("binds {\n    Mod+Q { close-window; }\n}\nlayout {\n    gaps 4\n}\n");
        let hypr = "general {\n    gaps_out = 20\n}\nbind = $mod, Return, exec, foot\nexec-once = waybar\n";
        let result = crate::import::parse(hypr, crate::import::Source::Hyprland);
        session.apply_import(&result).unwrap();
        let text = fs::read_to_string(dir.path().join("niri-settings-overrides.kdl")).unwrap();
        assert!(text.contains("gaps 20"));
        assert!(text.contains("Mod+Return"));
        assert!(text.contains("spawn-at-startup \"waybar\""));
        assert_eq!(session.history().len(), 1);
        session.undo().unwrap();
        assert!(!dir.path().join("niri-settings-overrides.kdl").exists());
    }

    #[test]
    fn edits_a_window_rule() {
        let (dir, mut session) =
            open("window-rule {\n    match app-id=\"^firefox$\"\n    open-floating true\n}\n");
        session
            .edit_window_rule(0, "match app-id=\"^firefox$\"\nopen-floating false")
            .unwrap();
        let text = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
        assert!(text.contains("open-floating false"));
    }

    #[test]
    fn adds_and_removes_a_workspace() {
        let (dir, mut session) = open("layout {}\n");
        session.add_workspace("browser", "HDMI-A-1").unwrap();
        let text = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
        assert!(text.contains("workspace \"browser\""));
        assert!(text.contains("open-on-output \"HDMI-A-1\""));
        session.remove_workspace("browser").unwrap();
        let text = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
        assert!(!text.contains("browser"));
    }

    #[test]
    fn nix_snippet_names_changed_options() {
        let (_dir, mut session) = open("layout {}\n");
        session.set("layout.gaps", Value::Int(20)).unwrap();
        let snippet = session.nix_snippet();
        assert!(snippet.contains("programs.niri.settings"));
        assert!(snippet.contains("layout.gaps = 20.0;"));
    }

    #[test]
    fn every_option_writes_a_config_that_niri_accepts() {
        if !command_exists("niri") {
            return;
        }
        let (dir, mut session) = open("layout {}\n");
        for option in crate::schema::OPTIONS {
            let value = test_value(option);
            session
                .set(option.key, value)
                .unwrap_or_else(|error| panic!("setting {} failed: {error}", option.key));
        }
        let path = dir.path().join("config.kdl");
        let output = std::process::Command::new("niri")
            .arg("validate")
            .arg("-c")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "niri rejected the generated config:\n{}
--- file ---\n{}",
            String::from_utf8_lossy(&output.stderr),
            fs::read_to_string(&path).unwrap()
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn random_edits_are_atomic(bytes in prop::collection::vec(any::<u8>(), 1..48)) {
            let (dir, mut session) = open("layout {}\n");
            let path = dir.path().join("config.kdl");
            for byte in bytes {
                let option =
                    &crate::schema::OPTIONS[byte as usize % crate::schema::OPTIONS.len()];
                let before = fs::read_to_string(&path).unwrap();
                match session.set(option.key, test_value(option)) {
                    Ok(_) => {
                        let after = fs::read_to_string(&path).unwrap();
                        prop_assert!(before != after, "{} reported a change", option.key);
                        prop_assert!(doc::load(&path).is_ok(), "{} left broken KDL", option.key);
                        let main = session.tree.main_index();
                        prop_assert_eq!(&after, &session.tree.files[main].text);
                    }
                    Err(ApplyError::NoChange) => {
                        prop_assert_eq!(before, fs::read_to_string(&path).unwrap());
                    }
                    Err(error) => prop_assert!(false, "{}: {error}", option.key),
                }
            }
            if command_exists("niri") {
                let output = std::process::Command::new("niri")
                    .arg("validate")
                    .arg("-c")
                    .arg(&path)
                    .output()
                    .unwrap();
                prop_assert!(
                    output.status.success(),
                    "niri rejected the final config: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }

    #[test]
    fn a_failed_write_rolls_back_earlier_files() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("one.kdl");
        fs::write(&first, "old\n").unwrap();
        let blocked = dir.path().join("blocked");
        fs::write(&blocked, "not a directory").unwrap();
        let second = blocked.join("two.kdl");
        let changes = vec![
            TextChange {
                path: first.clone(),
                before: Some("old\n".to_string()),
                after: Some("new\n".to_string()),
            },
            TextChange {
                path: second,
                before: None,
                after: Some("new\n".to_string()),
            },
        ];

        let failure = write_ordered(&changes, &[0, 1]).unwrap_err();
        assert!(
            matches!(&failure.error, ApplyError::Io(_)),
            "{}",
            failure.error
        );
        assert!(failure.rolled_back);
        assert_eq!(fs::read_to_string(&first).unwrap(), "old\n");
    }

    #[test]
    fn a_successful_write_leaves_no_journal() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        fs::write(&path, "layout {}\n").unwrap();
        let mut session = Session::open_with(
            &path,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                ..Default::default()
            },
        )
        .unwrap();

        session.set("layout.gaps", Value::from(12.0)).unwrap();

        let journal = Journal::new(state.path().to_path_buf());
        assert!(!journal.path().exists());
        assert!(session.recovery.is_none());
    }

    #[test]
    fn an_uncommitted_journal_rolls_back_on_open() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let main = dir.path().join("config.kdl");
        let extra = dir.path().join("extra.kdl");
        let old_main = "include \"extra.kdl\"\nlayout {\n    gaps 8\n}\n";
        let new_main = "include \"extra.kdl\"\nlayout {\n    gaps 16\n}\n";
        let old_extra = "prefer-no-csd\n";
        let new_extra = "prefer-no-csd\ncursor {\n    xcursor-size 24\n}\n";
        fs::write(&main, old_main).unwrap();
        fs::write(&extra, old_extra).unwrap();
        let mut history = History::open(state.path().to_path_buf());
        let entry = history
            .record(
                "Change".to_string(),
                vec![
                    HistoryFile {
                        path: main.clone(),
                        before: Some(old_main.to_string()),
                        after: new_main.to_string(),
                    },
                    HistoryFile {
                        path: extra.clone(),
                        before: Some(old_extra.to_string()),
                        after: new_extra.to_string(),
                    },
                ],
            )
            .unwrap();
        let journal = Journal::new(state.path().to_path_buf());
        journal
            .write(&PendingWrite {
                summary: "Change".to_string(),
                history_id: Some(entry.id),
                committed: false,
                files: vec![
                    PendingFile {
                        path: main.clone(),
                        before: Some(old_main.to_string()),
                        after: Some(new_main.to_string()),
                    },
                    PendingFile {
                        path: extra.clone(),
                        before: Some(old_extra.to_string()),
                        after: Some(new_extra.to_string()),
                    },
                ],
            })
            .unwrap();
        // The crash happened after the first rename.
        fs::write(&main, new_main).unwrap();

        let session = Session::open_with(
            &main,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&main).unwrap(), old_main);
        assert_eq!(fs::read_to_string(&extra).unwrap(), old_extra);
        assert!(!journal.path().exists());
        assert!(session.history.entries().is_empty());
        assert!(!session.recovery.expect("recovery notice").committed);
    }

    #[test]
    fn a_committed_journal_finishes_on_open() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let main = dir.path().join("config.kdl");
        let old_main = "layout {\n    gaps 8\n}\n";
        let new_main = "layout {\n    gaps 16\n}\n";
        fs::write(&main, old_main).unwrap();
        let mut history = History::open(state.path().to_path_buf());
        let entry = history
            .record(
                "Change".to_string(),
                vec![HistoryFile {
                    path: main.clone(),
                    before: Some(old_main.to_string()),
                    after: new_main.to_string(),
                }],
            )
            .unwrap();
        let journal = Journal::new(state.path().to_path_buf());
        journal
            .write(&PendingWrite {
                summary: "Change".to_string(),
                history_id: Some(entry.id),
                committed: true,
                files: vec![PendingFile {
                    path: main.clone(),
                    before: Some(old_main.to_string()),
                    after: Some(new_main.to_string()),
                }],
            })
            .unwrap();

        let session = Session::open_with(
            &main,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&main).unwrap(), new_main);
        assert!(!journal.path().exists());
        assert_eq!(session.history.entries().len(), 1);
        assert!(session.recovery.expect("recovery notice").committed);
    }

    #[test]
    fn a_journal_conflict_is_reported_and_left_alone() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let main = dir.path().join("config.kdl");
        let old_main = "layout {\n    gaps 8\n}\n";
        let new_main = "layout {\n    gaps 16\n}\n";
        let external = "layout {\n    gaps 99\n}\n";
        fs::write(&main, external).unwrap();
        let journal = Journal::new(state.path().to_path_buf());
        journal
            .write(&PendingWrite {
                summary: "Change".to_string(),
                history_id: None,
                committed: false,
                files: vec![PendingFile {
                    path: main.clone(),
                    before: Some(old_main.to_string()),
                    after: Some(new_main.to_string()),
                }],
            })
            .unwrap();

        let session = Session::open_with(
            &main,
            SessionOptions {
                state_dir: Some(state.path().to_path_buf()),
                validate: false,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&main).unwrap(), external);
        assert!(!journal.path().exists());
        assert_eq!(
            session.recovery.expect("recovery notice").conflicts,
            vec![main]
        );
    }

    #[test]
    fn writes_includes_before_the_file_that_includes_them() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("config.kdl");
        let colors = dir.path().join("colors.kdl");
        let overrides = dir.path().join("niri-settings-overrides.kdl");
        fs::write(&colors, "layout {}\n").unwrap();
        fs::write(&main, "include \"colors.kdl\"\n").unwrap();
        let tree = doc::load(&main).unwrap();
        let changes = vec![
            TextChange {
                path: main.clone(),
                before: Some("include \"colors.kdl\"\n".to_string()),
                after: Some("include \"colors.kdl\"\n".to_string()),
            },
            TextChange {
                path: colors.clone(),
                before: Some("layout {}\n".to_string()),
                after: Some("layout { gaps 8; }\n".to_string()),
            },
            TextChange {
                path: overrides.clone(),
                before: None,
                after: Some("layout {}\n".to_string()),
            },
        ];

        let order = write_order(&changes, &tree, &overrides);
        let position = |path: &PathBuf| {
            order
                .iter()
                .position(|index| &changes[*index].path == path)
                .unwrap()
        };
        assert!(position(&overrides) < position(&colors));
        assert!(position(&colors) < position(&main));
    }

    fn test_value(option: &OptionInfo) -> Value {
        match option.control {
            schema::Control::Toggle { default, .. } => Value::Bool(!default),
            schema::Control::Number {
                default,
                min,
                max,
                step,
                ..
            } => {
                let value = (default + step).clamp(min, max);
                Value::Float(value)
            }
            schema::Control::Text { .. } => match option.key {
                "input.keyboard.xkb.layout" => Value::from("us,de"),
                "input.keyboard.xkb.variant" => Value::from("colemak"),
                "input.keyboard.xkb.options" => Value::from("compose:caps"),
                "input.keyboard.xkb.model" => Value::from("pc105"),
                "cursor.xcursor-theme" => Value::from("Adwaita"),
                "screenshot-path" => Value::from("~/Pictures/test.png"),
                "input.focus-follows-mouse.max-scroll-amount" => Value::from("10%"),
                "input.warp-mouse-to-focus.mode" => Value::from("center-xy-always"),
                _ => Value::from("test"),
            },
            schema::Control::Color { .. } => Value::from("#123456"),
            schema::Control::Choice {
                choices, default, ..
            } => Value::from(
                choices
                    .iter()
                    .find(|choice| choice.value != default)
                    .map(|choice| choice.value)
                    .unwrap_or(default),
            ),
        }
    }

    #[test]
    fn sets_an_output_field() {
        let (dir, mut session) =
            open("output \"HDMI-A-1\" {\n    mode \"1920x1080@60.000\"\n    scale 1\n}\n");
        session
            .set_output("HDMI-A-1", "scale", Value::Float(1.5))
            .unwrap();
        let text = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
        assert!(text.contains("scale 1.5"));
        session
            .set_output("HDMI-A-1", "vrr", Value::from("on"))
            .unwrap();
        session
            .set_output("HDMI-A-1", "position", Value::from("1920,0"))
            .unwrap();
        let text = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
        assert!(text.contains("vrr \"on\""));
        assert!(text.contains("position x=1920 y=0"));
    }

    #[test]
    fn adding_and_removing_a_bind() {
        let (dir, mut session) = open("binds {\n    Mod+Q { close-window; }\n}\n");
        session.add_bind("Mod+T", "spawn \"foot\"").unwrap();
        assert!(
            fs::read_to_string(dir.path().join("config.kdl"))
                .unwrap()
                .contains("Mod+T")
        );
        session.remove_bind("Mod+T").unwrap();
        assert!(
            !fs::read_to_string(dir.path().join("config.kdl"))
                .unwrap()
                .contains("Mod+T")
        );
    }
}
