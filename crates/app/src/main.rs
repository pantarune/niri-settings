//! niri settings: a safe GTK4 settings app for the niri compositor.

mod a11y;
mod app;
mod arrange;
mod generated;
mod i18n;
mod jobs;
mod keep;
mod nav;
mod pages;
mod preview;
mod repair;
mod rows;
mod shot;
mod toasts;
mod util;
mod welcome;

use std::path::PathBuf;

use relm4::RelmApp;

fn main() {
    let mut config: Option<PathBuf> = None;
    let mut page = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                if let Some(path) = args.next() {
                    config = Some(PathBuf::from(path));
                }
            }
            "--page" | "-p" => {
                page = args.next();
            }
            "--help" | "-h" => {
                println!("niri-settings-app [--config PATH] [--page PAGE]");
                println!();
                println!("Pages: home, appearance, layout, apps, keyboard, shortcuts,");
                println!("mouse, displays, workspaces, animations, startup, health, all, config");
                return;
            }
            "--version" | "-V" => {
                println!("niri-settings-app {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    if let Some(id) = page.as_deref()
        && niri_settings_core::Page::from_id(id).is_none()
    {
        eprintln!("Unknown page: {id}");
        eprintln!("Pages: home, appearance, layout, apps, keyboard, shortcuts, mouse,");
        eprintln!("displays, workspaces, animations, startup, health, all, config");
        std::process::exit(2);
    }
    // Find the config exactly as niri does (core `location` module).
    let location = niri_settings_core::locate(config);
    let Some(file) = location.file().map(PathBuf::from) else {
        let message = location.missing_message().unwrap_or_default();
        std::process::exit(repair::run_missing(&message));
    };
    let session = match niri_settings_core::location::open(
        &location,
        niri_settings_core::SessionOptions::default(),
    ) {
        Ok(session) => session,
        Err(niri_settings_core::OpenError::Session(
            niri_settings_core::SessionError::NiriMissing,
        )) => {
            let code = repair::run_notice(
                "niri is not on PATH",
                "dialog-warning-symbolic",
                "niri settings checks every change with niri validate. Add niri to PATH, then start the app again. The config was not changed.",
            );
            std::process::exit(code);
        }
        Err(error) => {
            let code = repair::run(&file, &error.to_string());
            std::process::exit(code);
        }
    };
    let program = std::env::args()
        .next()
        .unwrap_or_else(|| "niri-settings-app".to_string());
    // The check modes start their own instance instead of handing over to
    // a window that is already open.
    let checking = std::env::var_os("NIRI_SETTINGS_A11Y").is_some()
        || std::env::var_os("NIRI_SETTINGS_SCREENSHOT").is_some()
        || std::env::var_os("NIRI_SETTINGS_MEASURE").is_some()
        || std::env::var_os("NIRI_SETTINGS_WELCOME_STEP").is_some();
    let flags = if checking {
        gtk::gio::ApplicationFlags::NON_UNIQUE
    } else {
        gtk::gio::ApplicationFlags::empty()
    };
    let application = adw::Application::builder()
        .application_id(app::APP_ID)
        .flags(flags)
        .build();
    RelmApp::from_app(application)
        .with_args(vec![program])
        .run::<app::AppModel>(app::Init {
            session,
            page,
            location,
        });
}
