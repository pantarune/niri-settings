//! Built-in presets. A preset is data, not code: a list of option changes
//! applied as one undo step.

use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetGroup {
    Appearance,
    Animations,
}

#[derive(Debug, Clone, Copy)]
pub struct Preset {
    pub id: &'static str,
    pub group: PresetGroup,
    pub label: &'static str,
    pub detail: &'static str,
    pub changes: &'static [(&'static str, Value)],
}

/// Every preset, grouped by page.
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "appearance.compact",
        group: PresetGroup::Appearance,
        label: "Compact",
        detail: "Tight gaps and thin edges",
        changes: &[
            ("layout.gaps", Value::Float(8.0)),
            ("layout.focus-ring.width", Value::Float(2.0)),
            ("layout.border.width", Value::Float(2.0)),
        ],
    },
    Preset {
        id: "appearance.balanced",
        group: PresetGroup::Appearance,
        label: "Balanced",
        detail: "The niri defaults, slightly softer",
        changes: &[
            ("layout.gaps", Value::Float(16.0)),
            ("layout.focus-ring.width", Value::Float(4.0)),
            ("layout.border.width", Value::Float(4.0)),
        ],
    },
    Preset {
        id: "appearance.roomy",
        group: PresetGroup::Appearance,
        label: "Roomy",
        detail: "Wide gaps and strong edges",
        changes: &[
            ("layout.gaps", Value::Float(28.0)),
            ("layout.focus-ring.width", Value::Float(5.0)),
            ("layout.border.width", Value::Float(5.0)),
        ],
    },
    Preset {
        id: "animations.off",
        group: PresetGroup::Animations,
        label: "Off",
        detail: "No motion at all",
        changes: &[("animations.off", Value::Bool(false))],
    },
    Preset {
        id: "animations.snappy",
        group: PresetGroup::Animations,
        label: "Snappy",
        detail: "Fast and direct",
        changes: &[
            ("animations.off", Value::Bool(true)),
            ("animations.slowdown", Value::Float(0.7)),
        ],
    },
    Preset {
        id: "animations.smooth",
        group: PresetGroup::Animations,
        label: "Smooth",
        detail: "The niri defaults",
        changes: &[
            ("animations.off", Value::Bool(true)),
            ("animations.slowdown", Value::Float(1.0)),
        ],
    },
    Preset {
        id: "animations.playful",
        group: PresetGroup::Animations,
        label: "Playful",
        detail: "Slower, with more travel",
        changes: &[
            ("animations.off", Value::Bool(true)),
            ("animations.slowdown", Value::Float(1.6)),
        ],
    },
];

pub fn for_group(group: PresetGroup) -> impl Iterator<Item = &'static Preset> {
    PRESETS.iter().filter(move |preset| preset.group == group)
}

/// The preset in `group` whose changes all match the current values.
///
/// Numbers compare loosely, so `16` in the file matches a preset that asks
/// for `16.0`. A missing option means the preset does not match: the caller
/// always knows every option's effective value.
pub fn matching<F>(group: PresetGroup, get: F) -> Option<&'static Preset>
where
    F: Fn(&str) -> Option<Value>,
{
    for_group(group).find(|preset| {
        preset
            .changes
            .iter()
            .all(|(key, want)| get(key).is_some_and(|have| have.loose_eq(want)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn read(values: &[(&str, Value)], key: &str) -> Option<Value> {
        values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.clone())
    }

    #[test]
    fn every_preset_names_known_options() {
        for preset in PRESETS {
            assert!(!preset.changes.is_empty());
            for (key, _) in preset.changes {
                assert!(schema::option(key).is_some(), "unknown option {key}");
            }
        }
    }

    #[test]
    fn the_active_preset_is_the_one_whose_values_all_match() {
        let values = [
            ("layout.gaps", Value::Int(16)),
            ("layout.focus-ring.width", Value::Int(4)),
            ("layout.border.width", Value::Int(4)),
        ];
        let active = matching(PresetGroup::Appearance, |key| read(&values, key));
        assert_eq!(active.map(|preset| preset.id), Some("appearance.balanced"));
    }

    #[test]
    fn one_different_value_means_no_preset_is_active() {
        let values = [
            ("layout.gaps", Value::Int(16)),
            ("layout.focus-ring.width", Value::Int(3)),
            ("layout.border.width", Value::Int(4)),
        ];
        assert!(matching(PresetGroup::Appearance, |key| read(&values, key)).is_none());
    }

    #[test]
    fn a_missing_value_means_no_preset_is_active() {
        let values = [("layout.gaps", Value::Int(16))];
        assert!(matching(PresetGroup::Appearance, |key| read(&values, key)).is_none());
    }

    #[test]
    fn animations_off_matches_only_the_off_preset() {
        let values = [("animations.off", Value::Bool(false))];
        let active = matching(PresetGroup::Animations, |key| read(&values, key));
        assert_eq!(active.map(|preset| preset.id), Some("animations.off"));
    }

    #[test]
    fn default_animation_speed_is_the_smooth_preset() {
        let values = [
            ("animations.off", Value::Bool(true)),
            ("animations.slowdown", Value::Float(1.0)),
        ];
        let active = matching(PresetGroup::Animations, |key| read(&values, key));
        assert_eq!(active.map(|preset| preset.id), Some("animations.smooth"));
    }
}
