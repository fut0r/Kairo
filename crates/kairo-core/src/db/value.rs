//! A database cell, already shaped for display.

use serde::Serialize;

/// Text longer than this is clipped before it leaves the core, so a single
/// huge cell cannot stall a result grid.
pub const MAX_TEXT_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueKind {
    Null,
    Bool,
    Int,
    Float,
    Text,
    Blob,
}

/// One cell. Numbers travel as text so 64-bit integers survive JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Value {
    pub kind: ValueKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Value {
    fn of(kind: ValueKind, text: String) -> Self {
        Self {
            kind,
            text,
            truncated: false,
        }
    }

    pub fn null() -> Self {
        Self::of(ValueKind::Null, String::new())
    }

    pub fn bool(value: bool) -> Self {
        Self::of(ValueKind::Bool, value.to_string())
    }

    pub fn int(value: i64) -> Self {
        Self::of(ValueKind::Int, value.to_string())
    }

    pub fn float(value: f64) -> Self {
        // `{:?}` keeps the decimal point, so 1.0 does not read as an integer.
        Self::of(ValueKind::Float, format!("{value:?}"))
    }

    /// A number the database already rendered, such as a PostgreSQL `numeric`.
    pub fn number_text(kind: ValueKind, text: impl Into<String>) -> Self {
        Self::of(kind, text.into())
    }

    pub fn text(value: impl Into<String>) -> Self {
        let value: String = value.into();
        match value.char_indices().nth(MAX_TEXT_CHARS) {
            Some((cut, _)) => Self {
                kind: ValueKind::Text,
                text: value[..cut].to_string(),
                truncated: true,
            },
            None => Self::of(ValueKind::Text, value),
        }
    }

    pub fn blob(len: usize) -> Self {
        let unit = if len == 1 { "byte" } else { "bytes" };
        Self::of(ValueKind::Blob, format!("<blob {len} {unit}>"))
    }

    pub fn is_null(&self) -> bool {
        self.kind == ValueKind::Null
    }

    /// The cell as plain text, with `null` for a missing value.
    pub fn display(&self) -> &str {
        if self.is_null() { "null" } else { &self.text }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_integers_keep_every_digit() {
        let value = Value::int(i64::MAX);
        assert_eq!(value.text, "9223372036854775807");
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#"{"kind":"int","text":"9223372036854775807"}"#);
    }

    #[test]
    fn null_serialises_without_text() {
        assert_eq!(
            serde_json::to_string(&Value::null()).unwrap(),
            r#"{"kind":"null"}"#
        );
        assert_eq!(Value::null().display(), "null");
    }

    #[test]
    fn floats_keep_their_decimal_point() {
        assert_eq!(Value::float(1.0).text, "1.0");
        assert_eq!(Value::float(0.25).text, "0.25");
    }

    #[test]
    fn long_text_is_clipped_on_a_character_boundary() {
        let value = Value::text("é".repeat(MAX_TEXT_CHARS + 50));
        assert!(value.truncated);
        assert_eq!(value.text.chars().count(), MAX_TEXT_CHARS);

        let short = Value::text("hello");
        assert!(!short.truncated);
        assert_eq!(short.display(), "hello");
    }

    #[test]
    fn blobs_are_summarised() {
        assert_eq!(Value::blob(1).text, "<blob 1 byte>");
        assert_eq!(Value::blob(2048).text, "<blob 2048 bytes>");
    }
}
