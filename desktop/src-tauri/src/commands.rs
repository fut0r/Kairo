//! The Tauri command adapter.
//!
//! Each command is a thin wrapper: find the session, call a `kairo-core`
//! service, return its structured result. Errors are `KairoError`, which
//! serialises to `{ kind, message, detail?, hint? }`.

use crate::state::{AppState, ConnectionView};
use crate::store::{HistoryEntry, RecentItem, Settings, now};
use kairo_core::db::{
    Connection, ConnectionTarget, PageRequest, PostgresTarget, RowPage, SqliteTarget, TableDetail,
    TableSummary,
};
use kairo_core::schema::{self, Dialect, ValidationReport};
use kairo_core::services::export::ExportedSchema;
use kairo_core::services::project::{self, ProjectStatus};
use kairo_core::services::query::{self, PreparedQuery, QueryOptions, QueryOutcome};
use kairo_core::services::safety::{self, Risk};
use kairo_core::services::schema_apply::{self, ApplyPlan, ApplyReport, SchemaPreview};
use kairo_core::services::{self, ConnectionCheck, export};
use kairo_core::{ErrorKind, KairoError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::State;

/// Schema files larger than this are refused rather than loaded into the editor.
const MAX_SCHEMA_BYTES: u64 = 2 * 1024 * 1024;

/// How long catalog reads, row pages, plans and exports may take.
const CATALOG_LIMIT: Duration = Duration::from_secs(45);
/// Added to the user's query time limit before Kairo stops waiting itself.
const QUERY_GRACE: Duration = Duration::from_secs(15);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    core_version: &'static str,
    platform: &'static str,
    arch: &'static str,
    /// Where settings, recents and history are kept.
    store_path: Option<String>,
}

// No `Debug`: a request can carry a password.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ConnectRequest {
    Sqlite {
        path: String,
        /// Create the file if it does not exist.
        #[serde(default)]
        create: bool,
    },
    Postgres {
        url: String,
        /// Supplied separately when reconnecting to a saved connection.
        #[serde(default)]
        password: Option<String>,
    },
}

