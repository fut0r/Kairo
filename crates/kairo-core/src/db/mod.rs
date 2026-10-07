//! Database access: connection targets, the two adapters, typed results.

pub mod model;
pub mod sql;
pub mod target;
pub mod value;

mod postgres;
mod sqlite;

pub use model::{
    ColumnInfo, ConnectionInfo, Engine, ForeignKeyInfo, IndexInfo, MAX_PAGE_SIZE, PageRequest,
    RawOutcome, ResultColumn, RowPage, SortSpec, TableDetail, TableKind, TableSummary,
};
pub use target::{ConnectionTarget, PostgresTarget, SqliteTarget, is_postgres_url};
pub use value::{Value, ValueKind};

use crate::error::{KairoError, Result};
use crate::schema::Dialect;
use sqlx::postgres::PgPool;
use sqlx::sqlite::SqlitePool;
use std::time::Duration;

/// How a script is executed.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Rows kept from a result set. More than this sets `truncated`.
    pub max_rows: usize,
    /// The statement is stopped when it runs longer than this. `None` lets
    /// it run as long as it needs.
    pub timeout: Option<Duration>,
    /// The script is one statement, so reading may stop at `max_rows`.
    pub single_statement: bool,
    /// The statement is expected to produce a result set.
    pub returns_rows: bool,
}

/// A batch stopped at one statement. Nothing in the batch was applied.
#[derive(Debug)]
pub struct BatchFailure {
    /// Index of the failing statement, or `None` when the transaction itself
    /// could not be opened or committed.
    pub index: Option<usize>,
    pub error: KairoError,
}

impl BatchFailure {
    fn before_start(error: KairoError) -> Self {
        Self { index: None, error }
    }
}

enum Pool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
}

/// An open database. Cheap to share behind an `Arc`.
pub struct Connection {
    pool: Pool,
    info: ConnectionInfo,
}

impl Connection {
    /// Opens the target and verifies it is usable before returning.
    pub async fn open(target: &ConnectionTarget) -> Result<Self> {
        match target {
            ConnectionTarget::Sqlite(target) => {
                let (pool, info) = sqlite::open(target).await?;
                Ok(Self {
                    pool: Pool::Sqlite(pool),
                    info,
                })
            }
            ConnectionTarget::Postgres(target) => {
                let (pool, info) = postgres::open(target).await?;
                Ok(Self {
                    pool: Pool::Postgres(pool),
                    info,
                })
            }
        }
    }

    pub fn info(&self) -> &ConnectionInfo {
        &self.info
    }

    pub fn engine(&self) -> Engine {
        self.info.engine
    }

    pub fn dialect(&self) -> Dialect {
        match self.pool {
            Pool::Sqlite(_) => Dialect::Sqlite,
            Pool::Postgres(_) => Dialect::Postgres,
        }
    }

    /// Tables and views, ordered by name.
    pub async fn list_tables(&self) -> Result<Vec<TableSummary>> {
        match &self.pool {
            Pool::Sqlite(pool) => sqlite::list_tables(pool).await,
            Pool::Postgres(pool) => postgres::list_tables(pool).await,
        }
    }

    pub async fn describe_table(&self, table: &str) -> Result<TableDetail> {
        match &self.pool {
            Pool::Sqlite(pool) => sqlite::describe_table(pool, table).await,
            Pool::Postgres(pool) => postgres::describe_table(pool, table).await,
        }
    }

    /// Counts rows exactly. Can be slow on very large tables.
    pub async fn count_rows(&self, table: &str) -> Result<i64> {
        match &self.pool {
            Pool::Sqlite(pool) => sqlite::count_rows(pool, table).await,
            Pool::Postgres(pool) => postgres::count_rows(pool, table).await,
        }
    }

    /// One page of a table, optionally filtered and sorted. The table and
    /// sort column are checked against the catalog; the filter is bound.
    pub async fn fetch_rows(&self, table: &str, request: &PageRequest) -> Result<RowPage> {
        match &self.pool {
            Pool::Sqlite(pool) => sqlite::fetch_rows(pool, table, request).await,
            Pool::Postgres(pool) => postgres::fetch_rows(pool, table, request).await,
        }
    }

    /// Runs a script exactly as written. Callers decide whether it may run;
    /// see [`crate::services::safety`].
    pub async fn run_sql(&self, sql: &str, options: &RunOptions) -> Result<RawOutcome> {
        match &self.pool {
            Pool::Sqlite(pool) => sqlite::run_sql(pool, sql, options).await,
            Pool::Postgres(pool) => postgres::run_sql(pool, sql, options).await,
        }
    }

    /// Runs statements in one transaction: all of them, or none.
    pub async fn execute_batch(
        &self,
        statements: &[String],
    ) -> std::result::Result<(), BatchFailure> {
        match &self.pool {
            Pool::Sqlite(pool) => sqlite::execute_batch(pool, statements).await,
            Pool::Postgres(pool) => postgres::execute_batch(pool, statements).await,
        }
    }

    pub async fn close(&self) {
        match &self.pool {
            Pool::Sqlite(pool) => pool.close().await,
            Pool::Postgres(pool) => pool.close().await,
        }
    }
}
