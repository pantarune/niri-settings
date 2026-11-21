//! Command line interface for niri settings.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use niri_settings_core::Location;
use niri_settings_core::lint::{self, LintContext, Severity};
use niri_settings_core::schema::{self, Control};
use niri_settings_core::session::{Session, SessionOptions};
use niri_settings_core::{Value, import, niri};

#[derive(Debug, Parser)]
#[command(
    name = "niri-settings-cli",
    version,
    about = "Safe settings for the niri compositor",
    long_about = "Read, change and check the niri config without breaking it.\n\
                  With no command, the graphical app opens."
)]
struct Cli {
    /// Path to the config file.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,

    /// Open the app on a page.
    #[arg(long, global = true, value_name = "PAGE")]
    page: Option<String>,

    /// Revert an unconfirmed risky change.
    #[arg(long, global = true)]
    recover: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print a value and where it comes from.
    Get { option: String },
    /// Apply a change. The file is validated and can be undone.
    Set { option: String, value: String },
    /// Remove a value, so that niri's default applies again.
    Reset { option: String },
    /// Undo the last change.
    Undo,
    /// Redo the last undone change.
    Redo,
    /// List changes.
    History {
        /// Restore this history entry.
        #[arg(long, value_name = "ID")]
        restore: Option<u64>,
    },
    /// Run the setup checks. Exit code 1 means problems were found.
    Check,
    /// Import shortcuts and looks from Hyprland or Sway.
    Import {
        file: PathBuf,
        /// Show the result without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Explain which window rules match a window.
    Why {
        #[command(subcommand)]
        what: WhyCommand,
    },
    /// Print every catalogue option as JSON.
    Options,
    /// Validate the config with niri.
    Validate,
}

#[derive(Debug, Subcommand)]
enum WhyCommand {
    /// Pick a window with the mouse and explain its rules.
    Window,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let location = niri_settings_core::locate(cli.config.clone());

