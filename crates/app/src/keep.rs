//! Keep the scroll position and keyboard focus when a page is rebuilt.
//!
//! Pages are rebuilt after a change, an undo or a reset. Without this, focus
//! jumps to the top of the window and the page scrolls back up, which makes
//! keyboard and screen-reader use hard (design section 11).
//!
//! Controls that should keep focus carry a widget name. Option controls use
//! `opt:<key>`, with `#<part>` for a second focusable part, and `reset:<key>` /
//! `help:<key>` for their buttons. Other rows are found again by title.

use adw::prelude::*;

/// Where the focus was before a rebuild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// A widget with a `opt:`/`reset:`/`help:` name.
    Named(String),
    /// A widget inside a preferences row with this title.
    Row(String),
}

/// Remember the focused widget inside `page`, if any.
pub fn focus_mark(window: &impl IsA<gtk::Window>, page: &gtk::Widget) -> Option<Mark> {
    let focus = gtk::prelude::GtkWindowExt::focus(window.as_ref())?;
    if !focus.is_ancestor(page) {
        return None;
    }
    let mut current = Some(focus);
    let mut row_title = None;
    while let Some(widget) = current {
        if widget == *page {
            break;
        }
        let name = widget.widget_name();
        if is_mark_name(&name) {
            return Some(Mark::Named(name.to_string()));
        }
        if row_title.is_none()
            && let Some(row) = widget.downcast_ref::<adw::PreferencesRow>()
        {
            let title = row.title();
            if !title.is_empty() {
                row_title = Some(title.to_string());
            }
        }
        current = widget.parent();
    }
    row_title.map(Mark::Row)
}

/// Put the focus back on the widget that matches `mark`, or the closest one.
pub fn restore_focus(page: &gtk::Widget, mark: &Mark) -> bool {
    let target = match mark {
        Mark::Named(name) => fallback_names(name)
            .iter()
            .find_map(|name| find(page, &|widget| widget.widget_name() == name.as_str())),
        Mark::Row(title) => find(page, &|widget| {
            widget
                .downcast_ref::<adw::PreferencesRow>()
                .is_some_and(|row| row.title() == title.as_str())
        }),
    };
    let Some(target) = target else {
        return false;
    };
    target.grab_focus() || target.child_focus(gtk::DirectionType::TabForward)
}

/// Set the vertical scroll position once the new content is tall enough.
pub fn restore_scroll(scrolled: &gtk::ScrolledWindow, value: f64) {
    let adjustment = scrolled.vadjustment();
    if value <= 0.0 {
        return;
    }
    if adjustment.upper() - adjustment.page_size() >= value {
        adjustment.set_value(value);
        return;
    }
    // The new page has no size yet. Wait for its first layout.
    let handler = std::rc::Rc::new(std::cell::RefCell::new(None));
    let handler_inner = handler.clone();
    let id = adjustment.connect_changed(move |adjustment| {
        if adjustment.upper() <= adjustment.page_size() {
            return;
        }
        adjustment.set_value(value.min(adjustment.upper() - adjustment.page_size()));
        if let Some(id) = handler_inner.borrow_mut().take() {
            adjustment.disconnect(id);
        }
    });
    *handler.borrow_mut() = Some(id);
}

/// The names to try, most exact first. A reset button disappears once the
/// value is back to the default, so its control is the next best place.
pub fn fallback_names(name: &str) -> Vec<String> {
    let mut names = vec![name.to_string()];
    let base = name.split('#').next().unwrap_or(name);
    if base != name {
        names.push(base.to_string());
    }
    for prefix in ["reset:", "help:"] {
        if let Some(key) = base.strip_prefix(prefix) {
            names.push(format!("opt:{key}"));
        }
    }
    names
}

fn is_mark_name(name: &str) -> bool {
    ["opt:", "reset:", "help:"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn find(widget: &gtk::Widget, matches: &dyn Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if matches(widget) {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find(&current, matches) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_control_name_is_tried_as_is() {
        assert_eq!(fallback_names("opt:layout.gaps"), ["opt:layout.gaps"]);
    }

    #[test]
    fn a_control_part_falls_back_to_the_control() {
        assert_eq!(
            fallback_names("opt:layout.gaps#spin"),
            ["opt:layout.gaps#spin", "opt:layout.gaps"]
        );
    }

    #[test]
    fn a_reset_button_falls_back_to_its_control() {
        assert_eq!(
            fallback_names("reset:layout.gaps"),
            ["reset:layout.gaps", "opt:layout.gaps"]
        );
        assert_eq!(
            fallback_names("help:cursor.xcursor-size"),
            ["help:cursor.xcursor-size", "opt:cursor.xcursor-size"]
        );
    }

    #[test]
    fn only_marked_names_count() {
        assert!(is_mark_name("opt:layout.gaps"));
        assert!(is_mark_name("reset:layout.gaps"));
        assert!(!is_mark_name("GtkBox"));
        assert!(!is_mark_name("niri-content"));
    }
}
