//! The plain-text database report printed by `kairo read`.
//!
//! This is the one place the core produces text for a terminal. It lives here
//! so the format stays the same as earlier releases.

use crate::db::{Connection, PageRequest, TableKind};
use crate::error::Result;
use crate::redact::redact;

/// Rows shown per table.
const SAMPLE_ROWS: u32 = 5;

/// Describes every table: its columns, row count and first rows.
///
/// `source` is shown in the first line; it defaults to the connection's
/// location. It is redacted, so a URL never shows its password.
pub async fn read_report(conn: &Connection, source: Option<&str>) -> Result<String> {
    let info = conn.info();
    let source = redact(source.unwrap_or(&info.location));

    let mut out = format!(
        "Database ({}): {}\n{}\n\n",
        info.engine.label(),
        source,
        "-".repeat(40)
    );

    let tables: Vec<_> = conn
        .list_tables()
        .await?
        .into_iter()
        .filter(|t| t.kind == TableKind::Table)
        .collect();

    if tables.is_empty() {
        out.push_str("(empty database)\n");
        return Ok(out);
    }

    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    out.push_str(&format!("Tables: {}\n\n", names.join(", ")));

    for table in &tables {
        let detail = conn.describe_table(&table.name).await?;

        out.push_str(&format!("table {} {{\n", table.name));
        for column in &detail.columns {
            let mut line = format!("  {}: {}", column.name, column.kairo_type);
            if let Some(default) = column.default_value.as_deref().filter(|d| !d.is_empty()) {
                line.push_str(&format!(" = {default}"));
            }
            if !column.nullable {
                line.push_str(" [required]");
            }
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("}\n\n");

        let page = conn
            .fetch_rows(&table.name, &PageRequest::first(SAMPLE_ROWS))
            .await?;
        out.push_str(&format!("  -- {} rows\n", page.total_rows));

        for row in &page.rows {
            let fields: Vec<String> = page
                .columns
                .iter()
                .zip(row)
                .map(|(column, value)| format!("{}: {}", column.name, value.display()))
                .collect();
            out.push_str(&format!("  | {}\n", fields.join(" | ")));
        }

        let shown = page.rows.len() as i64;
        if page.total_rows > shown {
            out.push_str(&format!(
                "  ... and {} more rows\n",
                page.total_rows - shown
            ));
        }
        out.push('\n');
    }

    Ok(out)
}