    if cli.recover {
        match recover(&location) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    let Some(command) = cli.command else {
        // The app finds the config the same way; pass only an explicit path.
        return open_app(cli.config.as_deref(), cli.page.as_deref());
    };

    match run(command, &location, cli.json) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn open_app(config: Option<&Path>, page: Option<&str>) -> ExitCode {
    let mut command = std::process::Command::new("niri-settings-app");
    if let Some(config) = config {
        command.arg("--config").arg(config);
    }
    if let Some(page) = page {
        command.arg("--page").arg(page);
    }
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(error) => {
            eprintln!("Cannot start the graphical app: {error}");
            eprintln!(
                "Install niri-settings-app, or run a command such as `niri-settings-cli check`."
            );
            ExitCode::FAILURE
        }
    }
}

fn open(location: &Location) -> Result<Session, String> {
    let session = niri_settings_core::location::open(location, SessionOptions::default())
        .map_err(|error| error.to_string())?;
    if let Some(recovery) = &session.recovery {
        eprintln!("{}", recovery.message());
    }
    Ok(session)
}

fn run(command: Command, config: &Location, json: bool) -> Result<ExitCode, String> {
    match command {
        Command::Get { option } => {
            let session = open(config)?;
            let resolved = session
                .get(&option)
                .ok_or_else(|| format!("{option} is not set and has no catalog entry"))?;
            let source = match &resolved.source {
                niri_settings_core::Source::Default => "default".to_string(),
                niri_settings_core::Source::File(path) => path.display().to_string(),
                niri_settings_core::Source::Managed(path, name) => {
                    format!("{} ({name})", path.display())
                }
            };
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "option": option,
                        "value": resolved.value,
                        "source": source,
                    })
                );
            } else {
                println!("{}  ({source})", resolved.value);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Set { option, value } => {
            let info = schema::option(&option).ok_or_else(|| {
                format!("{option} is not in the catalog. For raw paths, use the All options page in the app.")
            })?;
            let parsed = parse_value(info, &value)?;
            let mut session = open(config)?;
            match session.set(&option, parsed) {
                Ok(applied) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "ok": true,
                                "summary": applied.summary,
                                "files": applied.files,
                                "history": applied.entry,
                            })
                        );
                    } else {
                        println!("{}", applied.summary);
                    }
                    Ok(ExitCode::SUCCESS)
                }
                Err(niri_settings_core::ApplyError::NoChange) => {
                    println!("Nothing to change.");
                    Ok(ExitCode::SUCCESS)
                }
                Err(error) => Err(format!("{error}")),
            }
        }
        Command::Reset { option } => {
            let mut session = open(config)?;
            let applied = session.reset(&option).map_err(|error| error.to_string())?;
            println!("{}", applied.summary);
            Ok(ExitCode::SUCCESS)
        }
        Command::Undo => {
            let mut session = open(config)?;
            let applied = session.undo().map_err(|error| error.to_string())?;
            println!("{}", applied.summary);
            Ok(ExitCode::SUCCESS)
        }
        Command::Redo => {
            let mut session = open(config)?;
            let applied = session.redo().map_err(|error| error.to_string())?;
            println!("{}", applied.summary);
            Ok(ExitCode::SUCCESS)
        }
        Command::History { restore } => {
            let mut session = open(config)?;
            if let Some(id) = restore {
                let applied = session.restore(id).map_err(|error| error.to_string())?;
                println!("{}", applied.summary);
                return Ok(ExitCode::SUCCESS);
            }
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(session.history()).unwrap()
                );
            } else {
                for entry in session.history().iter().rev() {
                    println!(
                        "{:>6}  {:>10}  {}",
                        entry.id,
                        format_time(entry.time),
                        entry.summary
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Check => {
            let session = open(config)?;
            let context = LintContext::from_system();
            let problems = lint::check(&session.tree, &context);
            if json {
                let values: Vec<_> = problems
                    .iter()
                    .map(|problem| {
                        serde_json::json!({
                            "id": problem.id,
                            "severity": problem.severity,
                            "title": problem.title,
                            "detail": problem.detail,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&values).unwrap());
            } else if problems.is_empty() {
                println!("No problems found.");
            } else {
                for problem in &problems {
                    let mark = match problem.severity {
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                        Severity::Info => "note",
                    };
                    println!("{mark}: {}", problem.title);
                    println!("       {}", problem.detail);
                }
            }
            if problems.iter().any(|problem| {
                problem.severity == Severity::Error || problem.severity == Severity::Warning
            }) {
                Ok(ExitCode::FAILURE)
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Import { file, dry_run } => {
            let text = std::fs::read_to_string(&file)
                .map_err(|error| format!("cannot read {}: {error}", file.display()))?;
            let source = import::detect(&file, &text)
                .ok_or_else(|| "cannot tell whether this is Hyprland or Sway".to_string())?;
            let result = import::parse(&text, source);
            if json {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else if dry_run {
                println!("From {}:", source.label());
                for (key, value) in &result.changes {
                    println!("  {key} = {value}");
                }
                for bind in &result.binds {
                    println!("  {} → {}", bind.key, bind.action);
                }
                for program in &result.startup {
                    println!("  start {program}");
                }
                for rule in &result.window_rules {
                    println!("  rule: {}", rule.replace('\n', "; "));
                }
                for note in &result.notes {
                    println!("  note: {note}");
                }
            }
            if dry_run {
                return Ok(ExitCode::SUCCESS);
            }
            let mut session = open(config)?;
            match session.apply_import(&result) {
                Ok(applied) => println!("{}", applied.summary),
                Err(niri_settings_core::ApplyError::NoChange) => println!("Nothing to import."),
                Err(error) => return Err(format!("{error}")),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Why {
            what: WhyCommand::Window,
        } => {
            let session = open(config)?;
            let window = niri::pick_window()
                .ok_or_else(|| "no window was picked, or niri is not running".to_string())?;
            let app_id = window.app_id.unwrap_or_default();
            let title = window.title.unwrap_or_default();
            println!("Window: {app_id} — {title}");
            let mut matched = 0;
            for (index, node) in session.tree.top_level("window-rule").iter().enumerate() {
                let text = &session.tree.files[node.file].text[node.node_span.clone()];
                let mut matches = false;
                for line in text.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix("match app-id=")
                        && let Some(pattern) = quoted(rest)
                    {
                        matches |= regex_like(&app_id, pattern);
                    }
                    if let Some(rest) = line.strip_prefix("match title=")
                        && let Some(pattern) = quoted(rest)
                    {
                        matches &= regex_like(&title, pattern);
                    }
                }
                if matches {
                    matched += 1;
                    let summary = text
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with("match "))
                        .take(4)
                        .collect::<Vec<_>>()
                        .join(" · ");
                    println!("  rule {} matches: {summary}", index + 1);
                }
            }
            if matched == 0 {
                println!("No window rules match this window.");
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Options => {
            let options: Vec<_> = schema::OPTIONS
                .iter()
                .map(|option| {
                    serde_json::json!({
                        "key": option.key,
                        "label": option.label,
                        "help": option.help,
                        "page": option.page.info().id,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&options).unwrap());
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate => {
            let session = open(config)?;
            let problems = lint::check(&session.tree, &LintContext::default());
            if let Some(problem) = problems
                .iter()
                .find(|problem| problem.id == "invalid-config")
            {
                Err(problem.detail.clone())
            } else {
                println!("The config is valid.");
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}

fn quoted(text: &str) -> Option<&str> {
    let rest = text.trim();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn regex_like(value: &str, pattern: &str) -> bool {
    let pattern = pattern.trim_start_matches('^').trim_end_matches('$');
    let pattern = pattern.replace("\\\\", "\\");
    if pattern.starts_with('(') {
        return true;
    }
    value.contains(&pattern)
}

fn parse_value(option: &schema::OptionInfo, raw: &str) -> Result<Value, String> {
    match option.control {
        Control::Toggle { .. } => match raw {
            "true" | "on" | "yes" | "1" => Ok(Value::Bool(true)),
            "false" | "off" | "no" | "0" => Ok(Value::Bool(false)),
            _ => Err(format!("{raw} is not true or false")),
        },
        Control::Number { integer, .. } => {
            if integer {
                raw.parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| format!("{raw} is not a whole number"))
            } else {
                raw.parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| format!("{raw} is not a number"))
            }
        }
        Control::Color { .. } | Control::Text { .. } | Control::Choice { .. } => {
            Ok(Value::String(raw.to_string()))
        }
    }
}

fn format_time(seconds: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(seconds);
    let delta = now.saturating_sub(seconds);
    if delta < 60 {
        "now".to_string()
    } else if delta < 3600 {
        format!("{} min ago", delta / 60)
    } else if delta < 86_400 {
        format!("{} h ago", delta / 3600)
    } else {
        format!("{} d ago", delta / 86_400)
    }
}

/// Revert a pending risky change. The marker is written before a change that
/// can lock the user out, and removed when the user keeps it.
fn recover(config: &Location) -> Result<String, String> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    let output_marker = state.join("niri-settings/pending-output.json");
    if output_marker.exists() {
        let text = std::fs::read_to_string(&output_marker)
            .map_err(|error| format!("cannot read {}: {error}", output_marker.display()))?;
        let commands: Vec<Vec<String>> = serde_json::from_str(&text)
            .map_err(|error| format!("cannot read the display marker: {error}"))?;
        for command in &commands {
            if let Some((name, args)) = command.split_first() {
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                niri::output(name, &refs).map_err(|error| error.to_string())?;
            }
        }
        let _ = std::fs::remove_file(&output_marker);
        return Ok("Restored the display settings from the last session.".to_string());
    }

    let marker = state.join("niri-settings/pending-revert");
    if marker.exists() {
        let mut session = niri_settings_core::location::open(config, SessionOptions::default())
            .map_err(|error| error.to_string())?;
        if let Some(recovery) = &session.recovery {
            eprintln!("{}", recovery.message());
        }
        session.undo().map_err(|error| error.to_string())?;
        let _ = std::fs::remove_file(&marker);
        return Ok("Reverted the unconfirmed change.".to_string());
    }

    Ok("There is no pending change to revert.".to_string())
}
