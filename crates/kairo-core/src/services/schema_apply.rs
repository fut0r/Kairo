//! Previewing a schema and applying it to a database.

use crate::db::{ColumnInfo, Connection, Engine, TableKind};
use crate::error::{KairoError, Result};
use crate::schema::sqlgen::{create_table_sql, effective_name, sql_type};
use crate::schema::{DefaultValue, Dialect, Schema, Table};
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldPreview {
    pub name: String,
    pub type_name: String,
    /// The column type this field becomes in the chosen dialect.
    pub sql_type: String,
    /// False when the type is not a `.kairo` type and falls back to text.
    pub known_type: bool,
    pub default_value: Option<String>,
    pub required: bool,
    pub primary: bool,
    pub unique: bool,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TablePreview {
    pub name: String,
    pub fields: Vec<FieldPreview>,
    pub sql: String,
    pub line: u32,
}

/// A parsed schema, resolved for one dialect, before anything touches a database.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaPreview {
    pub dialect: Dialect,
    pub tables: Vec<TablePreview>,
    pub sql: String,
}

fn default_text(value: &DefaultValue) -> String {
    match value {
        DefaultValue::Bool(value) => value.to_string(),
        DefaultValue::Int(text) | DefaultValue::Float(text) => text.clone(),
        DefaultValue::Text(text) => format!("\"{text}\""),
    }
}

pub fn preview(schema: &Schema, dialect: Dialect) -> SchemaPreview {
    let tables: Vec<TablePreview> = schema
        .tables
        .iter()
        .map(|table| TablePreview {
            name: table.name.clone(),
            line: table.line,
            sql: create_table_sql(table, dialect),
            fields: table
                .fields
                .iter()
                .map(|field| FieldPreview {
                    name: field.name.clone(),
                    type_name: field.type_name.clone(),
                    sql_type: sql_type(&field.type_name, dialect).to_string(),
                    known_type: field.is_known_type(),
                    default_value: field.default_value.as_ref().map(default_text),
                    required: field.required,
                    primary: field.primary,
                    unique: field.unique,
                    line: field.line,
                })
                .collect(),
        })
        .collect();

    let sql = tables
        .iter()
        .map(|t| t.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    SchemaPreview {
        dialect,
        tables,
        sql,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanAction {
    /// The table does not exist and will be created.
    Create,
    /// A table with this name exists. It will be left exactly as it is.
    Exists,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub table: String,
    pub action: PlanAction,
    pub sql: String,
    /// How an existing table differs from the schema. Empty when it matches
    /// or when the table is new.
    pub differences: Vec<String>,
}

/// What applying a schema to a specific database will do.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPlan {
    pub engine: Engine,
    pub target_name: String,
    /// Path, or URL with the password masked.
    pub target_location: String,
    pub items: Vec<PlanItem>,
    pub creates: usize,
    pub existing: usize,
    /// The exact script that will run.
    pub sql: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub engine: Engine,
    pub target_name: String,
    pub target_location: String,
    pub items: Vec<PlanItem>,
    pub created: usize,
    pub unchanged: usize,
    pub sql: String,
    pub elapsed_ms: u64,
}

/// Describes how a live table differs from the schema's table.
fn differences(table: &Table, live: &[ColumnInfo]) -> Vec<String> {
    let mut found = Vec::new();

    for field in &table.fields {
        match live
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&field.name))
        {
            None => found.push(format!(
                "`{}` is in the schema but not in the database.",
                field.name
            )),
            Some(column) if field.is_known_type() && column.kairo_type != field.type_name => found
                .push(format!(
                    "`{}` is `{}` in the schema but `{}` in the database.",
                    field.name,
                    field.type_name,
                    column.data_type.to_lowercase()
                )),
            Some(_) => {}
        }
    }

    for column in live {
        if !table
            .fields
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case(&column.name))
        {
            found.push(format!(
                "`{}` is in the database but not in the schema.",
                column.name
            ));
        }
    }

    found
}

