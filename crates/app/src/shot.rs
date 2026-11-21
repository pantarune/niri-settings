//! Save the window as a PNG and quit, for checking the UI without a screen.
//!
//! `NIRI_SETTINGS_SCREENSHOT=out.png` saves the window once it has settled.
//! `NIRI_SETTINGS_SIZE=WIDTHxHEIGHT` sets the window size first. With GTK's
//! Broadway backend nothing appears on the desktop; `scripts/screenshot.sh`
//! sets that up.

use adw::prelude::*;

pub fn schedule(root: &adw::ApplicationWindow) {
    let Some(out) = std::env::var_os("NIRI_SETTINGS_SCREENSHOT") else {
        return;
    };
    if let Some((width, height)) = std::env::var("NIRI_SETTINGS_SIZE")
        .ok()
        .and_then(|size| parse_size(&size))
    {
        root.set_default_size(width, height);
    }
    let root = root.clone();
    // Wait for layout and the first background checks.
    gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(2500), move || {
        let code = match save(&root, std::path::Path::new(&out)) {
            Ok(()) => {
                eprintln!(
                    "screenshot: {} ({}x{})",
                    std::path::Path::new(&out).display(),
                    root.width(),
                    root.height()
                );
                0
            }
            Err(error) => {
                eprintln!("screenshot failed: {error}");
                1
            }
        };
        std::process::exit(code);
    });
}

fn save(root: &adw::ApplicationWindow, out: &std::path::Path) -> Result<(), String> {
    let paintable = gtk::WidgetPaintable::new(Some(root));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, root.width() as f64, root.height() as f64);
    let node = snapshot.to_node().ok_or("the window drew nothing")?;
    let renderer = root
        .native()
        .and_then(|native| native.renderer())
        .ok_or("the window has no renderer")?;
    renderer
        .render_texture(&node, None)
        .save_to_png(out)
        .map_err(|error| error.to_string())
}

fn parse_size(size: &str) -> Option<(i32, i32)> {
    let (width, height) = size.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

#[cfg(test)]
mod tests {
    #[test]
    fn sizes_parse() {
        assert_eq!(super::parse_size("360x640"), Some((360, 640)));
        assert_eq!(super::parse_size("360"), None);
        assert_eq!(super::parse_size("ax1"), None);
    }
}
