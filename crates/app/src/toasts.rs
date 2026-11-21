//! Change notices that merge (design section 5.1).
//!
//! Each change used to add its own toast, and libadwaita queues toasts, so a
//! few quick changes left a line of notices that took half a minute to clear.
//! Now one notice is on screen at a time. Changes in a row merge into
//! "3 changes · Undo", and its button undoes all three. Undo and redo merge the
//! same way, with a button that reverses them.

/// What the notice on screen stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A message without a history step.
    Plain,
    /// This many changes in a row; the button undoes them.
    Changed(u32),
    /// This many undos in a row; the button redoes them.
    Undone(u32),
    /// This many redos in a row; the button undoes them.
    Redone(u32),
}

/// What just happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Plain,
    Changed,
    Undone,
    Redone,
}

/// The kind of the next notice, given the one still on screen.
pub fn next(showing: Option<Kind>, event: Event) -> Kind {
    match (showing, event) {
        (_, Event::Plain) => Kind::Plain,
        (Some(Kind::Changed(count)), Event::Changed) => Kind::Changed(count + 1),
        (Some(Kind::Undone(count)), Event::Undone) => Kind::Undone(count + 1),
        (Some(Kind::Redone(count)), Event::Redone) => Kind::Redone(count + 1),
        (_, Event::Changed) => Kind::Changed(1),
        (_, Event::Undone) => Kind::Undone(1),
        (_, Event::Redone) => Kind::Redone(1),
    }
}

/// The notice text. A single step shows its own summary.
pub fn title(kind: Kind, latest: &str) -> String {
    let (count, pattern) = match kind {
        Kind::Plain => return latest.to_string(),
        Kind::Changed(count) => (count, crate::i18n::tr("{n} changes")),
        Kind::Undone(count) => (count, crate::i18n::tr("Undid {n} changes")),
        Kind::Redone(count) => (count, crate::i18n::tr("Redid {n} changes")),
    };
    if count <= 1 {
        latest.to_string()
    } else {
        pattern.replace("{n}", &count.to_string())
    }
}

/// What the notice button does: undo or redo, how many times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Undo(u32),
    Redo(u32),
}

pub fn button(kind: Kind) -> Option<Button> {
    match kind {
        Kind::Plain => None,
        Kind::Changed(count) | Kind::Redone(count) => Some(Button::Undo(count)),
        Kind::Undone(count) => Some(Button::Redo(count)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_in_a_row_merge() {
        let first = next(None, Event::Changed);
        let second = next(Some(first), Event::Changed);
        let third = next(Some(second), Event::Changed);
        assert_eq!(third, Kind::Changed(3));
        assert_eq!(title(third, "Gaps 16 → 20 px"), "3 changes");
        assert_eq!(button(third), Some(Button::Undo(3)));
    }

    #[test]
    fn one_change_shows_its_summary() {
        let kind = next(None, Event::Changed);
        assert_eq!(title(kind, "Gaps 16 → 20 px"), "Gaps 16 → 20 px");
        assert_eq!(button(kind), Some(Button::Undo(1)));
    }

    #[test]
    fn undos_merge_and_offer_redo() {
        let after_changes = Some(Kind::Changed(3));
        let first = next(after_changes, Event::Undone);
        assert_eq!(first, Kind::Undone(1));
        let second = next(Some(first), Event::Undone);
        assert_eq!(title(second, "Gaps 20 → 16 px"), "Undid 2 changes");
        assert_eq!(button(second), Some(Button::Redo(2)));
    }

    #[test]
    fn redos_offer_undo() {
        let kind = next(Some(Kind::Redone(1)), Event::Redone);
        assert_eq!(kind, Kind::Redone(2));
        assert_eq!(title(kind, "x"), "Redid 2 changes");
        assert_eq!(button(kind), Some(Button::Undo(2)));
    }

    #[test]
    fn a_new_kind_starts_counting_again() {
        assert_eq!(
            next(Some(Kind::Undone(4)), Event::Changed),
            Kind::Changed(1)
        );
        assert_eq!(next(Some(Kind::Changed(4)), Event::Plain), Kind::Plain);
        assert_eq!(next(Some(Kind::Plain), Event::Changed), Kind::Changed(1));
    }

    #[test]
    fn a_plain_notice_has_no_button() {
        assert_eq!(button(Kind::Plain), None);
        assert_eq!(title(Kind::Plain, "Copied"), "Copied");
    }
}
