//! Exporting a database's structure as a `.kairo` schema.
//!
//! The output always parses. What the grammar cannot express is written as a
//! comment so nothing is silently dropped.

use crate::db::sql::is_canonical_type;
use crate::db::{ColumnInfo, Connection, Engine, IndexInfo, TableKind};
use crate::error::Result;
use crate::redact::redact;
use crate::schema::{DefaultValue, Field, Schema, Table, is_bare_ident, render_kairo};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedSchema {
    /// The `.kairo` text, with a header naming the source.
    pub text: String,
    pub table_count: usize,
    /// Things left out, with the reason. Also written into `text` as comments.
    pub notes: Vec<String>,
}

/// A name the grammar can hold. Quoted names cannot contain a quote or a
/// line break.
fn representable(name: &str) -> bool {
    !name.is_empty() && !name.contains('"') && !name.contains('\n')
}

/// Whether a name must be written in quotes to survive a round trip.
fn needs_quotes(name: &str, engine: Engine) -> bool {
    // PostgreSQL folds unquoted names to lower case.
    !is_bare_ident(name) || (engine == Engine::Postgres && name != name.to_ascii_lowercase())
}

fn is_integer(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn is_decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    match digits.split_once('.') {
        Some((whole, fraction)) => {
            !whole.is_empty()
                && !fraction.is_empty()
                && whole.chars().all(|c| c.is_ascii_digit())
                && fraction.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// Converts a column default, as the database prints it, into a `.kairo`
/// literal. Returns `None` for expressions such as `CURRENT_TIMESTAMP`.
fn convert_default(expression: &str, kairo_type: &str) -> Option<DefaultValue> {
    let mut text = expression.trim();

    // PostgreSQL prints literals with a cast: 'abc'::text, '-1'::integer.
    if let Some(cast_at) = text.rfind("::") {
        let before = text[..cast_at].trim_end();
        if before.ends_with('\'') || is_integer(before) || is_decimal(before) {
            text = before;
        }
    }

    // SQLite keeps the parentheses of a default written as (1).
    while text.starts_with('(') && text.ends_with(')') && text.len() > 2 {
        text = text[1..text.len() - 1].trim();
    }

    let lower = text.to_ascii_lowercase();
    if lower == "true" || lower == "false" {
        return Some(DefaultValue::Bool(lower == "true"));
    }
    if kairo_type == "bool" && (text == "0" || text == "1") {
        return Some(DefaultValue::Bool(text == "1"));
    }
    if is_integer(text) {
        return Some(DefaultValue::Int(text.to_string()));
    }
    if is_decimal(text) {
        return Some(DefaultValue::Float(text.to_string()));
    }

    let inner = text
        .strip_prefix('\'')?
        .strip_suffix('\'')?
        .replace("''", "'");
    match kairo_type {
        "int" if is_integer(&inner) => Some(DefaultValue::Int(inner)),
        "float" if is_integer(&inner) || is_decimal(&inner) => Some(DefaultValue::Float(inner)),
        "bool" if inner == "true" || inner == "false" => Some(DefaultValue::Bool(inner == "true")),
        // The grammar has no escapes, so a quote or line break cannot be written.
        _ if inner.contains('"') || inner.contains('\n') => None,
        _ => Some(DefaultValue::Text(inner)),
    }
}

fn field_from_column(column: &ColumnInfo, indexes: &[IndexInfo], engine: Engine) -> Field {
    let mut comments = Vec::new();
    if !is_canonical_type(&column.data_type, &column.kairo_type) && !column.data_type.is_empty() {
        comments.push(format!("native: {}", column.data_type.to_lowercase()));
    }

    let default_value = column.default_value.as_deref().and_then(|expression| {
        let converted = convert_default(expression, &column.kairo_type);
        if converted.is_none() && !expression.trim().eq_ignore_ascii_case("null") {
            comments.push(format!("default: {}", expression.replace('\n', " ")));
        }
        converted
    });

    let primary = column.primary_key_position > 0;
    let unique = indexes.iter().any(|index| {
        index.unique
            && !index.primary
            && index.columns.len() == 1
            && index.columns[0] == column.name
    });

    Field {
        name: column.name.clone(),
        quoted: needs_quotes(&column.name, engine),
        type_name: column.kairo_type.clone(),
        default_value,
        required: !column.nullable && !primary,
        primary,
        unique,
        comment: (!comments.is_empty()).then(|| comments.join("; ")),
        line: 0,
    }
}

/// Reads every table into a [`Schema`], with notes about what was left out.
pub async fn schema_from_database(conn: &Connection) -> Result<(Schema, Vec<String>)> {
    let engine = conn.engine();
    let mut schema = Schema::default();
    let mut notes = Vec::new();

    for summary in conn.list_tables().await? {
        if summary.kind == TableKind::View {
            continue;
        }
        if !representable(&summary.name) {
            notes.push(format!(
                "Skipped table {:?}: its name cannot be written in a .kairo file.",
                summary.name
            ));
            continue;
        }

        let detail = conn.describe_table(&summary.name).await?;
        let mut fields = Vec::with_capacity(detail.columns.len());
        for column in &detail.columns {
            if representable(&column.name) {
                fields.push(field_from_column(column, &detail.indexes, engine));
            } else {
                notes.push(format!(
                    "Skipped column {:?} of {}: its name cannot be written in a .kairo file.",
                    column.name, summary.name
                ));
            }
        }

        if fields.is_empty() {
            notes.push(format!(
                "Skipped table {}: it has no columns.",
                summary.name
            ));
            continue;
        }

        schema.tables.push(Table {
            quoted: needs_quotes(&summary.name, engine),
            name: summary.name,
            fields,
            line: 0,
        });
    }

    Ok((schema, notes))
}

/// Exports the structure of every table as `.kairo` text.
///
/// `source` is what the header shows as the origin; it defaults to the
/// connection's own location. Either way it is redacted.
pub async fn export_schema(conn: &Connection, source: Option<&str>) -> Result<ExportedSchema> {
    let (schema, notes) = schema_from_database(conn).await?;
    let info = conn.info();
    let source = redact(source.unwrap_or(&info.location));

    let mut text = format!(
        "// Generated by KairoDB\n// Source ({}): {}\n",
        info.engine.label(),
        source
    );
    for note in &notes {
        text.push_str(&format!("// {note}\n"));
    }
    text.push('\n');
    text.push_str(&render_kairo(&schema));

    Ok(ExportedSchema {
        text,
        table_count: schema.tables.len(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_become_literals_when_they_can() {
        use DefaultValue::*;
        assert_eq!(convert_default("0", "int"), Some(Int("0".into())));
        assert_eq!(convert_default("-12", "int"), Some(Int("-12".into())));
        assert_eq!(convert_default("1.50", "float"), Some(Float("1.50".into())));
        assert_eq!(convert_default("TRUE", "bool"), Some(Bool(true)));
        assert_eq!(convert_default("1", "bool"), Some(Bool(true)));
        assert_eq!(convert_default("0", "bool"), Some(Bool(false)));
        assert_eq!(
            convert_default("'guest'", "string"),
            Some(Text("guest".into()))
        );
        assert_eq!(
            convert_default("'it''s'", "string"),
            Some(Text("it's".into()))
        );
        assert_eq!(convert_default("(7)", "int"), Some(Int("7".into())));
    }

    #[test]
    fn postgres_casts_are_removed() {
        use DefaultValue::*;
        assert_eq!(
            convert_default("'guest'::text", "string"),
            Some(Text("guest".into()))
        );
        assert_eq!(
            convert_default("'pending'::character varying", "string"),
            Some(Text("pending".into()))
        );
        assert_eq!(
            convert_default("'-1'::integer", "int"),
            Some(Int("-1".into()))
        );
        assert_eq!(convert_default("0::bigint", "int"), Some(Int("0".into())));
    }

    #[test]
    fn expressions_are_not_literals() {
        assert_eq!(convert_default("CURRENT_TIMESTAMP", "timestamp"), None);
        assert_eq!(convert_default("now()", "timestamp"), None);
        assert_eq!(
            convert_default("nextval('users_id_seq'::regclass)", "int"),
            None
        );
        assert_eq!(convert_default("NULL", "string"), None);
        assert_eq!(convert_default("'say \"hi\"'", "string"), None);
    }

    #[test]
    fn mixed_case_names_are_quoted_only_where_case_matters() {
        assert!(!needs_quotes("users", Engine::Postgres));
        assert!(needs_quotes("userId", Engine::Postgres));
        assert!(!needs_quotes("userId", Engine::Sqlite));
        assert!(needs_quotes("order items", Engine::Sqlite));
    }

    #[test]
    fn column_facts_become_field_modifiers_and_notes() {
        let column = ColumnInfo {
            name: "email".into(),
            data_type: "VARCHAR(255)".into(),
            kairo_type: "string".into(),
            nullable: false,
            default_value: Some("lower('X')".into()),
            primary_key_position: 0,
        };
        let index = IndexInfo {
            name: "users_email_key".into(),
            unique: true,
            primary: false,
            columns: vec!["email".into()],
            origin: "unique constraint".into(),
            definition: None,
        };
        let field = field_from_column(&column, &[index], Engine::Sqlite);
        assert!(field.required && field.unique && !field.primary);
        assert_eq!(field.default_value, None);
        assert_eq!(
            field.comment.as_deref(),
            Some("native: varchar(255); default: lower('X')")
        );
    }
}
