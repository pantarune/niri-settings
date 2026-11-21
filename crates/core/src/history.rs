//! Change history.
//!
//! Every applied change stores the full text before and after for each file it
//! touched. Undo is another change, so it is also history. Entries live in
//! `$XDG_STATE_HOME/niri-settings/history` and the last 200 are kept.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use thiserror::Error;

/// One file in a history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryFile {
    pub path: PathBuf,
    /// `None` when the file did not exist before the change.
    pub before: Option<String>,
    pub after: String,
}

/// One undoable change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    /// Seconds since the UNIX epoch.
    pub time: u64,
    pub summary: String,
    pub files: Vec<HistoryFile>,
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

/// The on-disk history.
#[derive(Debug, Clone)]
pub struct History {
    dir: PathBuf,
    entries: Vec<HistoryEntry>,
    next_id: u64,
}

impl History {
    pub fn open(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let mut entries = Vec::new();
        if let Ok(read) = fs::read_dir(&dir) {
            for item in read.flatten() {
                let path = item.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(&path)
                    && let Ok(entry) = serde_json::from_str::<HistoryEntry>(&text)
                {
                    entries.push(entry);
                }
            }
        }
        entries.sort_by_key(|entry| entry.id);
        let next_id = entries.last().map(|entry| entry.id + 1).unwrap_or(1);
        Self {
            dir,
            entries,
            next_id,
        }
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn latest(&self) -> Option<&HistoryEntry> {
        self.entries.last()
    }

    pub fn record(
        &mut self,
        summary: String,
        files: Vec<HistoryFile>,
    ) -> Result<HistoryEntry, HistoryError> {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let entry = HistoryEntry {
            id: self.next_id,
            time,
            summary,
            files,
        };
        self.next_id += 1;
        // Save first. A change that cannot be recorded must not be written.
        self.save(&entry)?;
        self.entries.push(entry.clone());
        self.trim();
        Ok(entry)
    }

    /// Remove the newest entry from memory, but keep its file on disk until
    /// the undo that consumes it succeeds. A crash then still leaves the undo
    /// record behind.
    pub fn pop(&mut self) -> Option<HistoryEntry> {
        self.entries.pop()
    }

    /// Delete the file for an entry that was consumed successfully.
    pub fn forget(&mut self, id: u64) {
        let _ = fs::remove_file(self.path_for(id));
    }

    /// Drop an entry that was recorded but whose write failed.
    pub fn discard(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
        let _ = fs::remove_file(self.path_for(id));
    }

    pub fn push_back(&mut self, entry: HistoryEntry) {
        self.entries.push(entry);
        self.entries.sort_by_key(|e| e.id);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        if let Ok(read) = fs::read_dir(&self.dir) {
            for item in read.flatten() {
                let path = item.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }

    fn path_for(&self, id: u64) -> PathBuf {
        self.dir.join(format!("{id:010}.json"))
    }

    fn save(&self, entry: &HistoryEntry) -> Result<(), HistoryError> {
        fs::create_dir_all(&self.dir)?;
        let text = serde_json::to_string_pretty(entry)?;
        let path = self.path_for(entry.id);
        let mut temp = NamedTempFile::new_in(&self.dir)?;
        temp.write_all(text.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&path).map_err(|error| error.error)?;
        sync_parent(&path);
        Ok(())
    }

    fn trim(&mut self) {
        while self.entries.len() > 200 {
            let entry = self.entries.remove(0);
            let _ = fs::remove_file(self.path_for(entry.id));
        }
    }
}

/// The default history directory.
pub fn default_dir() -> PathBuf {
    if let Some(state) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state).join("niri-settings/history");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/niri-settings/history");
    }
    Path::new("/tmp").join("niri-settings/history")
}

fn sync_parent(path: &Path) {
    if let Some(parent) = path.parent()
        && let Ok(dir) = fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let mut history = History::open(dir.path().to_path_buf());
        let entry = history
            .record(
                "Gaps 8 → 16 px".to_string(),
                vec![HistoryFile {
                    path: dir.path().join("config.kdl"),
                    before: Some("layout { gaps 8 }".to_string()),
                    after: "layout { gaps 16 }".to_string(),
                }],
            )
            .unwrap();
        let loaded = History::open(dir.path().to_path_buf());
        assert_eq!(loaded.entries().len(), 1);
        assert_eq!(loaded.entries()[0].summary, "Gaps 8 → 16 px");
        assert_eq!(loaded.entries()[0].id, entry.id);
    }

    #[test]
    fn keeps_the_last_200() {
        let dir = tempfile::tempdir().unwrap();
        let mut history = History::open(dir.path().to_path_buf());
        for i in 0..205 {
            history.record(format!("change {i}"), Vec::new()).unwrap();
        }
        assert_eq!(history.entries().len(), 200);
        assert_eq!(history.entries()[0].summary, "change 5");
    }

    #[test]
    fn pop_keeps_the_undo_record_until_forget() {
        let dir = tempfile::tempdir().unwrap();
        let mut history = History::open(dir.path().to_path_buf());
        let entry = history.record("change".to_string(), Vec::new()).unwrap();
        let path = dir.path().join(format!("{:010}.json", entry.id));

        let popped = history.pop().unwrap();
        assert_eq!(popped.id, entry.id);
        assert!(path.exists(), "pop must not delete the undo record");

        history.forget(entry.id);
        assert!(!path.exists(), "forget deletes the consumed record");
    }

    #[test]
    fn a_failed_save_records_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        let mut history = History::open(blocked);
        assert!(history.record("change".to_string(), Vec::new()).is_err());
        assert!(history.entries().is_empty());
    }
}
