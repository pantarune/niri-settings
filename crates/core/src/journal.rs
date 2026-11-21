//! The pending-write journal.
//!
//! A change that touches several files cannot be one atomic rename. Before
//! the first rename, the full before/after text of every file is saved here.
//! If the process stops in the middle, the next `Session::open` finishes or
//! undoes the change, so a crash cannot leave a mix of old and new files.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::session::write_text_atomic;

const JOURNAL_NAME: &str = "pending-write.journal";

/// One file in a pending write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingFile {
    pub path: PathBuf,
    /// `None` when the file did not exist before the change.
    pub before: Option<String>,
    /// `None` when the change removes the file.
    pub after: Option<String>,
}

/// The change that is in flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingWrite {
    pub summary: String,
    /// The history entry for this change, when one was recorded.
    pub history_id: Option<u64>,
    /// Set after every rename succeeded. A committed journal is rolled
    /// forward; an uncommitted one is rolled back.
    pub committed: bool,
    pub files: Vec<PendingFile>,
}

/// What happened when an old journal was replayed.
#[derive(Debug, Clone)]
pub struct Recovery {
    pub committed: bool,
    pub summary: String,
    pub history_id: Option<u64>,
    /// Files another program changed after the interrupted write. They were
    /// left exactly as found.
    pub conflicts: Vec<PathBuf>,
}

impl Recovery {
    pub fn message(&self) -> String {
        let action = if self.committed {
            "Finished an interrupted change"
        } else {
            "Rolled back an interrupted change"
        };
        let mut message = if self.summary.is_empty() {
            format!("{action}.")
        } else {
            format!("{action}: {}.", self.summary)
        };
        if !self.conflicts.is_empty() {
            let names: Vec<String> = self
                .conflicts
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            message.push_str(&format!(
                " Another program changed {}; that file was left as it is.",
                names.join(", ")
            ));
        }
        message
    }
}

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("cannot save the recovery journal at {}: {source}", .path.display())]
    Save {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot encode the recovery journal at {}: {source}", .path.display())]
    Encode {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cannot read the recovery journal at {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the recovery journal at {} is damaged: {source}", .path.display())]
    Decode {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cannot restore {}: {source}", .path.display())]
    Restore {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The journal in one state directory.
#[derive(Debug, Clone)]
pub struct Journal {
    dir: PathBuf,
}

impl Journal {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.dir.join(JOURNAL_NAME)
    }

    pub fn load(&self) -> Result<Option<PendingWrite>, JournalError> {
        let path = self.path();
        match fs::read_to_string(&path) {
            Ok(text) => {
                let pending =
                    serde_json::from_str(&text).map_err(|source| JournalError::Decode {
                        path: path.clone(),
                        source,
                    })?;
                Ok(Some(pending))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(JournalError::Read { path, source }),
        }
    }

    pub fn write(&self, pending: &PendingWrite) -> Result<(), JournalError> {
        fs::create_dir_all(&self.dir).map_err(|source| JournalError::Save {
            path: self.path(),
            source,
        })?;
        let path = self.path();
        let text =
            serde_json::to_string_pretty(pending).map_err(|source| JournalError::Encode {
                path: path.clone(),
                source,
            })?;
        let mut temp = NamedTempFile::new_in(&self.dir).map_err(|source| JournalError::Save {
            path: path.clone(),
            source,
        })?;
        temp.write_all(text.as_bytes())
            .map_err(|source| JournalError::Save {
                path: path.clone(),
                source,
            })?;
        temp.as_file()
            .sync_all()
            .map_err(|source| JournalError::Save {
                path: path.clone(),
                source,
            })?;
        temp.persist(&path).map_err(|error| JournalError::Save {
            path: path.clone(),
            source: error.error,
        })?;
        sync_parent(&path);
        Ok(())
    }

    pub fn clear(&self) -> Result<(), JournalError> {
        let path = self.path();
        match fs::remove_file(&path) {
            Ok(()) => {
                sync_parent(&path);
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(JournalError::Save { path, source }),
        }
    }

    /// Replay an old journal. Returns `None` when there is nothing to do.
    ///
    /// A committed change is rolled forward; an uncommitted one is rolled
    /// back. A file that matches neither side was changed by another program
    /// while the journal waited, so it is left alone and reported.
    pub fn recover(&self) -> Result<Option<Recovery>, JournalError> {
        let Some(pending) = self.load()? else {
            return Ok(None);
        };
        let mut conflicts = Vec::new();
        for file in &pending.files {
            let current = read_current(&file.path)?;
            let (target, other) = if pending.committed {
                (file.after.as_deref(), file.before.as_deref())
            } else {
                (file.before.as_deref(), file.after.as_deref())
            };
            if current.as_deref() == target {
                continue;
            }
            if current.as_deref() == other {
                restore(&file.path, target)?;
            } else {
                conflicts.push(file.path.clone());
            }
        }
        // A journal that cannot be removed is replayed again next time. The
        // replay is idempotent, so do not block the app on cleanup.
        let _ = self.clear();
        Ok(Some(Recovery {
            committed: pending.committed,
            summary: pending.summary,
            history_id: pending.history_id,
            conflicts,
        }))
    }
}

fn read_current(path: &Path) -> Result<Option<String>, JournalError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(JournalError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn restore(path: &Path, text: Option<&str>) -> Result<(), JournalError> {
    match text {
        Some(text) => write_text_atomic(path, text).map_err(|source| JournalError::Restore {
            path: path.to_path_buf(),
            source,
        }),
        None => match fs::remove_file(path) {
            Ok(()) => {
                sync_parent(path);
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(JournalError::Restore {
                path: path.to_path_buf(),
                source,
            }),
        },
    }
}

fn sync_parent(path: &Path) {
    if let Some(parent) = path.parent()
        && let Ok(dir) = fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }
}
