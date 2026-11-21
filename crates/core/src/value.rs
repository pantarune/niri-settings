//! Values stored in the niri config.
//!
//! The model is deliberately small. niri uses KDL scalars, so a value is one of
//! five scalar kinds. Collections stay as nodes and get their own editor.

use std::fmt;

use kdl::KdlValue;
use serde::Serialize;

/// A scalar value in the niri config.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Null,
}

impl Value {
    /// Read a KDL value. Bare nodes have no value and read as `true` elsewhere.
    pub fn from_kdl(value: &KdlValue) -> Self {
        match value {
            KdlValue::Bool(v) => Self::Bool(*v),
            KdlValue::Integer(v) => Self::Int(i64::try_from(*v).unwrap_or_default()),
            KdlValue::Float(v) => Self::Float(*v),
            KdlValue::String(v) => Self::String(v.clone()),
            KdlValue::Null => Self::Null,
        }
    }

    /// Write the value as a KDL v1 literal. Strings keep quotes and escapes.
    pub fn literal(&self) -> String {
        match self {
            Self::Bool(v) => v.to_string(),
            Self::Int(v) => v.to_string(),
            Self::Float(v) => {
                let mut out = format!("{v}");
                if !out.contains('.') && !out.contains('e') && !out.contains('E') {
                    out.push_str(".0");
                }
                out
            }
            Self::String(v) => {
                let mut out = String::with_capacity(v.len() + 2);
                out.push('"');
                for ch in v.chars() {
                    match ch {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\t' => out.push_str("\\t"),
                        '\r' => out.push_str("\\r"),
                        other => out.push(other),
                    }
                }
                out.push('"');
                out
            }
            Self::Null => "null".to_string(),
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            Self::Int(v) => Some(*v != 0),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(v) => Some(*v as f64),
            Self::Float(v) => Some(*v),
            Self::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(v) => Some(v),
            _ => None,
        }
    }

    /// True when this value is equal to another value of a different but
    /// compatible kind, for example `1` and `1.0`.
    pub fn loose_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => (a - b).abs() < f64::EPSILON,
            (Self::Int(_), Self::Float(_)) | (Self::Float(_), Self::Int(_)) => {
                match (self.as_f64(), other.as_f64()) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                    _ => false,
                }
            }
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Null, Self::Null) => true,
            _ => false,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "true or false",
            Self::Int(_) => "a whole number",
            Self::Float(_) => "a number",
            Self::String(_) => "text",
            Self::Null => "empty",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(v) => f.write_str(v),
            other => f.write_str(&other.literal()),
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}
