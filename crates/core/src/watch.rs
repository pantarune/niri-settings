//! Watch the config folder so that edits from other programs show up at once.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// A watcher for one or more folders. Events are coalesced by the caller.
pub struct ConfigWatcher {
    receiver: Receiver<()>,
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// True when something changed since the last call.
    pub fn changed(&self) -> bool {
        let mut changed = false;
        while self.receiver.try_recv().is_ok() {
            changed = true;
        }
        changed
    }

    /// Block for one event, with a timeout. The app uses the polling form.
    pub fn wait(&self, timeout: Duration) -> bool {
        self.receiver.recv_timeout(timeout).is_ok()
    }
}

/// Watch the parent folders of these files. A rename replaces the inode, so
/// watching the folder is more reliable than watching the file.
pub fn watch(paths: &[PathBuf]) -> Option<ConfigWatcher> {
    let (sender, receiver) = channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = sender.send(());
        }
    })
    .ok()?;
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        if let Some(parent) = path.parent()
            && seen.insert(parent.to_path_buf())
        {
            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
        }
    }
    Some(ConfigWatcher {
        receiver,
        _watcher: watcher,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_a_file_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, "layout {}\n").unwrap();
        let watcher = watch(std::slice::from_ref(&path)).unwrap();
        std::fs::write(&path, "layout { gaps 8; }\n").unwrap();
        assert!(watcher.wait(Duration::from_secs(3)));
    }

    #[test]
    fn sees_a_change_in_another_folder() {
        let main_dir = tempfile::tempdir().unwrap();
        let other_dir = tempfile::tempdir().unwrap();
        let main = main_dir.path().join("config.kdl");
        let included = other_dir.path().join("colors.kdl");
        std::fs::write(&main, "layout {}\n").unwrap();
        std::fs::write(&included, "layout {}\n").unwrap();
        let watcher = watch(&[main, included.clone()]).unwrap();
        std::fs::write(&included, "layout { gaps 8; }\n").unwrap();
        assert!(watcher.wait(Duration::from_secs(3)));
    }
}