impl ConnectRequest {
    fn target(&self) -> Result<ConnectionTarget> {
        match self {
            Self::Sqlite { path, create } => {
                let path = path.trim();
                if path.is_empty() {
                    return Err(KairoError::invalid_input("Choose a database file."));
                }
                Ok(ConnectionTarget::Sqlite(if *create {
                    SqliteTarget::create(path)
                } else {
                    SqliteTarget::existing(path)
                }))
            }
            Self::Postgres { url, password } => Ok(ConnectionTarget::Postgres(
                PostgresTarget::parse(url, password.as_deref())?,
            )),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaCheck {
    report: ValidationReport,
    /// Present when the text parsed.
    preview: Option<SchemaPreview>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaFile {
    path: String,
    name: String,
    content: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        core_version: kairo_core::VERSION,
        platform: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        store_path: state
            .store()
            .path()
            .map(|p| p.to_string_lossy().into_owned()),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.store().settings()
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    state.store().set_settings(settings)
}

#[tauri::command]
pub fn list_recents(state: State<'_, AppState>) -> Vec<RecentItem> {
    state.store().recents()
}

#[tauri::command]
pub fn remove_recent(state: State<'_, AppState>, key: String) -> Result<Vec<RecentItem>> {
    let mut store = state.store();
    store.remove_recent(&key)?;
    Ok(store.recents())
}

#[tauri::command]
pub fn clear_recents(state: State<'_, AppState>) -> Result<()> {
    state.store().clear_recents()
}

/// Tries a connection and closes it again. Nothing is remembered.
#[tauri::command]
pub async fn test_connection(request: ConnectRequest) -> Result<ConnectionCheck> {
    services::test_connection(&request.target()?).await
}

async fn open_and_register(state: &AppState, target: &ConnectionTarget) -> Result<ConnectionView> {
    let conn = Connection::open(target).await?;
    let recent = RecentItem::for_connection(target, conn.info());

    let (session, replaced) = state.open(conn);
    if let Some(previous) = replaced {
        previous.conn.close().await;
    }
    state.store().touch_recent(recent);

    Ok(session.view())
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    request: ConnectRequest,
) -> Result<ConnectionView> {
    open_and_register(&state, &request.target()?).await
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, connection_id: String) -> Result<()> {
    if let Some(session) = state.close(&connection_id) {
        session.conn.close().await;
    }
    Ok(())
}

#[tauri::command]
pub fn list_connections(state: State<'_, AppState>) -> Vec<ConnectionView> {
    state.list()
}

/// Gives up on a database call that has stopped answering.
///
/// A connection can die without the driver noticing, for example when a
/// network path drops silently. Without a bound the caller would wait
/// forever and the screen would show a spinner that never ends.
async fn bounded<T>(
    what: &str,
    limit: Duration,
    work: impl Future<Output = Result<T>>,
) -> Result<T> {
    match tokio::time::timeout(limit, work).await {
        Ok(result) => result,
        Err(_) => Err(KairoError::new(
            ErrorKind::Timeout,
            format!(
                "{what} got no answer from the database after {} seconds.",
                limit.as_secs()
            ),
        )
        .with_hint("Check the connection, then try again or reconnect.")),
    }
}

#[tauri::command]
pub async fn list_tables(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<TableSummary>> {
    let session = state.session(&connection_id)?;
    bounded("Listing tables", CATALOG_LIMIT, session.conn.list_tables()).await
}

#[tauri::command]
pub async fn describe_table(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<TableDetail> {
    let session = state.session(&connection_id)?;
    bounded(
        "Reading the table",
        CATALOG_LIMIT,
        session.conn.describe_table(&table),
    )
    .await
}

#[tauri::command]
pub async fn fetch_rows(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    request: PageRequest,
) -> Result<RowPage> {
    let session = state.session(&connection_id)?;
    bounded(
        "Loading rows",
        CATALOG_LIMIT,
        session.conn.fetch_rows(&table, &request),
    )
    .await
}

/// Translates and classifies SQL without running it, so the UI can ask for
/// confirmation with the real reasons.
#[tauri::command]
pub fn analyze_query(sql: String) -> Result<PreparedQuery> {
    query::prepare(&sql)
}

/// Runs SQL. Anything above a read is refused unless `acknowledge` matches
/// its risk, so the confirmation cannot be skipped by a UI mistake.
#[tauri::command]
pub async fn run_query(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
    acknowledge: Option<Risk>,
) -> Result<QueryOutcome> {
    let session = state.session(&connection_id)?;
    let settings = state.store().settings();

    let prepared = query::prepare(&sql)?;
    safety::require_acknowledgement(&prepared.analysis, acknowledge, settings.confirm_writes)?;

    let limit = Duration::from_secs(settings.query_timeout_secs.into());
    let options = QueryOptions {
        max_rows: settings.max_rows as usize,
        timeout: Some(limit),
    };
    // The database enforces `limit` itself. The outer bound only matters when
    // the connection is so broken that even that never comes back.
    let result = bounded(
        "The query",
        limit + QUERY_GRACE,
        query::run_prepared(&session.conn, &prepared, &options),
    )
    .await;

    let entry = HistoryEntry {
        sql,
        at: now(),
        ok: result.is_ok(),
        risk: prepared.analysis.risk,
        elapsed_ms: result.as_ref().ok().map(|r| r.elapsed_ms),
        row_count: result.as_ref().ok().map(|r| r.row_count as u64),
        rows_affected: result.as_ref().ok().and_then(|r| r.rows_affected),
        error: result.as_ref().err().map(|e| e.message.clone()),
    };
    state
        .store()
        .push_history(&session.conn.info().workspace_key, entry);

    result
}

#[tauri::command]
pub fn list_history(state: State<'_, AppState>, workspace_key: String) -> Vec<HistoryEntry> {
    state.store().history(&workspace_key)
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>, workspace_key: String) -> Result<()> {
    state.store().clear_history(&workspace_key)
}

/// Validates `.kairo` text with the real parser and resolves it for a dialect.
/// With a connection the dialect is that database's; without one it is SQLite.
#[tauri::command]
pub fn validate_schema(
    state: State<'_, AppState>,
    text: String,
    connection_id: Option<String>,
) -> SchemaCheck {
    let dialect = connection_id
        .and_then(|id| state.session(&id).ok())
        .map(|session| session.conn.dialect())
        .unwrap_or(Dialect::Sqlite);

    let report = schema::validate(&text);
    let preview = report
        .schema
        .as_ref()
        .map(|parsed| schema_apply::preview(parsed, dialect));
    SchemaCheck { report, preview }
}

fn require_kairo_path(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path.trim());
    let is_kairo = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("kairo"));
    if !is_kairo {
        return Err(KairoError::invalid_input(
            "Schema files must have the .kairo extension.",
        ));
    }
    Ok(path)
}

/// Reads a `.kairo` file chosen in the open dialog. Only that extension is
/// accepted; this is not a general file reader.
#[tauri::command]
pub fn read_schema_file(path: String) -> Result<SchemaFile> {
    let path = require_kairo_path(&path)?;

    let size = std::fs::metadata(&path)
        .map_err(|err| KairoError::io("The schema file could not be opened.", &err))?
        .len();
    if size > MAX_SCHEMA_BYTES {
        return Err(KairoError::new(
            ErrorKind::Unsupported,
            "That file is too large to be a schema.",
        ));
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|err| KairoError::io("The schema file could not be read.", &err))?;

    Ok(SchemaFile {
        name: file_name(&path),
        path: path.to_string_lossy().into_owned(),
        // Editors on Windows often prepend a byte order mark.
        content: content
            .strip_prefix('\u{FEFF}')
            .unwrap_or(&content)
            .to_string(),
    })
}

/// Writes a `.kairo` file to a path chosen in the save dialog.
#[tauri::command]
pub fn write_schema_file(path: String, content: String) -> Result<SchemaFile> {
    let path = require_kairo_path(&path)?;
    std::fs::write(&path, &content)
        .map_err(|err| KairoError::io("The schema file could not be saved.", &err))?;

    Ok(SchemaFile {
        name: file_name(&path),
        path: path.to_string_lossy().into_owned(),
        content,
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn parse_for_apply(text: &str) -> Result<schema::Schema> {
    schema::parse_schema(text)
}

/// Compares a schema with the database. Changes nothing.
#[tauri::command]
pub async fn plan_schema(
    state: State<'_, AppState>,
    connection_id: String,
    text: String,
) -> Result<ApplyPlan> {
    let session = state.session(&connection_id)?;
    let parsed = parse_for_apply(&text)?;
    bounded(
        "Comparing the schema",
        CATALOG_LIMIT,
        schema_apply::plan(&session.conn, &parsed),
    )
    .await
}

/// Applies a schema. `confirmed` must be true: the UI sets it only after the
/// user has seen the plan and the target.
#[tauri::command]
pub async fn apply_schema(
    state: State<'_, AppState>,
    connection_id: String,
    text: String,
    confirmed: bool,
) -> Result<ApplyReport> {
    if !confirmed {
        return Err(KairoError::new(
            ErrorKind::ConfirmationRequired,
            "Applying a schema changes the database. Confirm to continue.",
        ));
    }
    let session = state.session(&connection_id)?;
    schema_apply::apply(&session.conn, &parse_for_apply(&text)?).await
}

#[tauri::command]
pub async fn export_schema(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<ExportedSchema> {
    let session = state.session(&connection_id)?;
    bounded(
        "Exporting",
        CATALOG_LIMIT,
        export::export_schema(&session.conn, None),
    )
    .await
}

/// Reports on a project folder and remembers it when it is one.
#[tauri::command]
pub fn project_status(state: State<'_, AppState>, dir: String) -> ProjectStatus {
    let dir = PathBuf::from(dir.trim());
    let status = project::status(&dir);
    if status.initialized {
        state.store().touch_recent(RecentItem::for_project(&dir));
    }
    status
}

/// Creates the project layout in a folder, as `kairo init` does.
#[tauri::command]
pub fn init_project(state: State<'_, AppState>, dir: String) -> Result<ProjectStatus> {
    let dir = PathBuf::from(dir.trim());
    project::init_project(&dir)?;
    state.store().touch_recent(RecentItem::for_project(&dir));
    Ok(project::status(&dir))
}

/// Opens the database a project's `kairo.config` points at.
#[tauri::command]
pub async fn connect_project(state: State<'_, AppState>, dir: String) -> Result<ConnectionView> {
    let dir = PathBuf::from(dir.trim());
    let config = project::load_config(&dir)?;
    let target = project::target_from_config(&config, &dir)?;
    open_and_register(&state, &target).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_requests_deserialise_from_the_wire_shape() {
        let sqlite: ConnectRequest =
            serde_json::from_str(r#"{ "kind": "sqlite", "path": "data/app.db" }"#).unwrap();
        assert!(matches!(
            sqlite,
            ConnectRequest::Sqlite { create: false, .. }
        ));

        let postgres: ConnectRequest = serde_json::from_str(
            r#"{ "kind": "postgres", "url": "postgres://u@h/db", "password": "pw" }"#,
        )
        .unwrap();
        let target = postgres.target().unwrap();
        assert_eq!(target.display(), "postgres://u:••••@h:5432/db");
    }

    #[test]
    fn empty_and_malformed_requests_are_rejected() {
        let empty = ConnectRequest::Sqlite {
            path: "  ".into(),
            create: false,
        };
        assert_eq!(empty.target().unwrap_err().kind, ErrorKind::InvalidInput);

        let bad = ConnectRequest::Postgres {
            url: "postgres://u:secret@host:port/db".into(),
            password: None,
        };
        let err = bad.target().unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidUrl);
        assert!(!serde_json::to_string(&err).unwrap().contains("secret"));
    }

    #[test]
    fn only_kairo_files_may_be_read_or_written() {
        assert!(require_kairo_path("schema/users.kairo").is_ok());
        assert!(require_kairo_path("C:\\x\\Users.KAIRO").is_ok());
        for refused in [
            "notes.txt",
            "data/kairo.db",
            "id_rsa",
            "kairo",
            ".kairo/config",
        ] {
            assert!(require_kairo_path(refused).is_err(), "{refused}");
        }
        assert_eq!(
            read_schema_file("C:\\Windows\\win.ini".into())
                .err()
                .unwrap()
                .kind,
            ErrorKind::InvalidInput
        );
        assert_eq!(
            write_schema_file("startup.bat".into(), "x".into())
                .err()
                .unwrap()
                .kind,
            ErrorKind::InvalidInput
        );
    }

    #[tokio::test]
    async fn a_call_that_never_answers_is_abandoned() {
        let stuck = bounded("Listing tables", Duration::from_millis(40), async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(1)
        })
        .await
        .unwrap_err();
        assert_eq!(stuck.kind, ErrorKind::Timeout);
        assert!(stuck.message.starts_with("Listing tables got no answer"));

        let quick = bounded("x", Duration::from_secs(5), async { Ok(7) }).await;
        assert_eq!(quick.unwrap(), 7);

        // An error from the database passes through unchanged.
        let failed: Result<i32> = bounded("x", Duration::from_secs(5), async {
            Err(KairoError::new(ErrorKind::Syntax, "bad"))
        })
        .await;
        assert_eq!(failed.unwrap_err().kind, ErrorKind::Syntax);
    }

    #[test]
    fn errors_serialise_to_the_documented_shape() {
        let err = KairoError::new(ErrorKind::AuthFailed, "nope").with_hint("try again");
        let json: serde_json::Value = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "auth_failed");
        assert_eq!(json["message"], "nope");
        assert_eq!(json["hint"], "try again");
        assert!(json.get("detail").is_none());
    }
}
