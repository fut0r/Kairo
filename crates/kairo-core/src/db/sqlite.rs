//! SQLite adapter.

use super::model::{
    ColumnInfo, ConnectionInfo, Engine, ForeignKeyInfo, IndexInfo, PageRequest, RawOutcome,
    ResultColumn, RowPage, TableDetail, TableKind, TableSummary,
};
use super::sql::{LIKE_ESCAPE, contains_pattern, kairo_type_for_sqlite, quote_ident};
use super::target::SqliteTarget;
use super::value::Value;
use super::{BatchFailure, RunOptions};
use crate::error::{KairoError, Result};
use futures_util::TryStreamExt;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteConnection, SqlitePool, SqlitePoolOptions, SqliteRow,
};
use sqlx::{
    AssertSqlSafe, Column, Either, Executor, Row, SqlSafeStr, Statement, TypeInfo, ValueRef,
};
use std::time::{Duration, Instant};

/// How long listing tables may spend counting rows before it stops counting.
const COUNT_BUDGET: Duration = Duration::from_millis(1500);

/// How many VM instructions SQLite runs between deadline checks.
const PROGRESS_INTERVAL: i32 = 1000;

pub(crate) async fn open(target: &SqliteTarget) -> Result<(SqlitePool, ConnectionInfo)> {
    target.check()?;

    // The path is passed as a filename, never spliced into a URL, so spaces,
    // `?` and `#` in file names are safe.
    let options = SqliteConnectOptions::new()
        .filename(&target.path)
        .create_if_missing(target.create)
        .read_only(target.read_only)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;

    // SQLite opens any file lazily. Reading the catalog is what proves the
    // file really is a database.
    let version = match probe(&pool).await {
        Ok(version) => version,
        Err(err) => {
            pool.close().await;
            return Err(err);
        }
    };

    let location = target.display_path();
    let metadata = std::fs::metadata(&target.path).ok();
    let name = target
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| location.clone());

    let info = ConnectionInfo {
        engine: Engine::Sqlite,
        name,
        workspace_key: format!("sqlite:{location}"),
        location,
        host: None,
        database: None,
        username: None,
        server_version: format!("SQLite {version}"),
        read_only: target.read_only
            || metadata
                .as_ref()
                .is_some_and(|m| m.permissions().readonly()),
        size_bytes: metadata.map(|m| m.len()),
    };

    Ok((pool, info))
}

async fn probe(pool: &SqlitePool) -> Result<String> {
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(pool)
        .await?;
    sqlx::query("SELECT COUNT(*) FROM sqlite_master")
        .fetch_one(pool)
        .await?;
    Ok(version)
}

/// Makes every statement on `conn` fail with an interrupt once `deadline`
/// passes. Must be paired with [`clear_deadline`].
async fn set_deadline(conn: &mut SqliteConnection, deadline: Instant) -> Result<()> {
    let mut handle = conn.lock_handle().await?;
    handle.set_progress_handler(PROGRESS_INTERVAL, move || Instant::now() < deadline);
    Ok(())
}

async fn clear_deadline(conn: &mut SqliteConnection) {
    if let Ok(mut handle) = conn.lock_handle().await {
        handle.remove_progress_handler();
    }
}

fn kind_of(type_name: &str) -> TableKind {
    if type_name == "view" {
        TableKind::View
    } else {
        TableKind::Table
    }
}

pub(crate) async fn list_tables(pool: &SqlitePool) -> Result<Vec<TableSummary>> {
    let mut conn = pool.acquire().await?;

    let rows = sqlx::query(
        "SELECT name, type FROM sqlite_master \
         WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' \
         ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut tables = Vec::with_capacity(rows.len());
    for row in &rows {
        let name: String = row.try_get("name")?;
        let type_name: String = row.try_get("type")?;
        let column_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?1)")
            .bind(&name)
            .fetch_one(&mut *conn)
            .await?;
        tables.push(TableSummary {
            name,
            kind: kind_of(&type_name),
            column_count: column_count as u32,
            row_count: None,
            row_count_estimated: false,
        });
    }

    // Counting scans each table. On a large file that can take a while, so
    // counting shares one time budget; tables past it show no count.
    let deadline = Instant::now() + COUNT_BUDGET;
    set_deadline(&mut conn, deadline).await?;
    for table in &mut tables {
        if Instant::now() >= deadline {
            break;
        }
        table.row_count = count_on(&mut conn, &table.name).await.ok();
    }
    clear_deadline(&mut conn).await;

    Ok(tables)
}

async fn count_on(conn: &mut SqliteConnection, table: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {}", quote_ident(table));
    Ok(sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(&mut *conn)
        .await?)
}

pub(crate) async fn count_rows(pool: &SqlitePool, table: &str) -> Result<i64> {
    let mut conn = pool.acquire().await?;
    count_on(&mut conn, table).await
}

