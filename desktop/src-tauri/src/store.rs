//! What the app remembers between runs: settings, recent connections and
//! query history. One JSON file in the OS config directory.
//!
//! Nothing here ever holds a password. Recent PostgreSQL connections keep the
//! URL with the password removed, and SQL is redacted before it is stored.

use kairo_core::db::{ConnectionInfo, ConnectionTarget, Engine};
use kairo_core::redact::{redact, redact_sql};
use kairo_core::services::safety::Risk;
use kairo_core::{KairoError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_RECENTS: usize = 12;
pub const MAX_HISTORY: usize = 200;

/// SQL longer than this is not kept in history.
const MAX_HISTORY_SQL: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: Theme,
    /// Rows per page in the Explorer.
    pub page_size: u32,
    /// Rows kept from a query result.
    pub max_rows: u32,
    pub query_timeout_secs: u32,
    /// Ask before statements that add data or objects. Destructive statements
    /// always ask, whatever this is set to.
    pub confirm_writes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            page_size: 50,
            max_rows: 1000,
            query_timeout_secs: 30,
            confirm_writes: true,
        }
    }
}

impl Settings {
    /// Brings every value into its supported range.
    pub fn sanitized(mut self) -> Self {
        self.page_size = self.page_size.clamp(10, 1000);
        self.max_rows = self.max_rows.clamp(10, 50_000);
        self.query_timeout_secs = self.query_timeout_secs.clamp(1, 3600);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecentKind {
    Sqlite,
    Postgres,
    Project,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentItem {
    /// Stable identity; also the key for query history.
    pub key: String,
    pub kind: RecentKind,
    pub name: String,
    /// What to show under the name. Never contains a password.
    pub location: String,
    /// What to reopen: a file path, a project folder, or a PostgreSQL URL
    /// without its password.
    pub target: String,
    /// Seconds since the Unix epoch.
    pub last_opened: u64,
}

impl RecentItem {
    /// Describes an opened connection in a form that is safe to store.
    pub fn for_connection(target: &ConnectionTarget, info: &ConnectionInfo) -> Self {
        let (kind, location, reopen) = match target {
            ConnectionTarget::Sqlite(_) => (
                RecentKind::Sqlite,
                info.location.clone(),
                info.location.clone(),
            ),
            ConnectionTarget::Postgres(pg) => (
                RecentKind::Postgres,
                format!("{}@{}:{}/{}", pg.username, pg.host, pg.port, pg.database),
                pg.safe_url.clone(),
            ),
        };
        debug_assert_eq!(
            target.engine() == Engine::Sqlite,
            kind == RecentKind::Sqlite
        );

        Self {
            key: info.workspace_key.clone(),
            kind,
            name: info.name.clone(),
            location,
            target: reopen,
            last_opened: now(),
        }
    }

    pub fn for_project(dir: &Path) -> Self {
        let shown = dir.to_string_lossy().into_owned();
        Self {
            key: format!("project:{shown}"),
            kind: RecentKind::Project,
            name: dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| shown.clone()),
            location: shown.clone(),
            target: shown,
            last_opened: now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub sql: String,
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub ok: bool,
    pub risk: Risk,
    pub elapsed_ms: Option<u64>,
    pub row_count: Option<u64>,
    pub rows_affected: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Data {
    settings: Settings,
    recents: Vec<RecentItem>,
    history: BTreeMap<String, Vec<HistoryEntry>>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct Store {
    /// `None` keeps everything in memory, for tests.
    path: Option<PathBuf>,
    data: Data,
}

impl Store {
    /// Reads the store. A missing file is a first run. An unreadable file is
    /// set aside as `.bad` and the app starts with defaults, so a damaged
    /// file can never stop Kairo from opening.
    pub fn load(path: PathBuf) -> Self {
        let data = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Data>(&text) {
                Ok(mut data) => {
                    data.settings = data.settings.sanitized();
                    data
                }
                Err(_) => {
                    let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                    Data::default()
                }
            },
            Err(_) => Data::default(),
        };
        Self {
            path: Some(path),
            data,
        }
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self {
            path: None,
            data: Data::default(),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Writes to a temporary file and renames it over the real one, so a
    /// crash mid-write cannot leave a half-written store.
    fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| KairoError::io("Could not create the settings folder.", &err))?;
        }
        let text = serde_json::to_string_pretty(&self.data)
            .map_err(|err| KairoError::internal(format!("Could not encode settings: {err}")))?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)
            .and_then(|()| std::fs::rename(&temporary, path))
            .map_err(|err| KairoError::io("Could not save settings.", &err))
    }

    /// Saves after a change the user did not ask for directly. Losing a
    /// history entry is not worth interrupting them over.
    fn save_quietly(&self) {
        if let Err(err) = self.save() {
            eprintln!("kairo: {err}");
        }
    }

    pub fn settings(&self) -> Settings {
        self.data.settings.clone()
    }

    pub fn set_settings(&mut self, settings: Settings) -> Result<Settings> {
        self.data.settings = settings.sanitized();
        self.save()?;
        Ok(self.settings())
    }

    /// Most recently opened first.
    pub fn recents(&self) -> Vec<RecentItem> {
        self.data.recents.clone()
    }

    pub fn touch_recent(&mut self, item: RecentItem) {
        self.data
            .recents
            .retain(|existing| existing.key != item.key);
        self.data.recents.insert(0, item);
        self.data.recents.truncate(MAX_RECENTS);
        self.save_quietly();
    }

    pub fn remove_recent(&mut self, key: &str) -> Result<()> {
        self.data.recents.retain(|existing| existing.key != key);
        self.data.history.remove(key);
        self.save()
    }

    pub fn clear_recents(&mut self) -> Result<()> {
        self.data.recents.clear();
        self.data.history.clear();
        self.save()
    }

    /// Newest first.
    pub fn history(&self, key: &str) -> Vec<HistoryEntry> {
        self.data.history.get(key).cloned().unwrap_or_default()
    }

    pub fn push_history(&mut self, key: &str, mut entry: HistoryEntry) {
        entry.sql = redact_sql(entry.sql.trim());
        entry.error = entry.error.map(|message| redact(&message));
        if entry.sql.is_empty() || entry.sql.len() > MAX_HISTORY_SQL {
            return;
        }

        let entries = self.data.history.entry(key.to_string()).or_default();
        // Running the same statement again updates its entry.
        entries.retain(|existing| existing.sql != entry.sql);
        entries.insert(0, entry);
        entries.truncate(MAX_HISTORY);
        self.save_quietly();
    }

    pub fn clear_history(&mut self, key: &str) -> Result<()> {
        self.data.history.remove(key);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairo_core::db::PostgresTarget;

    fn entry(sql: &str) -> HistoryEntry {
        HistoryEntry {
            sql: sql.to_string(),
            at: now(),
            ok: true,
            risk: Risk::Read,
            elapsed_ms: Some(3),
            row_count: Some(1),
            rows_affected: None,
            error: None,
        }
    }

    fn scratch_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kairo-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("kairo.json")
    }

    #[test]
    fn settings_are_clamped() {
        let wild = Settings {
            page_size: 5_000_000,
            max_rows: 1,
            query_timeout_secs: 0,
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(
            (wild.page_size, wild.max_rows, wild.query_timeout_secs),
            (1000, 10, 1)
        );
        assert!(Settings::default().confirm_writes);
    }

    #[test]
    fn the_store_round_trips_through_its_file() {
        let path = scratch_file("roundtrip");

        let mut store = Store::load(path.clone());
        assert_eq!(store.settings(), Settings::default());
        store
            .set_settings(Settings {
                theme: Theme::Light,
                page_size: 100,
                ..Settings::default()
            })
            .unwrap();
        store.push_history("sqlite:a", entry("SELECT 1"));

        let reopened = Store::load(path.clone());
        assert_eq!(reopened.settings().theme, Theme::Light);
        assert_eq!(reopened.settings().page_size, 100);
        assert_eq!(reopened.history("sqlite:a")[0].sql, "SELECT 1");
        assert!(!path.with_extension("json.tmp").exists());

        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn a_damaged_file_is_set_aside_not_fatal() {
        let path = scratch_file("damaged");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ this is not json").unwrap();

        let store = Store::load(path.clone());
        assert_eq!(store.settings(), Settings::default());
        assert!(path.with_extension("json.bad").exists());

        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn unknown_and_missing_fields_do_not_break_loading() {
        let data: Data = serde_json::from_str(
            r#"{ "settings": { "theme": "light", "fromTheFuture": 1 }, "extra": [] }"#,
        )
        .unwrap();
        assert_eq!(data.settings.theme, Theme::Light);
        assert_eq!(data.settings.page_size, 50);
    }

    #[test]
    fn recents_are_deduplicated_ordered_and_capped() {
        let mut store = Store::in_memory();
        for index in 0..(MAX_RECENTS + 5) {
            store.touch_recent(RecentItem::for_project(Path::new(&format!("/p/{index}"))));
        }
        assert_eq!(store.recents().len(), MAX_RECENTS);
        assert_eq!(store.recents()[0].name, format!("{}", MAX_RECENTS + 4));

        let oldest = store.recents().last().unwrap().clone();
        store.touch_recent(oldest.clone());
        assert_eq!(store.recents()[0].key, oldest.key);
        assert_eq!(store.recents().len(), MAX_RECENTS);

        store.remove_recent(&oldest.key).unwrap();
        assert!(store.recents().iter().all(|r| r.key != oldest.key));
    }

    #[test]
    fn history_is_newest_first_deduplicated_and_capped() {
        let mut store = Store::in_memory();
        store.push_history("k", entry("SELECT 1"));
        store.push_history("k", entry("SELECT 2"));
        store.push_history("k", entry("  SELECT 1  "));
        let sql: Vec<String> = store.history("k").into_iter().map(|e| e.sql).collect();
        assert_eq!(sql, ["SELECT 1", "SELECT 2"]);

        for index in 0..(MAX_HISTORY + 20) {
            store.push_history("k", entry(&format!("SELECT {index}")));
        }
        assert_eq!(store.history("k").len(), MAX_HISTORY);
        assert!(store.history("other").is_empty());

        store.push_history("k", entry("   "));
        assert_eq!(store.history("k").len(), MAX_HISTORY);

        store.clear_history("k").unwrap();
        assert!(store.history("k").is_empty());
    }

    #[test]
    fn nothing_stored_contains_a_password() {
        let target = ConnectionTarget::Postgres(
            PostgresTarget::parse("postgres://app:hunter2@db.example.com:5432/shop", None).unwrap(),
        );
        let info = ConnectionInfo {
            engine: Engine::Postgres,
            name: "shop".into(),
            location: target.display(),
            host: Some("db.example.com:5432".into()),
            database: Some("shop".into()),
            username: Some("app".into()),
            server_version: "PostgreSQL 16.2".into(),
            read_only: false,
            workspace_key: target.workspace_key(),
            size_bytes: None,
        };

        let mut store = Store::in_memory();
        let recent = RecentItem::for_connection(&target, &info);
        assert_eq!(recent.target, "postgres://app@db.example.com:5432/shop");
        assert_eq!(recent.location, "app@db.example.com:5432/shop");
        store.touch_recent(recent);

        store.push_history(
            &info.workspace_key,
            entry("ALTER ROLE app PASSWORD 'hunter2'"),
        );
        store.push_history(
            &info.workspace_key,
            HistoryEntry {
                ok: false,
                error: Some("could not reach postgres://app:hunter2@db.example.com/shop".into()),
                ..entry("SELECT dblink('host=x password=hunter2', 'select 1')")
            },
        );

        let saved = serde_json::to_string(&store.data).unwrap();
        assert!(!saved.contains("hunter2"), "{saved}");
    }
}
