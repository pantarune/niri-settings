//! Generate `data/schema.json` from the niri-config source.
//!
//! Usage:
//!   niri-settings-schema-gen /path/to/niri/niri-config/src > data/schema.json
//!
//! The extractor reads Rust structs and enums. It is deliberately simple: a
//! field that it cannot classify becomes plain text. The app uses the result
//! to list every option, including options that have no designed page yet.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use syn::{Fields, GenericArgument, Item, PathArguments, Type};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    integer: Option<bool>,
    doc: String,
    source: String,
}

#[derive(Debug, Clone, Default)]
struct StructInfo {
    fields: Vec<Field>,
    tuple_types: Vec<String>,
}

type StructMap = BTreeMap<String, StructInfo>;
type EnumMap = BTreeMap<String, EnumInfo>;

#[derive(Debug, Clone)]
struct Field {
    name: String,
    ty: String,
    doc: String,
}

#[derive(Debug, Clone, Default)]
struct EnumInfo {
    variants: Vec<String>,
    doc: String,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let source = args
        .next()
        .or_else(|| std::env::var("NIRI_CONFIG_SRC").ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("usage: niri-settings-schema-gen /path/to/niri");
            eprintln!("   or: NIRI_CONFIG_SRC=/path/to/niri niri-settings-schema-gen");
            std::process::exit(2);
        });
    let root = if source.join("niri-config/src").is_dir() {
        source.join("niri-config/src")
    } else {
        source
    };
    let (structs, enums) = parse_source(&root).unwrap_or_else(|error| {
        eprintln!("cannot read {root:?}: {error}");
        std::process::exit(1);
    });
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    if let Some(config) = structs.get("Config") {
        let mut path = Vec::new();
        for field in &config.fields {
            walk_field(
                &field.name,
                &field.ty,
                &field.doc,
                &mut path,
                &structs,
                &enums,
                &mut entries,
                &mut seen,
            );
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut seen_paths = HashSet::new();
    entries.retain(|entry| seen_paths.insert(entry.path.clone()));
    println!("{}", serde_json::to_string_pretty(&entries).unwrap());
}

fn parse_source(root: &std::path::Path) -> Result<(StructMap, EnumMap), std::io::Error> {
    let mut structs = BTreeMap::new();
    let mut enums = BTreeMap::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        let Ok(file) = syn::parse_file(&text) else {
            continue;
        };
        for item in file.items {
            match item {
                Item::Struct(item) => {
                    let name = item.ident.to_string();
                    let mut info = StructInfo::default();
                    match &item.fields {
                        Fields::Named(fields) => {
                            for field in &fields.named {
                                let Some(ident) = &field.ident else {
                                    continue;
                                };
                                info.fields.push(Field {
                                    name: ident.to_string(),
                                    ty: type_name(&field.ty),
                                    doc: docs(&field.attrs),
                                });
                            }
                        }
                        Fields::Unnamed(fields) => {
                            for field in &fields.unnamed {
                                info.tuple_types.push(type_name(&field.ty));
                            }
                        }
                        Fields::Unit => {}
                    }
                    structs.insert(name, info);
                }
                Item::Enum(item) => {
                    let name = item.ident.to_string();
                    let mut info = EnumInfo {
                        doc: docs(&item.attrs),
                        ..Default::default()
                    };
                    for variant in item.variants {
                        info.variants.push(kebab(&variant.ident.to_string()));
                    }
                    enums.insert(name, info);
                }
                _ => {}
            }
        }
    }
    Ok((structs, enums))
}

#[allow(clippy::too_many_arguments)]
fn walk_field(
    name: &str,
    ty: &str,
    doc: &str,
    path: &mut Vec<String>,
    structs: &StructMap,
    enums: &EnumMap,
    out: &mut Vec<Entry>,
    seen: &mut HashSet<String>,
) {
    let ty = unwrap(ty);
    if ty.starts_with("Vec<") || ty == "Gradient" || ty == "PresetSize" {
        return;
    }
    if ty == "Animation" {
        let mut switch = path.clone();
        switch.push("off".to_string());
        out.push(Entry {
            path: switch.join("."),
            kind: "bool".to_string(),
            options: None,
            integer: None,
            doc: "Turn this animation off.".to_string(),
            source: "Animation.off".to_string(),
        });
        return;
    }
    path.push(kebab(name));
    if let Some(info) = structs.get(ty) {
        if info.fields.is_empty()
            && info.tuple_types.len() == 1
            && info.tuple_types[0] == "Animation"
        {
            // Animation sections have an `off` switch and a curve choice.
            let mut switch = path.clone();
            switch.push("off".to_string());
            out.push(Entry {
                path: switch.join("."),
                kind: "bool".to_string(),
                options: None,
                integer: None,
                doc: if doc.is_empty() {
                    "Turn this animation off.".to_string()
                } else {
                    doc.to_string()
                },
                source: format!("{ty}.off"),
            });
        } else if is_special(ty) {
            add_leaf(path, ty, doc, enums, out);
        } else if ty == "PresetSize" {
            // Preset widths use a child node, not a scalar.
        } else if seen.insert(ty.to_string()) {
            for field in &info.fields {
                walk_field(
                    &field.name,
                    &field.ty,
                    &field.doc,
                    path,
                    structs,
                    enums,
                    out,
                    seen,
                );
            }
            seen.remove(ty);
        }
    } else if let Some(info) = enums.get(ty) {
        out.push(Entry {
            path: path.join("."),
            kind: "choice".to_string(),
            options: Some(info.variants.clone()),
            integer: None,
            doc: if doc.is_empty() {
                info.doc.clone()
            } else {
                doc.to_string()
            },
            source: ty.to_string(),
        });
    } else {
        add_leaf(path, ty, doc, enums, out);
    }
    path.pop();
}

