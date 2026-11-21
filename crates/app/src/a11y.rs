//! A small accessibility audit for icon-only controls.
//!
//! GTK and AT-SPI do the real work. This check catches the common regression:
//! a new icon button with no tooltip and no accessible name.

use adw::prelude::*;

pub fn audit(root: &impl IsA<gtk::Widget>) -> Vec<String> {
    let mut issues = Vec::new();
    walk(root.as_ref(), &mut issues);
    issues
}

fn walk(widget: &gtk::Widget, issues: &mut Vec<String>) {
    // Hidden widgets are not exposed to screen readers.
    if !widget.is_visible() {
        return;
    }
    if let Some(button) = widget.downcast_ref::<gtk::Button>()
        && button.label().is_none()
        && button.icon_name().is_some()
        && button.tooltip_text().is_none()
        && !matches!(
            button.icon_name().as_deref(),
            Some(
                "go-previous-symbolic"
                    | "go-next-symbolic"
                    | "value-decrease-symbolic"
                    | "value-increase-symbolic"
            )
        )
    {
        issues.push(format!(
            "icon-only button has no accessible label: {}",
            button.icon_name().unwrap_or_default()
        ));
    }
    // A switch row names its switch with the row title.
    if let Some(switch) = widget.downcast_ref::<gtk::Switch>()
        && switch.tooltip_text().is_none()
        && switch.ancestor(adw::SwitchRow::static_type()).is_none()
    {
        issues.push("switch has no accessible label".to_string());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        walk(&current, issues);
        child = current.next_sibling();
    }
}

pub fn print_report(root: &impl IsA<gtk::Widget>) {
    let issues = audit(root);
    if issues.is_empty() {
        eprintln!("a11y audit: no issues");
    } else {
        for issue in issues {
            eprintln!("a11y issue: {issue}");
        }
    }
}
