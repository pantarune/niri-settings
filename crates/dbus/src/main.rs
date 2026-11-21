//! D-Bus service `io.github.niri_settings.NiriSettings1`.
//!
//! Desktop shells can read and change settings through the same core as the
//! app. The service uses one `Session`, so writes stay validated and undoable.

use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};

use niri_settings_core::schema::{self, OPTIONS};
use niri_settings_core::session::{Session, SessionOptions};

const PATH: &str = "/io/github/niri_settings/NiriSettings1";

struct SettingsService {
    session: Session,
    changes: Sender<String>,
}

#[zbus::interface(name = "io.github.niri_settings.NiriSettings1")]
impl SettingsService {
    /// Read one option and where it comes from.
    fn get(&self, option: String) -> zbus::fdo::Result<String> {
        let resolved = self
            .session
            .get(&option)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("unknown option: {option}")))?;
        let source = match &resolved.source {
            niri_settings_core::Source::Default => "default".to_string(),
            niri_settings_core::Source::File(path) => path.display().to_string(),
            niri_settings_core::Source::Managed(path, name) => {
                format!("{} ({name})", path.display())
            }
        };
        Ok(format!("{} ({source})", resolved.value))
    }

    /// Change one option. The change is validated and recorded in history.
    fn set(&mut self, option: String, value: String) -> zbus::fdo::Result<String> {
        let info = schema::option(&option)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("unknown option: {option}")))?;
        let value = schema::parse_value(info, &value).map_err(zbus::fdo::Error::InvalidArgs)?;
        let applied = self
            .session
            .set(&option, value)
            .map_err(|error| zbus::fdo::Error::Failed(format!("{error}")))?;
        let _ = self.changes.send(applied.summary.clone());
        Ok(applied.summary)
    }

    /// Undo the last change.
    fn undo(&mut self) -> zbus::fdo::Result<String> {
        let applied = self
            .session
            .undo()
            .map_err(|error| zbus::fdo::Error::Failed(format!("{error}")))?;
        let _ = self.changes.send(applied.summary.clone());
        Ok(applied.summary)
    }

    /// Redo the last undone change.
    fn redo(&mut self) -> zbus::fdo::Result<String> {
        let applied = self
            .session
            .redo()
            .map_err(|error| zbus::fdo::Error::Failed(format!("{error}")))?;
        let _ = self.changes.send(applied.summary.clone());
        Ok(applied.summary)
    }

    /// Every catalogue option as JSON.
    fn list_options(&self) -> String {
        let values: Vec<_> = OPTIONS
            .iter()
            .map(|option| {
                let resolved = self.session.get_option(option);
                serde_json::json!({
                    "key": option.key,
                    "label": option.label,
                    "help": option.help,
                    "value": resolved.value,
                    "source": match &resolved.source {
                        niri_settings_core::Source::Default => "default".to_string(),
                        niri_settings_core::Source::File(path) => path.display().to_string(),
                        niri_settings_core::Source::Managed(path, name) => {
                            format!("{} ({name})", path.display())
                        }
                    },
                })
            })
            .collect();
        serde_json::to_string_pretty(&values).unwrap_or_else(|_| "[]".to_string())
    }

    /// Emitted after a change.
    #[zbus(signal)]
    async fn changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        summary: String,
    ) -> zbus::Result<()>;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let location = niri_settings_core::locate(parse_config());
    let session = niri_settings_core::location::open(&location, SessionOptions::default())?;
    if let Some(recovery) = &session.recovery {
        eprintln!("{}", recovery.message());
    }
    let (sender, receiver) = channel::<String>();
    let service = SettingsService {
        session,
        changes: sender,
    };

    let connection = zbus::blocking::connection::Builder::session()?
        .name("io.github.niri_settings.NiriSettings1")?
        .serve_at(PATH, service)?
        .build()?;

    println!("io.github.niri_settings.NiriSettings1 is ready on {PATH}");

    for summary in receiver {
        let iface = connection
            .object_server()
            .interface::<_, SettingsService>(PATH)?;
        let result = zbus::block_on(SettingsService::changed(iface.signal_emitter(), summary));
        if let Err(error) = result {
            eprintln!("cannot emit the Changed signal: {error}");
        }
    }
    Ok(())
}

fn parse_config() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if (arg == "--config" || arg == "-c")
            && let Some(path) = args.next()
        {
            return Some(PathBuf::from(path));
        }
    }
    None
}
