//! Where a connection points: an SQLite file or a PostgreSQL server.
//!
//! A target is parsed once. From then on the password exists only inside the
//! driver options; every other field is safe to show, store and log.

use super::model::Engine;
use crate::error::{ErrorKind, KairoError, Result};
use crate::redact::MASK;
use sqlx::postgres::PgConnectOptions;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// True when `input` is a PostgreSQL URL rather than a file path.
pub fn is_postgres_url(input: &str) -> bool {
    let lower = input.trim_start().to_ascii_lowercase();
    lower.starts_with("postgres://") || lower.starts_with("postgresql://")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteTarget {
    pub path: PathBuf,
    /// Create the file when it does not exist.
    pub create: bool,
    pub read_only: bool,
}

#[derive(Clone)]
pub struct PostgresTarget {
    /// Driver options. This is the only place the password is kept.
    pub(crate) options: PgConnectOptions,
    /// The URL with the password removed. Safe to persist.
    pub safe_url: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub has_password: bool,
}

// Written by hand so a stray `{:?}` can never print the driver options.
impl fmt::Debug for PostgresTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresTarget")
            .field("url", &self.display_url())
            .finish()
    }
}

#[derive(Debug, Clone)]
// One target exists per connection attempt, so the size difference between
// the variants costs nothing worth an extra allocation.
#[allow(clippy::large_enum_variant)]
pub enum ConnectionTarget {
    Sqlite(SqliteTarget),
    Postgres(PostgresTarget),
}

impl ConnectionTarget {
    /// Interprets what a user typed: a PostgreSQL URL or a path to a file
    /// that must already exist. This is what `kairo read` and `kairo export`
    /// accept.
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            return Err(KairoError::invalid_input(
                "Enter a database file path or a PostgreSQL URL.",
            ));
        }
        if is_postgres_url(input) {
            Ok(Self::Postgres(PostgresTarget::parse(input, None)?))
        } else {
            Ok(Self::Sqlite(SqliteTarget::existing(input)))
        }
    }

    pub fn engine(&self) -> Engine {
        match self {
            Self::Sqlite(_) => Engine::Sqlite,
            Self::Postgres(_) => Engine::Postgres,
        }
    }

    /// The path, or the URL with its password masked.
    pub fn display(&self) -> String {
        match self {
            Self::Sqlite(target) => target.display_path(),
            Self::Postgres(target) => target.display_url(),
        }
    }

    /// Identity of the database, independent of credentials.
    pub fn workspace_key(&self) -> String {
        match self {
            Self::Sqlite(target) => format!("sqlite:{}", target.display_path()),
            Self::Postgres(target) => format!(
                "postgres:{}@{}:{}/{}",
                target.username, target.host, target.port, target.database
            ),
        }
    }
}

impl SqliteTarget {
    /// A file that must already exist, opened read-write.
    pub fn existing(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            create: false,
            read_only: false,
        }
    }

    /// A file that is created when missing.
    pub fn create(path: impl AsRef<Path>) -> Self {
        Self {
            create: true,
            ..Self::existing(path)
        }
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// The absolute path when the file exists, without the `\\?\` prefix
    /// Windows adds to canonical paths.
    pub fn display_path(&self) -> String {
        let resolved = std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        let text = resolved.to_string_lossy();
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
    }

    /// Checks the path before the driver sees it, so the common mistakes get
    /// a clear message instead of a driver error.
    pub(crate) fn check(&self) -> Result<()> {
        let shown = self.path.display();

        if self
            .path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("kairo"))
        {
            return Err(KairoError::invalid_input(format!(
                "'{shown}' is a .kairo schema file, not a database."
            ))
            .with_hint("Open a database file such as data/kairo.db. Schemas are opened in the Schema workspace."));
        }

        if self.path.is_dir() {
            return Err(KairoError::invalid_input(format!(
                "'{shown}' is not a file."
            )));
        }

        if !self.path.exists() && !self.create {
            return Err(KairoError::not_found(format!(
                "database file '{shown}' not found."
            )));
        }

        Ok(())
    }
}

impl PostgresTarget {
    /// Parses a `postgres://` URL. `password` overrides one in the URL and is
    /// how a password is supplied for a saved connection.
    pub fn parse(url: &str, password: Option<&str>) -> Result<Self> {
        let url = url.trim();
        if !is_postgres_url(url) {
            return Err(KairoError::new(
                ErrorKind::InvalidUrl,
                "A PostgreSQL URL starts with postgres:// or postgresql://.",
            )
            .with_hint("Example: postgres://user@localhost:5432/database"));
        }

        let parts = split_userinfo(url);
        if parts.host_and_rest.is_empty() || parts.host_and_rest.starts_with('/') {
            return Err(
                KairoError::new(ErrorKind::InvalidUrl, "The URL has no host.")
                    .with_hint("Example: postgres://user@localhost:5432/database"),
            );
        }

        let mut options = PgConnectOptions::from_str(&parts.safe_url).map_err(|err| {
            KairoError::new(ErrorKind::InvalidUrl, "The PostgreSQL URL is not valid.")
                .with_detail(err.to_string())
                .with_hint("Example: postgres://user@localhost:5432/database")
        })?;

        let password = password
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .or(parts.password);
        if let Some(password) = &password {
            options = options.password(password);
        }
        // Shows up in pg_stat_activity, so an operator can tell who is connected.
        options = options.application_name("kairo");

        let username = options.get_username().to_string();
        let database = options
            .get_database()
            .map(str::to_string)
            .unwrap_or_else(|| username.clone());

        Ok(Self {
            host: options.get_host().to_string(),
            port: options.get_port(),
            database,
            username,
            has_password: password.is_some(),
            safe_url: parts.safe_url,
            options,
        })
    }

