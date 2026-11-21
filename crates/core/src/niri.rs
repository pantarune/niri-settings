//! The small part of niri that the app reads: outputs, windows, workspaces and
//! temporary output changes. All calls go through `niri msg --json`.

use std::collections::HashMap;
use std::process::Command;

use niri_ipc::{Output, Window, Workspace};

/// Read connected outputs. An empty list means niri is not running.
pub fn outputs() -> Vec<Output> {
    let Some(value) = run_json(&["outputs"]) else {
        return Vec::new();
    };
    let map: HashMap<String, Output> = serde_json::from_value(value).unwrap_or_default();
    let mut out: Vec<Output> = map.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Read open windows.
pub fn windows() -> Vec<Window> {
    let Some(value) = run_json(&["windows"]) else {
        return Vec::new();
    };
    serde_json::from_value(value).unwrap_or_default()
}

/// Read workspaces.
pub fn workspaces() -> Vec<Workspace> {
    let Some(value) = run_json(&["workspaces"]) else {
        return Vec::new();
    };
    serde_json::from_value(value).unwrap_or_default()
}

/// Ask niri to let the user click a window.
pub fn pick_window() -> Option<Window> {
    let value = run_json(&["pick-window"])?;
    serde_json::from_value(value).ok()
}

/// Run a niri action. Returns the message from niri on failure.
pub fn action(args: &[&str]) -> Result<(), String> {
    let output = Command::new("niri")
        .arg("msg")
        .arg("action")
        .args(args)
        .output()
        .map_err(|error| format!("cannot run niri: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Change an output for this session only. The file is not touched.
pub fn output(name: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new("niri")
        .arg("msg")
        .arg("output")
        .arg(name)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run niri: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Reload the config after a rename. niri watches the file on its own, but a
/// rename can replace the inode on some watches, so this is a cheap safety net.
pub fn reload_config() {
    let _ = action(&["load-config-file"]);
}

fn run_json(args: &[&str]) -> Option<serde_json::Value> {
    let output = Command::new("niri")
        .arg("msg")
        .arg("--json")
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

/// The running niri version, when available.
pub fn version() -> Option<String> {
    let output = Command::new("niri")
        .arg("msg")
        .arg("version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A human label for an output.
pub fn output_label(output: &Output) -> String {
    let make = output.make.trim();
    let model = output.model.trim();
    match (make.is_empty(), model.is_empty()) {
        (false, false) => format!("{make} {model}"),
        (true, false) => model.to_string(),
        (false, true) => make.to_string(),
        (true, true) => output.name.clone(),
    }
}

/// The current mode as width × height.
pub fn current_mode(output: &Output) -> Option<(u16, u16, u32)> {
    let index = output.current_mode?;
    let mode = output.modes.get(index)?;
    Some((mode.width, mode.height, mode.refresh_rate))
}
