//! Turns a [`Schema`] into `CREATE TABLE` statements for a dialect.

use super::{DefaultValue, Field, Schema, Table, is_bare_ident};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Sqlite,
    Postgres,
}

/// Words that cannot be used as a bare identifier in SQLite or PostgreSQL.
const RESERVED: &[&str] = &[
    "all",
    "alter",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "between",
    "both",
    "by",
    "case",
    "cast",
    "check",
    "collate",
    "column",
    "commit",
    "constraint",
    "create",
    "cross",
    "current_date",
    "current_time",
    "current_timestamp",
    "current_user",
    "default",
    "delete",
    "desc",
    "distinct",
    "do",
    "drop",
    "else",
    "end",
    "except",
    "exists",
    "false",
    "fetch",
    "for",
    "foreign",
    "from",
    "full",
    "grant",
    "group",
    "having",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "left",
    "like",
    "limit",
    "natural",
    "not",
    "null",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "outer",
    "primary",
    "references",
    "right",
    "select",
    "set",
    "table",
    "then",
    "to",
    "true",
    "union",
    "unique",
    "update",
    "user",
    "using",
    "values",
];

fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name.to_ascii_lowercase().as_str())
}

/// The name a table gets in the database. PostgreSQL folds unquoted names to
/// lower case; SQLite keeps them as written.
pub fn effective_name(name: &str, quoted: bool, dialect: Dialect) -> String {
    if dialect == Dialect::Postgres && !quoted && is_bare_ident(name) {
        name.to_ascii_lowercase()
    } else {
        name.to_string()
    }
}

/// Quotes a schema name only when SQL requires it, so ordinary schemas
/// generate the same SQL they always have.
fn sql_name(name: &str, quoted: bool, dialect: Dialect) -> String {
    if quoted || !is_bare_ident(name) {
        format!("\"{}\"", name.replace('"', "\"\""))
    } else if is_reserved(name) {
        // Quoting must not change which object an unquoted name refers to.
        format!("\"{}\"", effective_name(name, false, dialect))
    } else {
        name.to_string()
    }
}

/// The column type a `.kairo` type becomes. Unknown types are stored as text.
pub fn sql_type(type_name: &str, dialect: Dialect) -> &'static str {
    match (type_name, dialect) {
        ("string", _) => "TEXT",
        ("int", _) => "INTEGER",
        ("bool", _) => "BOOLEAN",
        ("float", Dialect::Sqlite) => "REAL",
        ("float", Dialect::Postgres) => "DOUBLE PRECISION",
        ("blob", Dialect::Sqlite) => "BLOB",
        ("blob", Dialect::Postgres) => "BYTEA",
        ("timestamp", _) => "TIMESTAMP",
        _ => "TEXT",
    }
}

fn default_sql(value: &DefaultValue) -> String {
    match value {
        DefaultValue::Bool(true) => "true".to_string(),
        DefaultValue::Bool(false) => "false".to_string(),
        DefaultValue::Int(text) | DefaultValue::Float(text) => text.clone(),
        DefaultValue::Text(text) => format!("'{}'", text.replace('\'', "''")),
    }
}

fn column_sql(field: &Field, dialect: Dialect, inline_primary: bool) -> String {
    let mut sql = format!(
        "  {} {}",
        sql_name(&field.name, field.quoted, dialect),
        sql_type(&field.type_name, dialect)
    );
    if field.primary && inline_primary {
        sql.push_str(" PRIMARY KEY");
    }
    if field.required {
        sql.push_str(" NOT NULL");
    }
    if field.unique {
        sql.push_str(" UNIQUE");
    }
    if let Some(default) = &field.default_value {
        sql.push_str(" DEFAULT ");
        sql.push_str(&default_sql(default));
    }
    sql
}