    /// `postgres://user:••••@host:port/database`, for headers and messages.
    pub fn display_url(&self) -> String {
        let secret = if self.has_password {
            format!(":{MASK}")
        } else {
            String::new()
        };
        format!(
            "postgres://{}{}@{}:{}/{}",
            self.username, secret, self.host, self.port, self.database
        )
    }
}

struct UrlParts {
    safe_url: String,
    password: Option<String>,
    host_and_rest: String,
}

/// Removes the password from a URL and returns it percent-decoded.
fn split_userinfo(url: &str) -> UrlParts {
    let Some(scheme_end) = url.find("://") else {
        return UrlParts {
            safe_url: url.to_string(),
            password: None,
            host_and_rest: String::new(),
        };
    };
    let (scheme, tail) = url.split_at(scheme_end + 3);

    let authority_end = tail.find(['/', '?', '#']).unwrap_or(tail.len());
    let Some(at) = tail[..authority_end].rfind('@') else {
        return UrlParts {
            safe_url: url.to_string(),
            password: None,
            host_and_rest: tail.to_string(),
        };
    };

    let userinfo = &tail[..at];
    let host_and_rest = &tail[at + 1..];
    let (user, password) = match userinfo.split_once(':') {
        Some((user, password)) => (user, Some(percent_decode(password))),
        None => (userinfo, None),
    };

    UrlParts {
        safe_url: format!("{scheme}{user}@{host_and_rest}"),
        password: password.filter(|p| !p.is_empty()),
        host_and_rest: host_and_rest.to_string(),
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let decoded = (bytes[i] == b'%')
            .then(|| input.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match decoded {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_postgres_urls() {
        assert!(is_postgres_url("postgres://localhost/db"));
        assert!(is_postgres_url("  PostgreSQL://localhost/db"));
        assert!(!is_postgres_url("data/kairo.db"));
        assert!(!is_postgres_url("C:\\data\\postgres.db"));
    }

    #[test]
    fn parses_a_url_and_keeps_the_password_out_of_every_public_field() {
        let target =
            PostgresTarget::parse("postgres://alice:s3cr%40t@db.example.com:6543/shop", None)
                .unwrap();
        assert_eq!(target.host, "db.example.com");
        assert_eq!(target.port, 6543);
        assert_eq!(target.database, "shop");
        assert_eq!(target.username, "alice");
        assert!(target.has_password);
        assert_eq!(target.safe_url, "postgres://alice@db.example.com:6543/shop");
        assert_eq!(
            target.display_url(),
            format!("postgres://alice:{MASK}@db.example.com:6543/shop")
        );

        let everything = format!(
            "{target:?} {} {} {}",
            target.safe_url,
            target.display_url(),
            ConnectionTarget::Postgres(target.clone()).workspace_key()
        );
        assert!(!everything.contains("s3cr"), "{everything}");
    }

    #[test]
    fn a_separate_password_overrides_the_url() {
        let target =
            PostgresTarget::parse("postgres://alice:old@localhost/shop", Some("new")).unwrap();
        assert!(target.has_password);
        assert_eq!(target.safe_url, "postgres://alice@localhost/shop");

        let none = PostgresTarget::parse("postgres://alice@localhost/shop", None).unwrap();
        assert!(!none.has_password);
        assert_eq!(none.display_url(), "postgres://alice@localhost:5432/shop");
    }

    #[test]
    fn query_parameters_survive_in_the_safe_url() {
        let target = PostgresTarget::parse("postgres://u:p@h/db?sslmode=require", None).unwrap();
        assert_eq!(target.safe_url, "postgres://u@h/db?sslmode=require");
    }

    #[test]
    fn malformed_urls_are_rejected_without_echoing_the_password() {
        for bad in [
            "postgres://",
            "postgres:///nohost",
            "postgres://user:hunter2@host:notaport/db",
            "mysql://user:hunter2@host/db",
        ] {
            let err = PostgresTarget::parse(bad, None).unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidUrl, "{bad}");
            assert!(!format!("{err:?}").contains("hunter2"), "{err:?}");
        }
    }

    #[test]
    fn target_parse_routes_by_shape() {
        assert_eq!(
            ConnectionTarget::parse("postgres://u@h/db")
                .unwrap()
                .engine(),
            Engine::Postgres
        );
        assert_eq!(
            ConnectionTarget::parse("data/kairo.db").unwrap().engine(),
            Engine::Sqlite
        );
        assert!(ConnectionTarget::parse("   ").is_err());
    }

    #[test]
    fn sqlite_checks_explain_common_mistakes() {
        let schema = SqliteTarget::existing("schema/users.kairo")
            .check()
            .unwrap_err();
        assert_eq!(schema.kind, ErrorKind::InvalidInput);
        assert!(schema.message.contains(".kairo schema file"));

        let missing = SqliteTarget::existing("definitely/not/here.db")
            .check()
            .unwrap_err();
        assert_eq!(missing.kind, ErrorKind::NotFound);

        assert!(
            SqliteTarget::create("definitely/not/here.db")
                .check()
                .is_ok()
        );

        let dir = SqliteTarget::existing(std::env::temp_dir())
            .check()
            .unwrap_err();
        assert!(dir.message.contains("is not a file"));
    }
}
