use std::path::PathBuf;
use std::process::Command;

fn find_command(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

fn command_exists(name: &str) -> bool {
    find_command(name).is_some()
}

/// The session bus config that belongs to the dbus-daemon on PATH. Nix puts it
/// in the package prefix, while some systems only have `/etc/dbus-1`.
fn session_config() -> Option<PathBuf> {
    let daemon = std::fs::canonicalize(find_command("dbus-daemon")?).ok()?;
    let config = daemon.parent()?.parent()?.join("share/dbus-1/session.conf");
    config.is_file().then_some(config)
}

#[test]
fn set_round_trip_over_a_private_bus() {
    if !command_exists("dbus-run-session") || !command_exists("busctl") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.kdl");
    std::fs::write(&config, "layout {\n    gaps 10\n}\n").unwrap();
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_niri-settings-daemon"));
    let script = format!(
        "DBUS_SESSION_BUS_ADDRESS=${{DBUS_SESSION_BUS_ADDRESS%%;*}}; \
         export DBUS_SESSION_BUS_ADDRESS; \
         {} --config {} >/tmp/niri-settings-dbus-test.log 2>&1 & sleep 2; \
         busctl --user call io.github.niri_settings.NiriSettings1 /io/github/niri_settings/NiriSettings1 \
         io.github.niri_settings.NiriSettings1 Set ss layout.gaps 20",
        binary.display(),
        config.display()
    );
    let output = {
        let mut session = Command::new("dbus-run-session");
        if let Some(config) = session_config() {
            session.arg("--config-file").arg(config);
        }
        session
            .arg("--")
            .arg("bash")
            .arg("-c")
            .arg(script)
            .output()
            .unwrap()
    };
    assert!(
        output.status.success(),
        "dbus call failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("20"));
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .contains("gaps 20")
    );
}
