//! PostgreSQL adapter.
//!
//! Everything is scoped to `current_schema()`, the first schema on the
//! connection's search path (normally `public`).

use super::model::{
    ColumnInfo, ConnectionInfo, Engine, ForeignKeyInfo, IndexInfo, PageRequest, RawOutcome,
    ResultColumn, RowPage, TableDetail, TableKind, TableSummary,
};
use super::sql::{LIKE_ESCAPE, contains_pattern, kairo_type_for_postgres, quote_ident};
use super::target::{ConnectionTarget, PostgresTarget};
use super::value::{Value, ValueKind};
use super::{BatchFailure, RunOptions};
use crate::error::{ErrorKind, KairoError, Result};
use futures_util::TryStreamExt;
use sqlx::postgres::{PgConnection, PgPool, PgPoolOptions, PgRow, PgValueFormat};
use sqlx::{
    AssertSqlSafe, Column, Connection, Either, Executor, Row, SqlSafeStr, Statement, TypeInfo,
    ValueRef,
};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);

/// Separates list items packed into one text column by the catalog queries.
const UNIT_SEPARATOR: char = '\u{1f}';

pub(crate) async fn open(target: &PostgresTarget) -> Result<(PgPool, ConnectionInfo)> {
    // Connect once directly before building a pool. A pool keeps retrying a
    // refused connection until it times out, which hides the real reason.
    let attempt =
        tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect_with(&target.options));
    let mut probe = attempt.await.map_err(|_| {
        KairoError::new(
            ErrorKind::Timeout,
            format!("Timed out connecting to {}:{}.", target.host, target.port),
        )
        .with_hint("Check the host, the port, and any firewall or VPN in between.")
    })??;

    let settings = sqlx::query(
        "SELECT current_setting('server_version') AS version, \
                current_setting('transaction_read_only') AS read_only",
    )
    .fetch_one(&mut probe)
    .await;
    let _ = probe.close().await;
    let settings = settings?;

    let version: String = settings.try_get("version")?;
    let read_only: String = settings.try_get("read_only")?;

    let pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(CONNECT_TIMEOUT)
        .connect_lazy_with(target.options.clone());

    let info = ConnectionInfo {
        engine: Engine::Postgres,
        name: target.database.clone(),
        location: target.display_url(),
        host: Some(format!("{}:{}", target.host, target.port)),
        database: Some(target.database.clone()),
        username: Some(target.username.clone()),
        // "16.2 (Debian 16.2-1.pgdg120+2)" reads better as "PostgreSQL 16.2".
        server_version: format!(
            "PostgreSQL {}",
            version.split_whitespace().next().unwrap_or(&version)
        ),
        read_only: read_only == "on",
        workspace_key: ConnectionTarget::Postgres(target.clone()).workspace_key(),
        size_bytes: None,
    };

    Ok((pool, info))
}

fn no_such_table(table: &str) -> KairoError {
    KairoError::not_found(format!("There is no table or view named '{table}'."))
}

fn kind_of(relkind: &str) -> TableKind {
    match relkind {
        "v" | "m" => TableKind::View,
        _ => TableKind::Table,
    }
}

fn split_list(packed: &str) -> Vec<String> {
    if packed.is_empty() {
        Vec::new()
    } else {
        packed.split(UNIT_SEPARATOR).map(str::to_string).collect()
    }
}

pub(crate) async fn list_tables(pool: &PgPool) -> Result<Vec<TableSummary>> {
    let rows = sqlx::query(
        "SELECT c.relname::text AS name, \
                c.relkind::text AS kind, \
                c.reltuples::float8 AS estimate, \
                (SELECT COUNT(*) FROM pg_attribute a \
                  WHERE a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped) AS column_count \
           FROM pg_class c \
           JOIN pg_namespace n ON n.oid = c.relnamespace \
          WHERE n.nspname = current_schema() AND c.relkind IN ('r', 'p', 'v', 'm') \
          ORDER BY lower(c.relname), c.relname",
    )
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            let relkind: String = row.try_get("kind")?;
            let kind = kind_of(&relkind);
            let estimate: f64 = row.try_get("estimate")?;
            let column_count: i64 = row.try_get("column_count")?;
            // The planner's estimate is negative until a table is analysed.
            let row_count =
                (kind == TableKind::Table && estimate >= 0.0).then_some(estimate as i64);
            Ok(TableSummary {
                name: row.try_get("name")?,
                kind,
                column_count: column_count as u32,
                row_count,
                row_count_estimated: row_count.is_some(),
            })
        })
        .collect()
}

