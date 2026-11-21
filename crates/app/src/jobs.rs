//! Work that runs off the GTK thread: writes and setup checks.
//!
//! A write runs `niri validate`, syncs the file to disk and records history.
//! That takes 50–150 ms, and much longer when the disk is busy, so it runs
//! on a worker thread with a copy of the session. The worker sends the
//! updated session back; the window keeps drawing meanwhile.
//!
//! Only one write runs at a time. Changes made meanwhile wait in a queue, in
//! order, and repeated changes to the same setting collapse into the last
//! one. Setup checks run the same way after the config changes.

use std::collections::VecDeque;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use niri_settings_core::Value;
use niri_settings_core::doc::Tree;
use niri_settings_core::lint::{self, LintContext, Problem};
use niri_settings_core::session::{Applied, ApplyError, Session};

/// A change that runs on the worker thread.
#[derive(Debug, Clone)]
pub enum Job {
    /// Set a value. `rebuild` rebuilds the page afterwards.
    Set {
        key: String,
        value: Value,
        rebuild: bool,
    },
    Reset(String),
    Undo,
    Redo,
}

/// What a finished job sends back.
pub struct Outcome {
    pub session: Session,
    pub job: Job,
    pub result: Result<Applied, ApplyError>,
}

/// A finished job, ready to cross back to the GTK thread inside a message.
/// The outcome is taken out once.
#[derive(Clone)]
pub struct Finished(Arc<Mutex<Option<Outcome>>>);

impl Finished {
    pub fn take(&self) -> Option<Outcome> {
        self.0.lock().ok()?.take()
    }
}

impl fmt::Debug for Finished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Finished")
    }
}

/// Run a job on a worker thread and hand the outcome to `done`.
pub fn run(mut session: Session, job: Job, done: impl FnOnce(Finished) + Send + 'static) {
    std::thread::spawn(move || {
        let result = match &job {
            Job::Set { key, value, .. } => session.set(key, value.clone()),
            Job::Reset(key) => session.reset(key),
            Job::Undo => session.undo(),
            Job::Redo => session.redo(),
        };
        done(Finished(Arc::new(Mutex::new(Some(Outcome {
            session,
            job,
            result,
        })))));
    });
}

/// Setup check results for one version of the config.
#[derive(Clone)]
pub struct Checked {
    pub fingerprint: u64,
    pub problems: Arc<Vec<Problem>>,
}

impl fmt::Debug for Checked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Checked({} problems)", self.problems.len())
    }
}

/// Run the setup checks on a worker thread. They call `niri validate` and
/// `niri msg`, which takes about 60 ms.
pub fn check(tree: Tree, done: impl FnOnce(Checked) + Send + 'static) {
    std::thread::spawn(move || {
        let fingerprint = fingerprint(&tree);
        let problems = lint::check(&tree, &LintContext::from_system());
        done(Checked {
            fingerprint,
            problems: Arc::new(problems),
        });
    });
}

/// A cheap identity for the loaded files, to know when checks are stale.
pub fn fingerprint(tree: &Tree) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in &tree.files {
        file.path.hash(&mut hasher);
        file.text.hash(&mut hasher);
    }
    hasher.finish()
}

/// Queue a change that arrived while a write was running. A later change to
/// the same setting replaces the earlier one in place, so dragging through
/// values writes once, not once per value.
pub fn enqueue<M>(queue: &mut VecDeque<M>, message: M, key_of: impl Fn(&M) -> Option<&str>) {
    if let Some(key) = key_of(&message)
        && let Some(slot) = queue.iter_mut().find(|queued| key_of(queued) == Some(key))
    {
        *slot = message;
        return;
    }
    queue.push_back(message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum M {
        Set(&'static str, i32),
        Undo,
    }

    fn key(message: &M) -> Option<&str> {
        match message {
            M::Set(key, _) => Some(key),
            M::Undo => None,
        }
    }

    #[test]
    fn a_later_value_replaces_a_queued_one_for_the_same_setting() {
        let mut queue = VecDeque::new();
        enqueue(&mut queue, M::Set("layout.gaps", 10), key);
        enqueue(&mut queue, M::Set("layout.shadow.on", 1), key);
        enqueue(&mut queue, M::Set("layout.gaps", 20), key);
        assert_eq!(
            Vec::from(queue),
            [M::Set("layout.gaps", 20), M::Set("layout.shadow.on", 1)]
        );
    }

    #[test]
    fn other_changes_keep_their_order() {
        let mut queue = VecDeque::new();
        enqueue(&mut queue, M::Undo, key);
        enqueue(&mut queue, M::Set("layout.gaps", 10), key);
        enqueue(&mut queue, M::Undo, key);
        assert_eq!(
            Vec::from(queue),
            [M::Undo, M::Set("layout.gaps", 10), M::Undo]
        );
    }

    #[test]
    fn the_fingerprint_follows_the_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, "layout {\n    gaps 16\n}\n").unwrap();
        let first = niri_settings_core::doc::load(&path).unwrap();
        let again = niri_settings_core::doc::load(&path).unwrap();
        assert_eq!(fingerprint(&first), fingerprint(&again));
        std::fs::write(&path, "layout {\n    gaps 20\n}\n").unwrap();
        let changed = niri_settings_core::doc::load(&path).unwrap();
        assert_ne!(fingerprint(&first), fingerprint(&changed));
    }
}
