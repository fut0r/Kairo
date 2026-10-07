//! A Kairo project: a folder with a `kairo.config` and a `schema/` directory.

use crate::db::{ConnectionTarget, PostgresTarget, SqliteTarget, is_postgres_url};
use crate::error::{ErrorKind, KairoError, Result};
use crate::redact::redact;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "kairo.config";
pub const SCHEMA_DIR: &str = "schema";

const PROJECT_DIRS: [&str; 5] = ["schema", "data", "migrations", "queries", "plugins"];
const DEFAULT_CONFIG: &str =
    "# KairoDB Configuration\nadapter = \"sqlite\"\ndatabase = \"data/kairo.db\"\n";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Config {
    pub adapter: String,
    pub database: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaFile {
    /// The file name without `.kairo`; what `kairo create <name>` takes.
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub root: String,
    /// A readable `kairo.config` exists.
    pub initialized: bool,
    pub adapter: Option<String>,
    /// The configured database, with any password masked.
    pub database: Option<String>,
    pub schema_files: Vec<SchemaFile>,
    /// For SQLite projects, whether the database file exists yet.
    pub database_exists: Option<bool>,
    /// Why the config could not be used, when it exists but is broken.
    pub problem: Option<String>,
}

pub fn load_config(dir: &Path) -> Result<Config> {
    let path = dir.join(CONFIG_FILE);
    let content = std::fs::read_to_string(&path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            KairoError::not_found("no kairo.config found. run 'kairo init' first.")
        } else {
            KairoError::io("kairo.config could not be read.", &err)
        }
    })?;

    toml::from_str(&content).map_err(|err| {
        KairoError::new(ErrorKind::InvalidInput, "kairo.config is not valid.")
            .with_detail(err.message())
            .with_hint("It needs two lines: adapter = \"sqlite\" and database = \"data/kairo.db\".")
    })
}

/// The path part of an SQLite `database` setting. Accepts a plain path or the
/// `sqlite:` URL form earlier releases allowed.
fn sqlite_path(database: &str) -> &str {
    let path = database
        .strip_prefix("sqlite://")
        .or_else(|| database.strip_prefix("sqlite:"))
        .unwrap_or(database);
    path.split('?').next().unwrap_or(path)
}

/// Resolves the configured database, relative to the project folder.
pub fn target_from_config(config: &Config, dir: &Path) -> Result<ConnectionTarget> {
    match config.adapter.as_str() {
        "sqlite" => {
            // A project database is created on first use, as it always was.
            let path = dir.join(sqlite_path(&config.database));
            Ok(ConnectionTarget::Sqlite(SqliteTarget::create(path)))
        }
        "postgres" | "postgresql" => Ok(ConnectionTarget::Postgres(PostgresTarget::parse(
            &config.database,
            None,
        )?)),
        other => Err(KairoError::new(
            ErrorKind::Unsupported,
            format!("adapter '{other}' not supported"),
        )
        .with_hint("Use \"sqlite\" or \"postgres\".")),
    }
}

/// Where `kairo create <name>` looks for a schema.
pub fn schema_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(SCHEMA_DIR).join(format!("{name}.kairo"))
}

/// Creates the project folders and a default config. Existing files are kept.
/// Returns what was created, relative to `dir`.
pub fn init_project(dir: &Path) -> Result<Vec<String>> {
    let mut created = Vec::new();

    for name in PROJECT_DIRS {
        let path = dir.join(name);
        if !path.exists() {
            std::fs::create_dir_all(&path)
                .map_err(|err| KairoError::io(format!("Could not create {name}/."), &err))?;
            created.push(format!("{name}/"));
        }
    }

    let config = dir.join(CONFIG_FILE);
    if !config.exists() {
        std::fs::write(&config, DEFAULT_CONFIG)
            .map_err(|err| KairoError::io("Could not write kairo.config.", &err))?;
        created.push(CONFIG_FILE.to_string());
    }

    Ok(created)
}

fn schema_files(dir: &Path) -> Vec<SchemaFile> {
    let Ok(entries) = std::fs::read_dir(dir.join(SCHEMA_DIR)) else {
        return Vec::new();
    };

    let mut files: Vec<SchemaFile> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "kairo"))
        .filter_map(|path| {
            Some(SchemaFile {
                name: path.file_stem()?.to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
            })
        })
        .collect();
    files.sort_by_key(|file| file.name.to_lowercase());
    files
}

