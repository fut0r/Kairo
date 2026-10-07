//! The `.kairo` schema domain: model, parser, SQL generation, rendering.

pub mod parser;
pub mod render;
pub mod sqlgen;

pub use parser::{Diagnostic, Severity, ValidationReport, parse_schema, validate};
pub use render::render_kairo;
pub use sqlgen::{Dialect, create_table_sql, generate_sql, generate_statements, sql_type};

use serde::Serialize;

/// The types a `.kairo` field may have.
pub const KNOWN_TYPES: [&str; 6] = ["string", "int", "float", "bool", "blob", "timestamp"];

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub tables: Vec<Table>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub name: String,
    /// The name was written in quotes, so its case and spelling are exact.
    pub quoted: bool,
    pub fields: Vec<Field>,
    /// 1-based source line, or 0 when the table did not come from a file.
    pub line: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub quoted: bool,
    pub type_name: String,
    pub default_value: Option<DefaultValue>,
    pub required: bool,
    pub primary: bool,
    pub unique: bool,
    /// Trailing comment written by export, e.g. the native column type.
    pub comment: Option<String>,
    pub line: u32,
}

/// A literal default. Numbers keep their source spelling so nothing is lost
/// to parsing and re-printing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum DefaultValue {
    Bool(bool),
    Int(String),
    Float(String),
    Text(String),
}

impl Field {
    pub fn is_known_type(&self) -> bool {
        KNOWN_TYPES.contains(&self.type_name.as_str())
    }
}

/// True when `name` can be written without quotes in a `.kairo` file.
pub fn is_bare_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