/// One `CREATE TABLE IF NOT EXISTS` statement, ending in `;`.
pub fn create_table_sql(table: &Table, dialect: Dialect) -> String {
    let primary: Vec<&Field> = table.fields.iter().filter(|f| f.primary).collect();
    let inline_primary = primary.len() == 1;

    let mut lines: Vec<String> = table
        .fields
        .iter()
        .map(|field| column_sql(field, dialect, inline_primary))
        .collect();

    if primary.len() > 1 {
        let columns: Vec<String> = primary
            .iter()
            .map(|f| sql_name(&f.name, f.quoted, dialect))
            .collect();
        lines.push(format!("  PRIMARY KEY ({})", columns.join(", ")));
    }

    format!(
        "CREATE TABLE IF NOT EXISTS {} (\n{}\n);",
        sql_name(&table.name, table.quoted, dialect),
        lines.join(",\n")
    )
}

/// One statement per table, in source order.
pub fn generate_statements(schema: &Schema, dialect: Dialect) -> Vec<String> {
    schema
        .tables
        .iter()
        .map(|table| create_table_sql(table, dialect))
        .collect()
}

/// The whole schema as one SQL script.
pub fn generate_sql(schema: &Schema, dialect: Dialect) -> String {
    generate_statements(schema, dialect)
        .iter()
        .map(|statement| format!("{statement}\n\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse_schema;

    fn sql(source: &str, dialect: Dialect) -> String {
        generate_sql(&parse_schema(source).unwrap(), dialect)
    }

    #[test]
    fn matches_the_sql_0_4_generated() {
        // Byte for byte what `kairo create users` printed in 0.4.0.
        assert_eq!(
            sql("table users { name: string, age: int }", Dialect::Sqlite),
            "CREATE TABLE IF NOT EXISTS users (\n  name TEXT,\n  age INTEGER\n);\n\n"
        );
        assert_eq!(
            sql(
                "table posts {\n  published: bool = false\n  views: int = 0\n}",
                Dialect::Sqlite
            ),
            "CREATE TABLE IF NOT EXISTS posts (\n  published BOOLEAN DEFAULT false,\n  views INTEGER DEFAULT 0\n);\n\n"
        );
    }

    #[test]
    fn text_defaults_are_sql_string_literals() {
        let out = sql("table t { a: string = \"it's\" }", Dialect::Postgres);
        assert!(out.contains("a TEXT DEFAULT 'it''s'"), "{out}");
    }

    #[test]
    fn modifiers_become_constraints() {
        let out = sql(
            "table t { id: int [primary], email: string [required, unique] }",
            Dialect::Sqlite,
        );
        assert!(out.contains("id INTEGER PRIMARY KEY,"), "{out}");
        assert!(out.contains("email TEXT NOT NULL UNIQUE"), "{out}");
    }

    #[test]
    fn several_primary_fields_become_a_composite_key() {
        let out = sql(
            "table t { a: int [primary], b: int [primary], c: string }",
            Dialect::Sqlite,
        );
        assert!(
            out.contains("  a INTEGER,\n  b INTEGER,\n  c TEXT,\n  PRIMARY KEY (a, b)\n"),
            "{out}"
        );
    }

    #[test]
    fn types_follow_the_dialect() {
        let source = "table t { a: float, b: blob, c: timestamp, d: money }";
        let sqlite = sql(source, Dialect::Sqlite);
        assert!(sqlite.contains("a REAL") && sqlite.contains("b BLOB"));
        assert!(sqlite.contains("c TIMESTAMP") && sqlite.contains("d TEXT"));
        let postgres = sql(source, Dialect::Postgres);
        assert!(postgres.contains("a DOUBLE PRECISION") && postgres.contains("b BYTEA"));
    }

    #[test]
    fn reserved_and_quoted_names_are_quoted() {
        let out = sql(
            "table order { user: string, \"Unit Price\": float }",
            Dialect::Postgres,
        );
        assert!(
            out.contains("CREATE TABLE IF NOT EXISTS \"order\" ("),
            "{out}"
        );
        assert!(out.contains("\"user\" TEXT"), "{out}");
        assert!(out.contains("\"Unit Price\" DOUBLE PRECISION"), "{out}");
    }

    #[test]
    fn each_table_is_its_own_statement() {
        let schema = parse_schema("table a { x: int }\ntable b { y: int }").unwrap();
        let statements = generate_statements(&schema, Dialect::Sqlite);
        assert_eq!(statements.len(), 2);
        assert!(statements.iter().all(|s| s.ends_with(");")));
    }
}
