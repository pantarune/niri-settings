//! Real-world config fixtures. The test copies a fixture to a temporary
//! folder, edits it, and checks the original text after Undo.

use std::fs;
use std::path::{Path, PathBuf};

use niri_settings_core::Value;
use niri_settings_core::doc;
use niri_settings_core::session::{Session, SessionOptions};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/configs")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn open_fixture(name: &str, config: &str) -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let source = fixtures().join(name);
    if source.is_dir() {
        copy_dir(&source, dir.path());
    } else {
        fs::copy(&source, dir.path().join("config.kdl")).unwrap();
    }
    let state = tempfile::tempdir().unwrap();
    let session = Session::open_with(
        &dir.path().join(config),
        SessionOptions {
            state_dir: Some(state.path().to_path_buf()),
            validate: true,
            ..Default::default()
        },
    )
    .unwrap();
    (dir, session)
}

#[test]
fn default_config_round_trips_and_undoes() {
    let (dir, mut session) = open_fixture("default.kdl", "config.kdl");
    let before = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
    for file in &session.tree.files {
        assert_eq!(file.text, fs::read_to_string(&file.path).unwrap());
    }
    session.set("layout.gaps", Value::Int(24)).unwrap();
    session.undo().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("config.kdl")).unwrap(),
        before
    );
}

#[test]
fn dotfiles_keep_their_comments() {
    let (dir, mut session) = open_fixture("dotfiles", "config.kdl");
    let before = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
    session.set("layout.gaps", Value::Int(24)).unwrap();
    let after = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
    assert!(after.contains("// My dotfiles. Keep this comment."));
    assert!(after.contains("gaps 24"));
    assert!(after.contains("// Values set after the include win."));
    session.undo().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("config.kdl")).unwrap(),
        before
    );
}

#[test]
fn managed_include_uses_the_override_file() {
    let (dir, mut session) = open_fixture("noctalia", "config.kdl");
    session
        .set("layout.focus-ring.active-color", Value::from("#ff0000"))
        .unwrap();
    let main = fs::read_to_string(dir.path().join("config.kdl")).unwrap();
    assert!(main.contains("niri-settings-overrides.kdl"));
    let overrides = fs::read_to_string(dir.path().join("niri-settings-overrides.kdl")).unwrap();
    assert!(overrides.contains("#ff0000"));
    assert_eq!(
        fs::read_to_string(dir.path().join("noctalia.kdl")).unwrap(),
        fs::read_to_string(fixtures().join("noctalia/noctalia.kdl")).unwrap()
    );
}

#[test]
fn edits_an_absolute_include_outside_the_config_folder() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let extra = outside.path().join("extra.kdl");
    fs::write(&extra, "layout {\n    gaps 8\n}\n").unwrap();
    let main = dir.path().join("config.kdl");
    fs::write(&main, format!("include \"{}\"\n", extra.display())).unwrap();
    let state = tempfile::tempdir().unwrap();
    let mut session = Session::open_with(
        &main,
        SessionOptions {
            state_dir: Some(state.path().to_path_buf()),
            validate: true,
            ..Default::default()
        },
    )
    .unwrap();
    session.set("layout.gaps", Value::Int(18)).unwrap();
    assert!(fs::read_to_string(&extra).unwrap().contains("gaps 18"));
}

#[cfg(unix)]
#[test]
fn writes_through_a_symlink() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("dotfiles-config.kdl");
    fs::write(&target, "layout {\n    gaps 8\n}\n").unwrap();
    let link = dir.path().join("config.kdl");
    symlink(&target, &link).unwrap();
    let state = tempfile::tempdir().unwrap();
    let mut session = Session::open_with(
        &link,
        SessionOptions {
            state_dir: Some(state.path().to_path_buf()),
            validate: false,
            ..Default::default()
        },
    )
    .unwrap();
    session.set("layout.gaps", Value::Int(12)).unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(fs::read_to_string(&target).unwrap().contains("gaps 12"));
}

#[test]
fn a_broken_file_is_reported() {
    let source = fixtures().join("broken.kdl");
    let error = doc::load(&source).unwrap_err();
    assert!(matches!(error, doc::LoadError::Parse { .. }));
}