async fn columns_of(conn: &mut SqliteConnection, table: &str) -> Result<Vec<ColumnInfo>> {
    let rows = sqlx::query(
        "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info(?1) ORDER BY cid",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await?;

    rows.iter()
        .map(|row| {
            let data_type: String = row.try_get("type")?;
            let not_null: i64 = row.try_get("notnull")?;
            let pk: i64 = row.try_get("pk")?;
            Ok(ColumnInfo {
                name: row.try_get("name")?,
                kairo_type: kairo_type_for_sqlite(&data_type).to_string(),
                data_type,
                nullable: not_null == 0,
                default_value: row.try_get("dflt_value")?,
                primary_key_position: pk as u32,
            })
        })
        .collect()
}

fn no_such_table(table: &str) -> KairoError {
    KairoError::not_found(format!("There is no table or view named '{table}'."))
}

pub(crate) async fn describe_table(pool: &SqlitePool, table: &str) -> Result<TableDetail> {
    let mut conn = pool.acquire().await?;

    let master = sqlx::query(
        "SELECT name, type, sql FROM sqlite_master \
         WHERE type IN ('table', 'view') AND name = ?1 COLLATE NOCASE",
    )
    .bind(table)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| no_such_table(table))?;

    // Use the catalog's spelling from here on.
    let name: String = master.try_get("name")?;
    let type_name: String = master.try_get("type")?;
    let table_sql: Option<String> = master.try_get("sql")?;

    let columns = columns_of(&mut conn, &name).await?;
    let (indexes, index_sql) = indexes_of(&mut conn, &name).await?;
    let foreign_keys = foreign_keys_of(&mut conn, &name).await?;

    let mut create_sql = table_sql.unwrap_or_default();
    if !create_sql.is_empty() && !create_sql.trim_end().ends_with(';') {
        create_sql.push(';');
    }
    for sql in index_sql {
        create_sql.push_str("\n\n");
        create_sql.push_str(&sql);
        create_sql.push(';');
    }

    Ok(TableDetail {
        name,
        kind: kind_of(&type_name),
        columns,
        indexes,
        foreign_keys,
        create_sql,
    })
}

