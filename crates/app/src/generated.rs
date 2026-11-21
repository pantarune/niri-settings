//! Options extracted from the niri-config source at build time.

use std::sync::OnceLock;

use serde::Deserialize;

const SCHEMA: &str = include_str!("../../../data/schema.json");

#[derive(Debug, Clone, Deserialize)]
pub struct GeneratedOption {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub doc: String,
}

pub fn options() -> &'static [GeneratedOption] {
    static CACHE: OnceLock<Vec<GeneratedOption>> = OnceLock::new();
    CACHE.get_or_init(|| serde_json::from_str(SCHEMA).unwrap_or_default())
}

pub fn extras(known: &std::collections::HashSet<String>) -> Vec<&'static GeneratedOption> {
    options()
        .iter()
        .filter(|option| !known.contains(&option.path))
        .collect()
}