fn add_leaf(path: &[String], ty: &str, doc: &str, _enums: &EnumMap, out: &mut Vec<Entry>) {
    let (kind, integer) = match ty {
        "bool" => ("bool", None),
        "f32" | "f64" => ("number", Some(false)),
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" => {
            ("number", Some(true))
        }
        "Color" => ("color", None),
        _ => ("text", None),
    };
    out.push(Entry {
        path: path.join("."),
        kind: kind.to_string(),
        options: None,
        integer,
        doc: doc.to_string(),
        source: ty.to_string(),
    });
}

fn is_special(ty: &str) -> bool {
    matches!(ty, "Color" | "ScreenshotPath" | "FloatOrInt" | "String")
}

fn unwrap(ty: &str) -> &str {
    let ty = ty.trim();
    for prefix in ["Option<", "Box<"] {
        if let Some(rest) = ty.strip_prefix(prefix)
            && let Some(inner) = rest.strip_suffix('>')
        {
            return unwrap(inner);
        }
    }
    ty
}

fn type_name(ty: &Type) -> String {
    match ty {
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return "Unknown".to_string();
            };
            let name = segment.ident.to_string();
            match &segment.arguments {
                PathArguments::AngleBracketed(args) => {
                    let inner: Vec<String> = args
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            GenericArgument::Type(ty) => Some(type_name(ty)),
                            _ => None,
                        })
                        .collect();
                    format!("{name}<{}>", inner.join(","))
                }
                _ => name,
            }
        }
        Type::Reference(reference) => type_name(&reference.elem),
        Type::Tuple(tuple) => {
            let inner: Vec<String> = tuple.elems.iter().map(type_name).collect();
            format!("({})", inner.join(","))
        }
        Type::Slice(slice) => format!("[{}]", type_name(&slice.elem)),
        _ => "Unknown".to_string(),
    }
}

fn docs(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(value) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(text),
                ..
            }) = &value.value
        {
            lines.push(text.value().trim().to_string());
        }
    }
    lines.join(" ")
}

fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if ch == '_' {
            out.push('-');
        } else if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_small_tree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lib.rs"),
            r#"
            pub struct Config {
                pub layout: Layout,
                pub prefer_no_csd: bool,
                pub outputs: Vec<Output>,
            }
            pub enum Center {
                Never,
                OnOverflow,
            }
            "#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("layout.rs"),
            r#"
            pub struct Layout {
                pub gaps: f64,
                pub center: Center,
                pub border: Option<Border>,
            }
            pub struct Border {
                pub off: bool,
                pub width: f64,
            }
            "#,
        )
        .unwrap();
        let (structs, enums) = parse_source(dir.path()).unwrap();
        let config = structs.get("Config").unwrap();
        let mut entries = Vec::new();
        let mut path = Vec::new();
        let mut seen = HashSet::new();
        for field in &config.fields {
            walk_field(
                &field.name,
                &field.ty,
                &field.doc,
                &mut path,
                &structs,
                &enums,
                &mut entries,
                &mut seen,
            );
        }
        let paths: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert!(paths.contains(&"layout.gaps"));
        assert!(paths.contains(&"layout.border.off"));
        assert!(paths.contains(&"layout.border.width"));
        assert!(paths.contains(&"prefer-no-csd"));
        assert!(paths.contains(&"layout.center"));
        let center = entries
            .iter()
            .find(|entry| entry.path == "layout.center")
            .unwrap();
        assert_eq!(center.options.as_ref().unwrap(), &["never", "on-overflow"]);
    }

    #[test]
    fn checked_in_schema_has_known_paths() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/schema.json"),
        )
        .unwrap();
        let entries: Vec<Entry> = serde_json::from_str(&text).unwrap();
        let paths: HashSet<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert!(paths.contains("layout.gaps"));
        assert!(paths.contains("input.keyboard.repeat-delay"));
        assert!(paths.contains("animations.window-open.off"));
        assert!(entries.len() > 150);
    }
}
