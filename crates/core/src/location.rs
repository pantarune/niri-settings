//! Find the config file the way niri does.
//!
//! Checked against niri 26.04 (`src/main.rs` `config_path`,
//! `niri-config/src/lib.rs` `ConfigPath::load_inner`):
//!
//! 1. `--config`, then `$NIRI_CONFIG` when it is not empty. niri loads only
//!    that file and never creates it.
//! 2. Otherwise the user file, `$XDG_CONFIG_HOME/niri/config.kdl` or
//!    `~/.config/niri/config.kdl`, if it exists.
//! 3. Otherwise `/etc/niri/config.kdl`, if it exists.
//! 4. Otherwise niri creates the user file from its default config, but only
//!    when the compositor starts. `niri validate` never creates it.
//!
//! Without a home directory niri loads `/etc/niri/config.kdl` and nothing else.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use thiserror::Error;

pub const SYSTEM_CONFIG: &str = "/etc/niri/config.kdl";

/// Where the config is, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// Set with `--config` or `$NIRI_CONFIG`, and the file exists.
    Explicit(PathBuf),
    /// Set with `--config` or `$NIRI_CONFIG`, but the file does not exist.
    /// niri does not load a config in this case either.
    ExplicitMissing(PathBuf),
    /// The user's own file.
    User(PathBuf),
    /// No user file, so niri uses the system-wide file. It belongs to the
    /// system; a change needs a personal copy at `user` first.
    System { system: PathBuf, user: PathBuf },
    /// Neither file exists. niri creates `user` the next time it starts.
    Missing { user: PathBuf, system: PathBuf },
}

impl Location {
    /// The file to open, if there is one.
    pub fn file(&self) -> Option<&Path> {
        match self {
            Self::Explicit(path) | Self::User(path) => Some(path),
            Self::System { system, .. } => Some(system),
            Self::ExplicitMissing(_) | Self::Missing { .. } => None,
        }
    }
}

impl Location {
    pub fn is_system(&self) -> bool {
        matches!(self, Self::System { .. })
    }

    /// Why there is no file to open, in plain words.
    pub fn missing_message(&self) -> Option<String> {
        match self {
            Self::ExplicitMissing(path) => Some(format!(
                "{} does not exist. It was set with --config or NIRI_CONFIG, and niri does not load a config from it either.",
                path.display()
            )),
            Self::Missing { user, .. } => Some(format!(
                "niri has no config yet. niri creates {} from its default config the next time it starts.",
                user.display()
            )),
            _ => None,
        }
    }
}

/// Why the config could not be opened.
#[derive(Debug, Error)]
pub enum OpenError {
    /// There is no file to open.
    #[error("{0}")]
    Missing(String),
    /// The file exists, but the session could not be opened.
    #[error(transparent)]
    Session(#[from] crate::session::SessionError),
}

/// Open the config at a location. The system-wide file opens read-only.
pub fn open(
    location: &Location,
    options: crate::session::SessionOptions,
) -> Result<crate::session::Session, OpenError> {
    let Some(file) = location.file() else {
        return Err(OpenError::Missing(
            location.missing_message().unwrap_or_default(),
        ));
    };
    Ok(crate::session::Session::open_with(
        file,
        crate::session::SessionOptions {
            system: location.is_system(),
            ..options
        },
    )?)
}

/// Find the config from the real environment.
pub fn locate(cli: Option<PathBuf>) -> Location {
    locate_with(
        cli,
        std::env::var_os("NIRI_CONFIG"),
        user_config_path(
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        ),
        PathBuf::from(SYSTEM_CONFIG),
        |path| path.exists(),
    )
}

/// The rules, without touching the environment or the disk.
pub fn locate_with(
    cli: Option<PathBuf>,
    niri_config: Option<OsString>,
    user: Option<PathBuf>,
    system: PathBuf,
    exists: impl Fn(&Path) -> bool,
) -> Location {
    let explicit = cli.or_else(|| {
        niri_config
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    });
    let explicit = explicit.or_else(|| user.is_none().then(|| system.clone()));
    if let Some(path) = explicit {
        return if exists(&path) {
            Location::Explicit(path)
        } else {
            Location::ExplicitMissing(path)
        };
    }
    let user = user.expect("checked above");
    if exists(&user) {
        Location::User(user)
    } else if exists(&system) {
        Location::System { system, user }
    } else {
        Location::Missing { user, system }
    }
}

/// The user config path, as the `directories` crate gives it to niri:
/// `$XDG_CONFIG_HOME` when it is an absolute path, else `$HOME/.config`.
pub fn user_config_path(
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    let base = xdg_config_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("niri").join("config.kdl"))
}

