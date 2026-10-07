//! Running queries.

use super::safety::{self, Risk, SqlAnalysis};
use crate::db::{Connection, ResultColumn, RunOptions, Value};
use crate::error::{KairoError, Result};
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct QueryOptions {
    /// Rows kept from the result. Use `usize::MAX` for no limit.
    pub max_rows: usize,
    /// Stop the statement after this long. `None` means no limit.
    pub timeout: Option<Duration>,
}

impl QueryOptions {
    /// Every row, however long it takes: the command line's behaviour.
    pub fn unlimited() -> Self {
        Self {
            max_rows: usize::MAX,
            timeout: None,
        }
    }
}

/// A query that has been translated and analysed but not run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedQuery {
    /// The SQL that will be sent.
    pub sql: String,
    /// The input used Kairo's short form and was expanded.
    pub translated: bool,
    pub analysis: SqlAnalysis,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryOutcome {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: usize,
    /// Rows changed, for statements that change rows.
    pub rows_affected: Option<u64>,
    /// More rows matched than were kept.
    pub truncated: bool,
    pub elapsed_ms: u64,
    pub statement_count: usize,
    /// Exactly what was sent to the database.
    pub executed_sql: String,
    pub translated: bool,
    pub risk: Risk,
}

/// Expands Kairo's short query form: `from users where age > 18` becomes
/// `SELECT * from users where age > 18`. Anything else is returned unchanged.
pub fn translate_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.to_lowercase().starts_with("from ") {
        format!("SELECT * {trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Translates and analyses a query, rejecting input that should never run.
pub fn prepare(input: &str) -> Result<PreparedQuery> {
    let sql = translate_query(input);
    let analysis = safety::analyze(&sql);

    if analysis.statements.is_empty() {
        return Err(KairoError::invalid_input("There is no SQL to run."));
    }

    // Statements run on pooled connections. A transaction left open would
    // hold locks on a connection nobody is using.
    if analysis.unbalanced_transaction {
        return Err(KairoError::invalid_input(
            "This script opens a transaction and does not close it.",
        )
        .with_hint("End it with COMMIT or ROLLBACK in the same script."));
    }

    Ok(PreparedQuery {
        translated: sql != input.trim(),
        sql,
        analysis,
    })
}

/// Runs a prepared query. Whether it is allowed to run is the caller's
/// decision; see [`safety::require_acknowledgement`].
pub async fn run_prepared(
    conn: &Connection,
    prepared: &PreparedQuery,
    options: &QueryOptions,
) -> Result<QueryOutcome> {
    let run_options = RunOptions {
        max_rows: options.max_rows.max(1),
        timeout: options.timeout,
        single_statement: prepared.analysis.statements.len() == 1,
        returns_rows: prepared.analysis.returns_rows,
    };

    let started = Instant::now();
    let raw = conn.run_sql(&prepared.sql, &run_options).await?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    Ok(QueryOutcome {
        row_count: raw.rows.len(),
        // A driver reports a stale count for statements that change nothing,
        // so only report it when something could have changed.
        rows_affected: (prepared.analysis.risk > Risk::Read).then_some(raw.rows_affected),
        truncated: raw.truncated,
        elapsed_ms,
        statement_count: raw.statement_count.max(1),
        executed_sql: prepared.sql.clone(),
        translated: prepared.translated,
        risk: prepared.analysis.risk,
        columns: raw.columns,
        rows: raw.rows,
    })
}

/// Translates, analyses and runs in one step, without asking for
/// confirmation. This is what the command line does.
pub async fn run_query(
    conn: &Connection,
    input: &str,
    options: &QueryOptions,
) -> Result<QueryOutcome> {
    let prepared = prepare(input)?;
    run_prepared(conn, &prepared, options).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn short_form_is_expanded() {
        assert_eq!(translate_query("from users"), "SELECT * from users");
        assert_eq!(
            translate_query("  FROM users where age > 18  "),
            "SELECT * FROM users where age > 18"
        );
        assert_eq!(translate_query("SELECT 1"), "SELECT 1");
        // `fromage` is a table name, not the keyword.
        assert_eq!(translate_query("fromage"), "fromage");
    }

    #[test]
    fn prepare_reports_translation_and_risk() {
        let prepared = prepare("from users").unwrap();
        assert!(prepared.translated);
        assert_eq!(prepared.sql, "SELECT * from users");
        assert_eq!(prepared.analysis.risk, Risk::Read);

        let prepared = prepare("  DROP TABLE users ").unwrap();
        assert!(!prepared.translated);
        assert_eq!(prepared.analysis.risk, Risk::Destructive);
    }

    #[test]
    fn prepare_rejects_empty_and_unclosed_transactions() {
        assert_eq!(prepare("   ").unwrap_err().kind, ErrorKind::InvalidInput);
        assert_eq!(
            prepare("-- nothing").unwrap_err().kind,
            ErrorKind::InvalidInput
        );
        let err = prepare("BEGIN; DELETE FROM t").unwrap_err();
        assert!(err.message.contains("does not close it"));
    }
}