pub(crate) async fn count_rows(pool: &PgPool, table: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {}", quote_ident(table));
    Ok(sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(pool)
        .await?)
}

async fn columns_of(conn: &mut PgConnection, table: &str) -> Result<Vec<ColumnInfo>> {
    let rows = sqlx::query(
        "SELECT a.attname::text AS name, \
                format_type(a.atttypid, a.atttypmod) AS data_type, \
                NOT a.attnotnull AS nullable, \
                pg_get_expr(d.adbin, d.adrelid) AS default_value, \
                COALESCE(( \
                    SELECT k.ord::int4 \
                      FROM pg_index i, unnest(i.indkey::int2[]) WITH ORDINALITY AS k(attnum, ord) \
                     WHERE i.indrelid = c.oid AND i.indisprimary AND k.attnum = a.attnum \
                ), 0) AS pk_position \
           FROM pg_attribute a \
           JOIN pg_class c ON c.oid = a.attrelid \
           JOIN pg_namespace n ON n.oid = c.relnamespace \
           LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
          WHERE n.nspname = current_schema() AND c.relname = $1 \
            AND c.relkind IN ('r', 'p', 'v', 'm') \
            AND a.attnum > 0 AND NOT a.attisdropped \
          ORDER BY a.attnum",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await?;

    rows.iter()
        .map(|row| {
            let data_type: String = row.try_get("data_type")?;
            let pk_position: i32 = row.try_get("pk_position")?;
            Ok(ColumnInfo {
                name: row.try_get("name")?,
                kairo_type: kairo_type_for_postgres(&data_type).to_string(),
                data_type,
                nullable: row.try_get("nullable")?,
                default_value: row.try_get("default_value")?,
                primary_key_position: pk_position.max(0) as u32,
            })
        })
        .collect()
}

pub(crate) async fn describe_table(pool: &PgPool, table: &str) -> Result<TableDetail> {
    let mut conn = pool.acquire().await?;

    let relkind: Option<String> = sqlx::query_scalar(
        "SELECT c.relkind::text \
           FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
          WHERE n.nspname = current_schema() AND c.relname = $1 \
            AND c.relkind IN ('r', 'p', 'v', 'm')",
    )
    .bind(table)
    .fetch_optional(&mut *conn)
    .await?;
    let kind = kind_of(&relkind.ok_or_else(|| no_such_table(table))?);

    let columns = columns_of(&mut conn, table).await?;
    let indexes = indexes_of(&mut conn, table).await?;
    let foreign_keys = foreign_keys_of(&mut conn, table).await?;

    let create_sql = match kind {
        TableKind::View => {
            let body: Option<String> = sqlx::query_scalar(
                "SELECT pg_get_viewdef(c.oid, true) \
                   FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                  WHERE n.nspname = current_schema() AND c.relname = $1",
            )
            .bind(table)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
            format!(
                "CREATE VIEW {} AS\n{}",
                quote_ident(table),
                body.unwrap_or_default().trim()
            )
        }
        TableKind::Table => reconstruct_create(table, &columns, &indexes, &foreign_keys),
    };

    Ok(TableDetail {
        name: table.to_string(),
        kind,
        columns,
        indexes,
        foreign_keys,
        create_sql,
    })
}

/// PostgreSQL does not keep the original `CREATE TABLE`. This rebuilds an
/// equivalent one from the catalog and says so.
fn reconstruct_create(
    table: &str,
    columns: &[ColumnInfo],
    indexes: &[IndexInfo],
    foreign_keys: &[ForeignKeyInfo],
) -> String {
    let mut lines: Vec<String> = columns
        .iter()
        .map(|column| {
            let mut line = format!("  {} {}", quote_ident(&column.name), column.data_type);
            if !column.nullable {
                line.push_str(" NOT NULL");
            }
            if let Some(default) = &column.default_value {
                line.push_str(&format!(" DEFAULT {default}"));
            }
            line
        })
        .collect();

    let mut primary: Vec<&ColumnInfo> = columns
        .iter()
        .filter(|c| c.primary_key_position > 0)
        .collect();
    primary.sort_by_key(|c| c.primary_key_position);
    if !primary.is_empty() {
        let names: Vec<String> = primary.iter().map(|c| quote_ident(&c.name)).collect();
        lines.push(format!("  PRIMARY KEY ({})", names.join(", ")));
    }

    let quoted = |names: &[String]| -> String {
        names
            .iter()
            .map(|n| quote_ident(n))
            .collect::<Vec<_>>()
            .join(", ")
    };

    for key in foreign_keys {
        lines.push(format!(
            "  FOREIGN KEY ({}) REFERENCES {} ({})",
            quoted(&key.columns),
            quote_ident(&key.references_table),
            quoted(&key.references_columns)
        ));
    }

    let mut sql = format!(
        "-- Reconstructed from the catalog by Kairo\nCREATE TABLE {} (\n{}\n);",
        quote_ident(table),
        lines.join(",\n")
    );

    for index in indexes.iter().filter(|i| !i.primary) {
        if let Some(definition) = &index.definition {
            sql.push_str("\n\n");
            sql.push_str(definition);
            sql.push(';');
        }
    }
    sql
}

async fn indexes_of(conn: &mut PgConnection, table: &str) -> Result<Vec<IndexInfo>> {
    let rows = sqlx::query(
        "SELECT i.relname::text AS name, \
                ix.indisunique AS is_unique, \
                ix.indisprimary AS is_primary, \
                pg_get_indexdef(ix.indexrelid) AS definition, \
                array_to_string(ARRAY( \
                    SELECT pg_get_indexdef(ix.indexrelid, k.ord::int4, true) \
                      FROM generate_series(1, ix.indnkeyatts::int4) AS k(ord) \
                     ORDER BY k.ord \
                ), chr(31)) AS columns, \
                EXISTS ( \
                    SELECT 1 FROM pg_constraint con \
                     WHERE con.conindid = ix.indexrelid AND con.contype = 'u' \
                ) AS from_constraint \
           FROM pg_index ix \
           JOIN pg_class i ON i.oid = ix.indexrelid \
           JOIN pg_class c ON c.oid = ix.indrelid \
           JOIN pg_namespace n ON n.oid = c.relnamespace \
          WHERE n.nspname = current_schema() AND c.relname = $1 \
          ORDER BY i.relname",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await?;

    rows.iter()
        .map(|row| {
            let primary: bool = row.try_get("is_primary")?;
            let from_constraint: bool = row.try_get("from_constraint")?;
            let columns: String = row.try_get("columns")?;
            Ok(IndexInfo {
                name: row.try_get("name")?,
                unique: row.try_get("is_unique")?,
                primary,
                columns: split_list(&columns),
                origin: if primary {
                    "primary key"
                } else if from_constraint {
                    "unique constraint"
                } else {
                    "index"
                }
                .to_string(),
                definition: row.try_get("definition")?,
            })
        })
        .collect()
}

fn referential_action(code: &str) -> Option<String> {
    let action = match code {
        "r" => "RESTRICT",
        "c" => "CASCADE",
        "n" => "SET NULL",
        "d" => "SET DEFAULT",
        _ => "NO ACTION",
    };
    Some(action.to_string())
}

async fn foreign_keys_of(conn: &mut PgConnection, table: &str) -> Result<Vec<ForeignKeyInfo>> {
    let rows = sqlx::query(
        "SELECT con.conname::text AS name, \
                array_to_string(ARRAY( \
                    SELECT att.attname::text \
                      FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord) \
                      JOIN pg_attribute att \
                        ON att.attrelid = con.conrelid AND att.attnum = k.attnum \
                     ORDER BY k.ord \
                ), chr(31)) AS columns, \
                ref.relname::text AS ref_table, \
                array_to_string(ARRAY( \
                    SELECT att.attname::text \
                      FROM unnest(con.confkey) WITH ORDINALITY AS k(attnum, ord) \
                      JOIN pg_attribute att \
                        ON att.attrelid = con.confrelid AND att.attnum = k.attnum \
                     ORDER BY k.ord \
                ), chr(31)) AS ref_columns, \
                con.confupdtype::text AS on_update, \
                con.confdeltype::text AS on_delete \
           FROM pg_constraint con \
           JOIN pg_class c ON c.oid = con.conrelid \
           JOIN pg_namespace n ON n.oid = c.relnamespace \
           JOIN pg_class ref ON ref.oid = con.confrelid \
          WHERE con.contype = 'f' AND n.nspname = current_schema() AND c.relname = $1 \
          ORDER BY con.conname",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await?;

    rows.iter()
        .map(|row| {
            let columns: String = row.try_get("columns")?;
            let ref_columns: String = row.try_get("ref_columns")?;
            let on_update: String = row.try_get("on_update")?;
            let on_delete: String = row.try_get("on_delete")?;
            Ok(ForeignKeyInfo {
                name: row.try_get("name")?,
                columns: split_list(&columns),
                references_table: row.try_get("ref_table")?,
                references_columns: split_list(&ref_columns),
                on_update: referential_action(&on_update),
                on_delete: referential_action(&on_delete),
            })
        })
        .collect()
}

fn kind_for(kairo_type: &str) -> ValueKind {
    match kairo_type {
        "int" => ValueKind::Int,
        "float" => ValueKind::Float,
        "bool" => ValueKind::Bool,
        "blob" => ValueKind::Blob,
        _ => ValueKind::Text,
    }
}

pub(crate) async fn fetch_rows(
    pool: &PgPool,
    table: &str,
    request: &PageRequest,
) -> Result<RowPage> {
    let mut conn = pool.acquire().await?;

    let columns = columns_of(&mut conn, table).await?;
    if columns.is_empty() {
        return Err(no_such_table(table));
    }

    let limit = request.clamped_limit();
    let offset = request.offset;
    let table_sql = quote_ident(table);

    // Every column is cast to text in SQL, so any type can be shown without a
    // decoder for it. Binary columns are reduced to their size.
    let select_list: Vec<String> = columns
        .iter()
        .map(|c| {
            let name = quote_ident(&c.name);
            if c.kairo_type == "blob" {
                format!("octet_length({name})::text AS {name}")
            } else {
                format!("{name}::text AS {name}")
            }
        })
        .collect();

    let pattern = request.active_filter().map(contains_pattern);
    let where_sql = if pattern.is_some() {
        let tests: Vec<String> = columns
            .iter()
            .filter(|c| c.kairo_type != "blob")
            .map(|c| {
                format!(
                    "{}::text ILIKE $1 ESCAPE '{LIKE_ESCAPE}'",
                    quote_ident(&c.name)
                )
            })
            .collect();
        if tests.is_empty() {
            " WHERE false".to_string()
        } else {
            format!(" WHERE ({})", tests.join(" OR "))
        }
    } else {
        String::new()
    };

    let order_sql = match &request.sort {
        Some(sort) => {
            let column = columns
                .iter()
                .find(|c| c.name == sort.column)
                .ok_or_else(|| {
                    KairoError::invalid_input(format!(
                        "'{}' is not a column of '{table}'.",
                        sort.column
                    ))
                })?;
            let direction = if sort.descending { "DESC" } else { "ASC" };
            format!(" ORDER BY {} {direction}", quote_ident(&column.name))
        }
        None => {
            // Without an order, pages are not guaranteed to be stable.
            let mut primary: Vec<&ColumnInfo> = columns
                .iter()
                .filter(|c| c.primary_key_position > 0)
                .collect();
            primary.sort_by_key(|c| c.primary_key_position);
            if primary.is_empty() {
                String::new()
            } else {
                let names: Vec<String> = primary.iter().map(|c| quote_ident(&c.name)).collect();
                format!(" ORDER BY {}", names.join(", "))
            }
        }
    };

    let count_sql = format!("SELECT COUNT(*) FROM {table_sql}{where_sql}");
    let page_sql = format!(
        "SELECT {} FROM {table_sql}{where_sql}{order_sql} LIMIT {limit} OFFSET {offset}",
        select_list.join(", ")
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(AssertSqlSafe(count_sql));
    let mut page_query = sqlx::query(AssertSqlSafe(page_sql.clone()));
    if let Some(pattern) = &pattern {
        count_query = count_query.bind(pattern);
        page_query = page_query.bind(pattern);
    }

    let total_rows = count_query.fetch_one(&mut *conn).await?;
    let rows = page_query.fetch_all(&mut *conn).await?;

    let kinds: Vec<ValueKind> = columns.iter().map(|c| kind_for(&c.kairo_type)).collect();
    let rows = rows
        .iter()
        .map(|row| {
            kinds
                .iter()
                .enumerate()
                .map(
                    |(index, kind)| match row.try_get::<Option<String>, _>(index) {
                        Ok(Some(text)) => match kind {
                            ValueKind::Bool => Value::bool(text == "true"),
                            ValueKind::Blob => Value::blob(text.parse().unwrap_or(0)),
                            ValueKind::Text => Value::text(text),
                            numeric => Value::number_text(*numeric, text),
                        },
                        _ => Value::null(),
                    },
                )
                .collect()
        })
        .collect();

    Ok(RowPage {
        columns: columns
            .into_iter()
            .map(|c| ResultColumn {
                name: c.name,
                data_type: c.data_type,
            })
            .collect(),
        rows,
        total_rows,
        offset,
        limit,
        sql: page_sql,
    })
}

/// Shapes a value the server sent as text, using the column type for its kind.
fn text_value(type_name: &str, text: &str) -> Value {
    match type_name {
        "BOOL" => Value::bool(text == "t" || text == "true"),
        "INT2" | "INT4" | "INT8" | "OID" => Value::number_text(ValueKind::Int, text),
        "FLOAT4" | "FLOAT8" | "NUMERIC" | "MONEY" => Value::number_text(ValueKind::Float, text),
        // bytea arrives as `\x` followed by two hex digits per byte.
        "BYTEA" => Value::blob(
            text.strip_prefix("\\x")
                .map_or(text.len(), |hex| hex.len() / 2),
        ),
        _ => Value::text(text),
    }
}

/// Fallback for a value in binary form. The simple query protocol used for
/// scripts returns text, so this only covers the common types.
fn binary_value(row: &PgRow, index: usize, type_name: &str) -> Value {
    let decoded = match type_name {
        "BOOL" => row.try_get_unchecked::<bool, _>(index).map(Value::bool),
        "INT2" => row
            .try_get_unchecked::<i16, _>(index)
            .map(|n| Value::int(n.into())),
        "INT4" => row
            .try_get_unchecked::<i32, _>(index)
            .map(|n| Value::int(n.into())),
        "INT8" => row.try_get_unchecked::<i64, _>(index).map(Value::int),
        "FLOAT4" => row
            .try_get_unchecked::<f32, _>(index)
            .map(|n| Value::float(n.into())),
        "FLOAT8" => row.try_get_unchecked::<f64, _>(index).map(Value::float),
        "BYTEA" => row
            .try_get_unchecked::<&[u8], _>(index)
            .map(|bytes| Value::blob(bytes.len())),
        _ => row.try_get_unchecked::<String, _>(index).map(Value::text),
    };
    decoded.unwrap_or_else(|_| Value::text(format!("<{} value>", type_name.to_lowercase())))
}

fn decode_row(row: &PgRow) -> Vec<Value> {
    row.columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let Ok(raw) = row.try_get_raw(index) else {
                return Value::null();
            };
            if raw.is_null() {
                return Value::null();
            }
            let type_name = column.type_info().name().to_ascii_uppercase();
            match raw.format() {
                PgValueFormat::Text => match raw.as_str() {
                    Ok(text) => text_value(&type_name, text),
                    Err(_) => Value::blob(raw.as_bytes().map_or(0, <[u8]>::len)),
                },
                PgValueFormat::Binary => binary_value(row, index, &type_name),
            }
        })
        .collect()
}

/// A type the driver could not name is reported as `?`.
fn display_type(name: &str) -> String {
    if name == "?" {
        String::new()
    } else {
        name.to_ascii_lowercase()
    }
}

pub(crate) async fn run_sql(pool: &PgPool, sql: &str, options: &RunOptions) -> Result<RawOutcome> {
    let mut conn = pool.acquire().await?;

    if let Some(timeout) = options.timeout {
        // An integer formatted by us; there is no user text in this statement.
        let limit = format!("SET statement_timeout = {}", timeout.as_millis().max(1));
        sqlx::raw_sql(AssertSqlSafe(limit))
            .execute(&mut *conn)
            .await?;
    }

    let result = run_on(&mut conn, sql, options).await;

    if result.is_err() {
        // A failure inside BEGIN leaves the session in an aborted transaction.
        let _ = sqlx::raw_sql("ROLLBACK").execute(&mut *conn).await;
    }
    if options.timeout.is_some() {
        let _ = sqlx::raw_sql("RESET statement_timeout")
            .execute(&mut *conn)
            .await;
    }

    result
}

async fn run_on(conn: &mut PgConnection, sql: &str, options: &RunOptions) -> Result<RawOutcome> {
    let mut outcome = RawOutcome::default();
    let mut set_finished = false;

    {
        // `raw_sql` uses the simple query protocol: scripts with several
        // statements work, and every value comes back as text.
        let mut stream = sqlx::raw_sql(AssertSqlSafe(sql.to_string())).fetch_many(&mut *conn);
        while let Some(item) = stream.try_next().await? {
            match item {
                Either::Left(done) => {
                    outcome.rows_affected += done.rows_affected();
                    outcome.statement_count += 1;
                    set_finished = true;
                }
                Either::Right(row) => {
                    if set_finished {
                        outcome.rows.clear();
                        outcome.columns.clear();
                        outcome.truncated = false;
                        set_finished = false;
                    }
                    if outcome.columns.is_empty() {
                        outcome.columns = row
                            .columns()
                            .iter()
                            .map(|column| ResultColumn {
                                name: column.name().to_string(),
                                data_type: display_type(column.type_info().name()),
                            })
                            .collect();
                    }
                    if outcome.rows.len() < options.max_rows {
                        outcome.rows.push(decode_row(&row));
                    } else {
                        outcome.truncated = true;
                        if options.single_statement {
                            break;
                        }
                    }
                }
            }
        }
    }

    if outcome.truncated && options.single_statement {
        outcome.statement_count = 1;
    }

    if outcome.columns.is_empty()
        && options.single_statement
        && options.returns_rows
        && let Ok(statement) = (&mut *conn)
            .prepare(AssertSqlSafe(sql.to_string()).into_sql_str())
            .await
    {
        outcome.columns = statement
            .columns()
            .iter()
            .map(|column| ResultColumn {
                name: column.name().to_string(),
                data_type: display_type(column.type_info().name()),
            })
            .collect();
    }

    Ok(outcome)
}

pub(crate) async fn execute_batch(
    pool: &PgPool,
    statements: &[String],
) -> std::result::Result<(), BatchFailure> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|err| BatchFailure::before_start(err.into()))?;

    for (index, statement) in statements.iter().enumerate() {
        sqlx::raw_sql(AssertSqlSafe(statement.clone()))
            .execute(&mut *tx)
            .await
            .map_err(|err| BatchFailure {
                index: Some(index),
                error: err.into(),
            })?;
    }

    tx.commit()
        .await
        .map_err(|err| BatchFailure::before_start(err.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_values_take_their_kind_from_the_column_type() {
        assert_eq!(text_value("BOOL", "t"), Value::bool(true));
        assert_eq!(text_value("BOOL", "f"), Value::bool(false));
        assert_eq!(
            text_value("INT8", "9007199254740993").text,
            "9007199254740993"
        );
        assert_eq!(text_value("INT8", "1").kind, ValueKind::Int);
        assert_eq!(text_value("NUMERIC", "12.50").kind, ValueKind::Float);
        assert_eq!(text_value("NUMERIC", "12.50").text, "12.50");
        assert_eq!(text_value("BYTEA", "\\xdeadbeef"), Value::blob(4));
        assert_eq!(text_value("UUID", "a-b").kind, ValueKind::Text);
        assert_eq!(text_value("?", "anything").kind, ValueKind::Text);
    }

    #[test]
    fn reconstructed_ddl_lists_columns_keys_and_indexes() {
        let column = |name: &str, data_type: &str, nullable: bool, pk: u32| ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            kairo_type: kairo_type_for_postgres(data_type).into(),
            nullable,
            default_value: None,
            primary_key_position: pk,
        };
        let sql = reconstruct_create(
            "orders",
            &[
                column("id", "integer", false, 1),
                column("user id", "integer", true, 0),
            ],
            &[IndexInfo {
                name: "orders_user_idx".into(),
                unique: false,
                primary: false,
                columns: vec!["user id".into()],
                origin: "index".into(),
                definition: Some("CREATE INDEX orders_user_idx ON orders (\"user id\")".into()),
            }],
            &[ForeignKeyInfo {
                name: Some("fk".into()),
                columns: vec!["user id".into()],
                references_table: "users".into(),
                references_columns: vec!["id".into()],
                on_update: None,
                on_delete: None,
            }],
        );
        assert!(sql.contains("CREATE TABLE \"orders\" ("), "{sql}");
        assert!(sql.contains("\"id\" integer NOT NULL"), "{sql}");
        assert!(sql.contains("PRIMARY KEY (\"id\")"), "{sql}");
        assert!(
            sql.contains("FOREIGN KEY (\"user id\") REFERENCES \"users\" (\"id\")"),
            "{sql}"
        );
        assert!(
            sql.ends_with("CREATE INDEX orders_user_idx ON orders (\"user id\");"),
            "{sql}"
        );
    }
}