/// Compares the schema with the database without changing anything.
pub async fn plan(conn: &Connection, schema: &Schema) -> Result<ApplyPlan> {
    if schema.tables.is_empty() {
        return Err(KairoError::invalid_input(
            "The schema defines no tables, so there is nothing to apply.",
        ));
    }

    let dialect = conn.dialect();
    let live_tables = conn.list_tables().await?;
    let mut items = Vec::with_capacity(schema.tables.len());

    for table in &schema.tables {
        let wanted = effective_name(&table.name, table.quoted, dialect);
        let existing = live_tables.iter().find(|live| match dialect {
            // SQLite compares table names without regard to case.
            Dialect::Sqlite => live.name.eq_ignore_ascii_case(&wanted),
            Dialect::Postgres => live.name == wanted,
        });

        let (action, differences) = match existing {
            Some(live) if live.kind == TableKind::View => (
                PlanAction::Exists,
                vec![format!("`{}` already exists as a view.", live.name)],
            ),
            Some(live) => {
                let detail = conn.describe_table(&live.name).await?;
                (PlanAction::Exists, differences(table, &detail.columns))
            }
            None => (PlanAction::Create, Vec::new()),
        };

        items.push(PlanItem {
            table: table.name.clone(),
            action,
            sql: create_table_sql(table, dialect),
            differences,
        });
    }

    let creates = items
        .iter()
        .filter(|i| i.action == PlanAction::Create)
        .count();
    let sql = items
        .iter()
        .map(|i| i.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    let info = conn.info();
    Ok(ApplyPlan {
        engine: info.engine,
        target_name: info.name.clone(),
        target_location: info.location.clone(),
        existing: items.len() - creates,
        creates,
        items,
        sql,
    })
}

/// Creates the schema's tables in one transaction. Tables that already exist
/// are not modified: Kairo creates, it does not migrate.
pub async fn apply(conn: &Connection, schema: &Schema) -> Result<ApplyReport> {
    let plan = plan(conn, schema).await?;
    let statements: Vec<String> = plan.items.iter().map(|i| i.sql.clone()).collect();

    let started = Instant::now();
    if let Err(failure) = conn.execute_batch(&statements).await {
        let table = failure
            .index
            .and_then(|index| plan.items.get(index))
            .map(|item| format!(" at table `{}`", item.table))
            .unwrap_or_default();

        let mut error = KairoError::new(
            failure.error.kind,
            format!("Applying the schema failed{table}. Nothing was changed."),
        )
        .with_detail(failure.error.to_string());
        if let Some(hint) = failure.error.hint {
            error = error.with_hint(hint);
        }
        return Err(error);
    }

    Ok(ApplyReport {
        engine: plan.engine,
        target_name: plan.target_name,
        target_location: plan.target_location,
        created: plan.creates,
        unchanged: plan.existing,
        items: plan.items,
        sql: plan.sql,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse_schema;

    #[test]
    fn preview_resolves_types_for_the_dialect() {
        let schema = parse_schema(
            "table t {\n  id: int [primary]\n  price: float = 1.5 [required]\n  tag: money\n}",
        )
        .unwrap();

        let sqlite = preview(&schema, Dialect::Sqlite);
        let fields = &sqlite.tables[0].fields;
        assert_eq!(fields[1].sql_type, "REAL");
        assert_eq!(fields[1].default_value.as_deref(), Some("1.5"));
        assert!(fields[1].required && fields[0].primary);
        assert!(!fields[2].known_type);
        assert_eq!(fields[2].sql_type, "TEXT");
        assert_eq!(fields[1].line, 3);

        let postgres = preview(&schema, Dialect::Postgres);
        assert_eq!(postgres.tables[0].fields[1].sql_type, "DOUBLE PRECISION");
        assert!(postgres.sql.starts_with("CREATE TABLE IF NOT EXISTS t ("));
    }

    #[test]
    fn differences_name_each_mismatch() {
        let schema = parse_schema("table t { a: int, b: string, c: bool }").unwrap();
        let column = |name: &str, data_type: &str, kairo_type: &str| ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            kairo_type: kairo_type.into(),
            nullable: true,
            default_value: None,
            primary_key_position: 0,
        };
        let live = [
            column("A", "INTEGER", "int"),
            column("b", "INTEGER", "int"),
            column("extra", "TEXT", "string"),
        ];

        let found = differences(&schema.tables[0], &live);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found[0].contains("`b` is `string` in the schema but `integer`"));
        assert!(found[1].contains("`c` is in the schema but not in the database"));
        assert!(found[2].contains("`extra` is in the database but not in the schema"));
    }
}