/// Copy the system-wide config to the user path, so it can be changed.
///
/// niri prefers the user file from then on. Relative includes in the copied
/// file pointed next to `/etc/niri/config.kdl`; they are rewritten to
/// absolute paths so they still find the same files. With `validate`, niri
/// checks the copy first. An existing user file is never replaced.
pub fn make_personal_copy(system: &Path, user: &Path, validate: bool) -> Result<(), String> {
    let tree = crate::doc::load(system)
        .map_err(|error| format!("cannot read {}: {error}", system.display()))?;
    let main = tree
        .files
        .iter()
        .position(|file| file.is_main)
        .ok_or_else(|| "the system config did not load".to_string())?;
    let mut text = tree.files[main].text.clone();
    let mut includes: Vec<_> = tree
        .includes
        .iter()
        .filter(|include| include.file == main)
        .filter(|include| {
            let written = text[include.path_span.clone()].trim_matches('"');
            !Path::new(written).is_absolute()
        })
        .collect();
    // Replace from the end so earlier spans stay valid.
    includes.sort_by_key(|include| std::cmp::Reverse(include.path_span.start));
    for include in includes {
        let absolute = include.path.to_string_lossy();
        let quoted = format!(
            "\"{}\"",
            absolute.replace('\\', "\\\\").replace('"', "\\\"")
        );
        text.replace_range(include.path_span.clone(), &quoted);
    }

    if user.exists() {
        return Err(format!(
            "{} already exists. It was left unchanged.",
            user.display()
        ));
    }
    if let Some(parent) = user.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    if validate {
        crate::session::validate_text(user, &text)
            .map_err(|error| format!("niri did not accept the copy: {error}"))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(user)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => {
                format!("{} already exists. It was left unchanged.", user.display())
            }
            _ => format!("cannot create {}: {error}", user.display()),
        })?;
    use std::io::Write;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", user.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "/home/u/.config/niri/config.kdl";

    fn find(cli: Option<&str>, env: Option<&str>, existing: &[&str]) -> Location {
        locate_with(
            cli.map(PathBuf::from),
            env.map(OsString::from),
            Some(PathBuf::from(USER)),
            PathBuf::from(SYSTEM_CONFIG),
            |path| existing.iter().any(|existing| Path::new(existing) == path),
        )
    }

    #[test]
    fn the_user_file_comes_first() {
        let found = find(None, None, &[USER, SYSTEM_CONFIG]);
        assert_eq!(found, Location::User(USER.into()));
    }

    #[test]
    fn the_system_file_is_the_fallback() {
        let found = find(None, None, &[SYSTEM_CONFIG]);
        assert_eq!(
            found,
            Location::System {
                system: SYSTEM_CONFIG.into(),
                user: USER.into()
            }
        );
        assert_eq!(found.file(), Some(Path::new(SYSTEM_CONFIG)));
    }

    #[test]
    fn no_file_at_all_is_missing_not_an_error() {
        let found = find(None, None, &[]);
        assert!(matches!(found, Location::Missing { .. }));
        assert_eq!(found.file(), None);
    }

    #[test]
    fn the_flag_wins_over_the_variable() {
        let found = find(Some("/a.kdl"), Some("/b.kdl"), &["/a.kdl", "/b.kdl"]);
        assert_eq!(found, Location::Explicit("/a.kdl".into()));
    }

    #[test]
    fn the_variable_wins_over_the_default_locations() {
        let found = find(None, Some("/b.kdl"), &["/b.kdl", USER]);
        assert_eq!(found, Location::Explicit("/b.kdl".into()));
    }

    #[test]
    fn an_empty_variable_is_ignored() {
        let found = find(None, Some(""), &[USER]);
        assert_eq!(found, Location::User(USER.into()));
    }

    #[test]
    fn an_explicit_file_is_never_replaced_by_a_fallback() {
        let found = find(None, Some("/gone.kdl"), &[USER, SYSTEM_CONFIG]);
        assert_eq!(found, Location::ExplicitMissing("/gone.kdl".into()));
    }

    #[test]
    fn without_a_home_only_the_system_file_counts() {
        let found = locate_with(None, None, None, PathBuf::from(SYSTEM_CONFIG), |_| true);
        assert_eq!(found, Location::Explicit(SYSTEM_CONFIG.into()));
    }

    #[test]
    fn the_user_path_follows_xdg_then_home() {
        let path = |xdg: Option<&str>, home: Option<&str>| {
            user_config_path(xdg.map(OsString::from), home.map(OsString::from))
        };
        assert_eq!(
            path(Some("/x"), Some("/home/u")),
            Some(PathBuf::from("/x/niri/config.kdl"))
        );
        assert_eq!(
            path(None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/niri/config.kdl"))
        );
        // A relative XDG_CONFIG_HOME is invalid and ignored.
        assert_eq!(
            path(Some("rel"), Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/niri/config.kdl"))
        );
        assert_eq!(path(None, None), None);
    }

    #[test]
    fn a_personal_copy_keeps_relative_includes_working() {
        let dir = tempfile::tempdir().unwrap();
        let etc = dir.path().join("etc/niri");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(
            etc.join("config.kdl"),
            "// system\ninclude \"extra.kdl\"\nlayout {\n    gaps 8\n}\n",
        )
        .unwrap();
        std::fs::write(etc.join("extra.kdl"), "prefer-no-csd\n").unwrap();
        let user = dir.path().join("home/.config/niri/config.kdl");

        make_personal_copy(&etc.join("config.kdl"), &user, false).unwrap();

        let copied = std::fs::read_to_string(&user).unwrap();
        let expected = format!(
            "// system\ninclude \"{}\"\nlayout {{\n    gaps 8\n}}\n",
            etc.join("extra.kdl").display()
        );
        assert_eq!(copied, expected);
        // The copy loads and still finds the included file.
        let tree = crate::doc::load(&user).unwrap();
        assert!(tree.includes.iter().all(|include| include.exists));
    }

    #[test]
    fn a_personal_copy_never_replaces_a_user_file() {
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("system.kdl");
        std::fs::write(&system, "layout {\n    gaps 8\n}\n").unwrap();
        let user = dir.path().join("user/config.kdl");
        std::fs::create_dir_all(user.parent().unwrap()).unwrap();
        std::fs::write(&user, "mine\n").unwrap();

        let error = make_personal_copy(&system, &user, false).unwrap_err();

        assert!(error.contains("already exists"), "{error}");
        assert_eq!(std::fs::read_to_string(&user).unwrap(), "mine\n");
    }

    #[test]
    fn a_missing_location_reports_why() {
        let location = Location::ExplicitMissing(PathBuf::from("/gone.kdl"));
        let error = open(&location, crate::session::SessionOptions::default())
            .err()
            .expect("open should fail");
        assert!(
            matches!(error, OpenError::Missing(ref message) if message.contains("does not exist")),
            "{error}"
        );
    }

    #[test]
    fn a_personal_copy_keeps_absolute_includes() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other.kdl");
        std::fs::write(&other, "prefer-no-csd\n").unwrap();
        let system = dir.path().join("system.kdl");
        let text = format!("include \"{}\"\n", other.display());
        std::fs::write(&system, &text).unwrap();
        let user = dir.path().join("user/config.kdl");

        make_personal_copy(&system, &user, false).unwrap();

        assert_eq!(std::fs::read_to_string(&user).unwrap(), text);
    }
}