async fn indexes_of(
    conn: &mut SqliteConnection,
    table: &str,
) -> Result<(Vec<IndexInfo>, Vec<String>)> {
    let rows = sqlx::query("SELECT name, \"unique\", origin FROM pragma_index_list(?1)")
        .bind(table)
        .fetch_all(&mut *conn)
        .await?;

    let mut indexes = Vec::with_capacity(rows.len());
    let mut definitions = Vec::new();

    for row in &rows {
        let name: String = row.try_get("name")?;
        let unique: i64 = row.try_get("unique")?;
        let origin: String = row.try_get("origin")?;

        // An index on an expression has no column name.
        let columns: Vec<Option<String>> =
            sqlx::query_scalar("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
                .bind(&name)
                .fetch_all(&mut *conn)
                .await?;

        let definition: Option<String> =
            sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1")
                .bind(&name)
                .fetch_optional(&mut *conn)
                .await?
                .flatten();

        if let Some(sql) = &definition {
            definitions.push(sql.clone());
        }

        indexes.push(IndexInfo {
            name,
            unique: unique != 0,
            primary: origin == "pk",
            columns: columns
                .into_iter()
                .map(|c| c.unwrap_or_else(|| "(expression)".to_string()))
                .collect(),
            origin: match origin.as_str() {
                "pk" => "primary key",
                "u" => "unique constraint",
                _ => "index",
            }
            .to_string(),
            definition,
        });
    }

    indexes.sort_by_key(|index| index.name.to_lowercase());
    Ok((indexes, definitions))
}

async fn foreign_keys_of(conn: &mut SqliteConnection, table: &str) -> Result<Vec<ForeignKeyInfo>> {
    let rows = sqlx::query(
        "SELECT id, \"table\", \"from\", \"to\", on_update, on_delete \
         FROM pragma_foreign_key_list(?1) ORDER BY id, seq",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await?;

    let mut keys: Vec<(i64, ForeignKeyInfo)> = Vec::new();
    for row in &rows {
        let id: i64 = row.try_get("id")?;
        let from: String = row.try_get("from")?;
        // NULL when the key points at the other table's primary key.
        let to: Option<String> = row.try_get("to")?;

        match keys.last_mut() {
            Some((last_id, key)) if *last_id == id => {
                key.columns.push(from);
                key.references_columns.extend(to);
            }
            _ => keys.push((
                id,
                ForeignKeyInfo {
                    name: None,
                    columns: vec![from],
                    references_table: row.try_get("table")?,
                    references_columns: to.into_iter().collect(),
                    on_update: row.try_get("on_update")?,
                    on_delete: row.try_get("on_delete")?,
                },
            )),
        }
    }

    Ok(keys.into_iter().map(|(_, key)| key).collect())
}

pub(crate) async fn fetch_rows(
    pool: &SqlitePool,
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

    let pattern = request.active_filter().map(contains_pattern);
    let where_sql = if pattern.is_some() {
        let tests: Vec<String> = columns
            .iter()
            .map(|c| {
                format!(
                    "CAST({} AS TEXT) LIKE ?1 ESCAPE '{LIKE_ESCAPE}'",
                    quote_ident(&c.name)
                )
            })
            .collect();
        format!(" WHERE ({})", tests.join(" OR "))
    } else {
        String::new()
    };

    let order_sql = match &request.sort {
        Some(sort) => {
            // Only a real column of this table may be sorted on.
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
        None => String::new(),
    };

    let count_sql = format!("SELECT COUNT(*) FROM {table_sql}{where_sql}");
    let page_sql =
        format!("SELECT * FROM {table_sql}{where_sql}{order_sql} LIMIT {limit} OFFSET {offset}");

    let mut count_query = sqlx::query_scalar::<_, i64>(AssertSqlSafe(count_sql));
    let mut page_query = sqlx::query(AssertSqlSafe(page_sql.clone()));
    if let Some(pattern) = &pattern {
        count_query = count_query.bind(pattern);
        page_query = page_query.bind(pattern);
    }

    let total_rows = count_query.fetch_one(&mut *conn).await?;
    let rows = page_query.fetch_all(&mut *conn).await?;

    Ok(RowPage {
        columns: columns
            .into_iter()
            .map(|c| ResultColumn {
                name: c.name,
                data_type: c.data_type,
            })
            .collect(),
        rows: rows.iter().map(decode_row).collect(),
        total_rows,
        offset,
        limit,
        sql: page_sql,
    })
}

fn decode_row(row: &SqliteRow) -> Vec<Value> {
    (0..row.len())
        .map(|index| decode_value(row, index))
        .collect()
}

/// Reads one cell by its storage class. SQLite types values, not columns, so
/// the declared column type is only a hint (used here for booleans).
fn decode_value(row: &SqliteRow, index: usize) -> Value {
    let storage = match row.try_get_raw(index) {
        Ok(raw) if raw.is_null() => return Value::null(),
        Ok(raw) => raw.type_info().name().to_string(),
        Err(_) => return Value::null(),
    };
    let declared_bool = row.column(index).type_info().name() == "BOOLEAN";

    let decoded = match storage.as_str() {
        "INTEGER" | "BOOLEAN" => row.try_get_unchecked::<i64, _>(index).map(|n| {
            if declared_bool && (n == 0 || n == 1) {
                Value::bool(n == 1)
            } else {
                Value::int(n)
            }
        }),
        "REAL" | "NUMERIC" => row.try_get_unchecked::<f64, _>(index).map(Value::float),
        "BLOB" => row
            .try_get_unchecked::<&[u8], _>(index)
            .map(|bytes| Value::blob(bytes.len())),
        _ => row.try_get_unchecked::<String, _>(index).map(Value::text),
    };

    decoded.unwrap_or_else(|_| {
        // Text that is not valid UTF-8 lands here; show its size, not garbage.
        row.try_get_unchecked::<&[u8], _>(index)
            .map(|bytes| Value::blob(bytes.len()))
            .unwrap_or_else(|_| Value::null())
    })
}

fn result_columns(row: &SqliteRow) -> Vec<ResultColumn> {
    row.columns()
        .iter()
        .map(|column| ResultColumn {
            name: column.name().to_string(),
            data_type: display_type(column.type_info().name()),
        })
        .collect()
}

/// An expression has no declared type; sqlx reports that as `NULL`.
fn display_type(name: &str) -> String {
    if name == "NULL" {
        String::new()
    } else {
        name.to_string()
    }
}

pub(crate) async fn run_sql(
    pool: &SqlitePool,
    sql: &str,
    options: &RunOptions,
) -> Result<RawOutcome> {
    let mut conn = pool.acquire().await?;

    if let Some(timeout) = options.timeout {
        set_deadline(&mut conn, Instant::now() + timeout).await?;
    }
    let result = run_on(&mut conn, sql, options).await;
    if options.timeout.is_some() {
        clear_deadline(&mut conn).await;
    }

    // A script that fails inside BEGIN leaves the pooled connection in a
    // transaction. Roll it back; when none is open this fails harmlessly.
    let _ = sqlx::raw_sql("ROLLBACK").execute(&mut *conn).await;

    result
}

async fn run_on(
    conn: &mut SqliteConnection,
    sql: &str,
    options: &RunOptions,
) -> Result<RawOutcome> {
    let mut outcome = RawOutcome::default();
    let mut set_finished = false;

    {
        let mut stream = sqlx::raw_sql(AssertSqlSafe(sql.to_string())).fetch_many(&mut *conn);
        while let Some(item) = stream.try_next().await? {
            match item {
                Either::Left(done) => {
                    outcome.rows_affected += done.rows_affected();
                    outcome.statement_count += 1;
                    set_finished = true;
                }
                Either::Right(row) => {
                    // A later statement returned rows: show its result set.
                    if set_finished {
                        outcome.rows.clear();
                        outcome.columns.clear();
                        outcome.truncated = false;
                        set_finished = false;
                    }
                    if outcome.columns.is_empty() {
                        outcome.columns = result_columns(&row);
                    }
                    if outcome.rows.len() < options.max_rows {
                        outcome.rows.push(decode_row(&row));
                    } else {
                        outcome.truncated = true;
                        // With one statement there is nothing after it to
                        // run, so stop reading. A script must run to its end.
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

    // A query with no rows yields no row to read column names from.
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
    pool: &SqlitePool,
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