/// Reports on a folder. Never fails: a missing or broken config is part of
/// the status.
pub fn status(dir: &Path) -> ProjectStatus {
    let mut status = ProjectStatus {
        root: dir.to_string_lossy().into_owned(),
        initialized: false,
        adapter: None,
        database: None,
        schema_files: schema_files(dir),
        database_exists: None,
        problem: None,
    };

    match load_config(dir) {
        Ok(config) => {
            status.initialized = true;
            if config.adapter == "sqlite" && !is_postgres_url(&config.database) {
                status.database_exists = Some(dir.join(sqlite_path(&config.database)).is_file());
            }
            status.database = Some(redact(&config.database));
            status.adapter = Some(config.adapter);
        }
        Err(err) if err.kind == ErrorKind::NotFound => {}
        Err(err) => status.problem = Some(err.to_string()),
    }

    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Engine;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kairo-project-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn init_creates_the_layout_once() {
        let dir = scratch("init");

        let created = init_project(&dir).unwrap();
        assert_eq!(
            created,
            [
                "schema/",
                "data/",
                "migrations/",
                "queries/",
                "plugins/",
                "kairo.config"
            ]
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(CONFIG_FILE)).unwrap(),
            DEFAULT_CONFIG
        );

        // Running it again changes nothing and keeps an edited config.
        std::fs::write(
            dir.join(CONFIG_FILE),
            "adapter = \"sqlite\"\ndatabase = \"x.db\"\n",
        )
        .unwrap();
        assert!(init_project(&dir).unwrap().is_empty());
        assert_eq!(load_config(&dir).unwrap().database, "x.db");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn status_describes_a_project() {
        let dir = scratch("status");
        assert!(!status(&dir).initialized);

        init_project(&dir).unwrap();
        std::fs::write(dir.join("schema/users.kairo"), "table users { a: int }").unwrap();
        std::fs::write(dir.join("schema/Posts.kairo"), "table posts { a: int }").unwrap();
        std::fs::write(dir.join("schema/notes.txt"), "not a schema").unwrap();

        let found = status(&dir);
        assert!(found.initialized);
        assert_eq!(found.adapter.as_deref(), Some("sqlite"));
        assert_eq!(found.database.as_deref(), Some("data/kairo.db"));
        assert_eq!(found.database_exists, Some(false));
        let names: Vec<&str> = found.schema_files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Posts", "users"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn status_masks_a_configured_password_and_reports_a_broken_config() {
        let dir = scratch("secrets");
        std::fs::write(
            dir.join(CONFIG_FILE),
            "adapter = \"postgres\"\ndatabase = \"postgres://app:hunter2@db/prod\"\n",
        )
        .unwrap();
        let found = status(&dir);
        assert!(!format!("{found:?}").contains("hunter2"));
        assert_eq!(found.database_exists, None);

        std::fs::write(dir.join(CONFIG_FILE), "adapter = ").unwrap();
        let broken = status(&dir);
        assert!(!broken.initialized);
        assert!(
            broken
                .problem
                .unwrap()
                .contains("kairo.config is not valid")
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn config_resolves_to_a_target() {
        let dir = Path::new("project");
        let sqlite = |database: &str| Config {
            adapter: "sqlite".into(),
            database: database.into(),
        };

        for database in [
            "data/kairo.db",
            "sqlite:data/kairo.db",
            "sqlite:data/kairo.db?mode=rwc",
        ] {
            match target_from_config(&sqlite(database), dir).unwrap() {
                ConnectionTarget::Sqlite(target) => {
                    assert_eq!(target.path, dir.join("data/kairo.db"), "{database}");
                    assert!(target.create);
                }
                other => panic!("expected sqlite, got {other:?}"),
            }
        }

        let postgres = Config {
            adapter: "postgres".into(),
            database: "postgres://u@h/db".into(),
        };
        assert_eq!(
            target_from_config(&postgres, dir).unwrap().engine(),
            Engine::Postgres
        );

        let err = target_from_config(
            &Config {
                adapter: "mysql".into(),
                database: "x".into(),
            },
            dir,
        )
        .unwrap_err();
        assert_eq!(err.message, "adapter 'mysql' not supported");
    }
}
