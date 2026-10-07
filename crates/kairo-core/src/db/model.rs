//! Structured results returned by the database layer.
//!
//! These types are the contract with the front ends. Their serialised form is
//! mirrored in `desktop/src/api/types.ts`.

use super::value::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Sqlite,
    Postgres,
}

impl Engine {
    pub fn label(self) -> &'static str {
        match self {
            Engine::Sqlite => "SQLite",
            Engine::Postgres => "PostgreSQL",
        }
    }
}

/// What a front end may know about an open connection. Never holds a password.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub engine: Engine,
    /// Short name: the file name, or the database name.
    pub name: String,
    /// The file path, or the connection URL with the password masked.
    pub location: String,
    pub host: Option<String>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub server_version: String,
    pub read_only: bool,
    /// Stable identity of this database across sessions; keys history and recents.
    pub workspace_key: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TableKind {
    Table,
    View,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSummary {
    pub name: String,
    pub kind: TableKind,
    pub column_count: u32,
    /// `None` when the count was skipped or could not be read.
    pub row_count: Option<i64>,
    /// The count comes from planner statistics, not from counting rows.
    pub row_count_estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    /// The type as the database declares it.
    pub data_type: String,
    /// The closest `.kairo` type.
    pub kairo_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    /// 1-based position in the primary key, or 0.
    pub primary_key_position: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub unique: bool,
    pub primary: bool,
    pub columns: Vec<String>,
    /// Why the index exists: `primary key`, `unique constraint` or `index`.
    pub origin: String,
    pub definition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyInfo {
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub references_table: String,
    pub references_columns: Vec<String>,
    pub on_update: Option<String>,
    pub on_delete: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDetail {
    pub name: String,
    pub kind: TableKind,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    /// The statement that defines the table. Read from the catalog on SQLite,
    /// reconstructed from it on PostgreSQL.
    pub create_sql: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

/// One page of a table. The limit is clamped to [`MAX_PAGE_SIZE`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    pub limit: u32,
    #[serde(default)]
    pub offset: u64,
    /// Case-insensitive text to look for in any column.
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub sort: Option<SortSpec>,
}

pub const MAX_PAGE_SIZE: u32 = 1000;

impl PageRequest {
    pub fn first(limit: u32) -> Self {
        Self {
            limit,
            offset: 0,
            filter: None,
            sort: None,
        }
    }

    pub(crate) fn clamped_limit(&self) -> u32 {
        self.limit.clamp(1, MAX_PAGE_SIZE)
    }

    pub(crate) fn active_filter(&self) -> Option<&str> {
        self.filter
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultColumn {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowPage {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
    /// Rows matching the filter, across all pages.
    pub total_rows: i64,
    pub offset: u64,
    pub limit: u32,
    /// The statement that produced this page, as it was sent.
    pub sql: String,
}

/// What executing a script produced, before any service-level annotation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawOutcome {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: u64,
    /// More rows existed than `max_rows`; the rest were not kept.
    pub truncated: bool,
    pub statement_count: usize,
}
