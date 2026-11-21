//! Top bar sections. Each section groups a few pages; the top bar picks a
//! section and a row of buttons under it picks the page.

use niri_settings_core::schema::Page;

pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub pages: &'static [Page],
}

pub const SECTIONS: &[Section] = &[
    Section {
        id: "home",
        title: "Home",
        icon: "go-home-symbolic",
        pages: &[Page::Home],
    },
    Section {
        id: "look",
        title: "Style",
        icon: "preferences-desktop-appearance-symbolic",
        pages: &[Page::Appearance, Page::Layout, Page::Animations],
    },
    Section {
        id: "input",
        title: "Input",
        icon: "input-keyboard-symbolic",
        pages: &[Page::Keyboard, Page::Shortcuts, Page::Mouse],
    },
    Section {
        id: "system",
        title: "System",
        icon: "computer-symbolic",
        pages: &[Page::Displays, Page::Workspaces, Page::Apps, Page::Startup],
    },
    Section {
        id: "advanced",
        title: "Advanced",
        icon: "emblem-system-symbolic",
        pages: &[Page::Health, Page::All, Page::Config],
    },
];

/// The section that holds a page.
pub fn section_of(page: Page) -> &'static Section {
    SECTIONS
        .iter()
        .find(|section| section.pages.contains(&page))
        .unwrap_or(&SECTIONS[0])
}

pub fn section_by_id(id: &str) -> Option<&'static Section> {
    SECTIONS.iter().find(|section| section.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niri_settings_core::schema::PAGES;

    #[test]
    fn every_page_is_in_exactly_one_section() {
        for info in PAGES {
            let count = SECTIONS
                .iter()
                .filter(|section| section.pages.contains(&info.page))
                .count();
            assert_eq!(count, 1, "{} is in {count} sections", info.id);
        }
        let total: usize = SECTIONS.iter().map(|section| section.pages.len()).sum();
        assert_eq!(total, PAGES.len());
    }

    #[test]
    fn section_ids_are_unique() {
        for section in SECTIONS {
            assert_eq!(
                section_by_id(section.id).map(|s| s.title),
                Some(section.title)
            );
        }
    }

    #[test]
    fn the_top_bar_stays_small() {
        // A view switcher works for a handful of sections, not more.
        assert!(SECTIONS.len() <= 5);
    }
}
