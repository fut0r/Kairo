//! One structured error type for every layer.
//!
//! Front ends decide how to present an error from its [`ErrorKind`]; they never
//! parse message strings. Every string stored here has been through
//! [`crate::redact::redact`], so a credential cannot escape via an error.

use crate::redact::redact;
use serde::Serialize;
use std::fmt;

pub type Result<T> = std::result::Result<T, KairoError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The caller passed something unusable (empty SQL, bad page size).
    InvalidInput,
    /// A file, table or database does not exist.
    NotFound,
    /// The file exists but is not a database, or is corrupted.
    InvalidDatabase,
    /// A connection URL could not be parsed.
    InvalidUrl,
    /// The server rejected the user name or password.
    AuthFailed,
    /// The server could not be reached.
    Network,
    /// TLS negotiation failed.
    Tls,
    /// The operation ran out of time or was interrupted.
    Timeout,
    /// The database is locked or the server is at capacity.
    Busy,
    /// The operating system or the server denied access.
    PermissionDenied,
    /// The SQL could not be parsed or referenced something unknown.
    Syntax,
    /// A constraint rejected the change.
    Constraint,
    /// A `.kairo` schema is malformed.
    Schema,
    /// The value or feature is not supported.
    Unsupported,
    /// The statement needs an explicit acknowledgement before it may run.
    ConfirmationRequired,
    /// No open connection matches the request.
    NotConnected,
    /// Reading or writing a file failed.
    Io,
    /// Any other database error.
    Database,
    /// A bug in Kairo.
    Internal,
}

#[derive(Debug, Clone, Serialize)]
pub struct KairoError {
    pub kind: ErrorKind,
    /// One sentence a person can act on.
    pub message: String,
    /// The underlying driver or OS message, when it adds information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What to try next.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl KairoError {
    pub fn new(kind: ErrorKind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            message: redact(message.as_ref()),
            detail: None,
            hint: None,
        }
    }

    pub fn with_detail(mut self, detail: impl AsRef<str>) -> Self {
        let detail = redact(detail.as_ref());
        if !detail.is_empty() && detail != self.message {
            self.detail = Some(detail);
        }
        self
    }

    pub fn with_hint(mut self, hint: impl AsRef<str>) -> Self {
        self.hint = Some(redact(hint.as_ref()));
        self
    }

    pub fn invalid_input(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    pub fn not_found(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    pub fn internal(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }

    pub fn io(context: impl AsRef<str>, source: &std::io::Error) -> Self {
        let kind = match source.kind() {
            std::io::ErrorKind::NotFound => ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Io,
        };
        Self::new(kind, context).with_detail(source.to_string())
    }
}

impl fmt::Display for KairoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        if let Some(detail) = &self.detail {
            write!(f, " ({detail})")?;
        }
        Ok(())
    }
}

impl std::error::Error for KairoError {}

impl From<sqlx::Error> for KairoError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::Database(db) => {
                let code = db.code().map(|c| c.to_string()).unwrap_or_default();
                from_database_error(&code, db.message())
            }
            sqlx::Error::Io(io) => from_io_error(io),
            sqlx::Error::Tls(e) => KairoError::new(
                ErrorKind::Tls,
                "A secure connection to the server could not be established.",
            )
            .with_detail(e.to_string())
            .with_hint("Check the sslmode parameter and the server certificate."),
            sqlx::Error::PoolTimedOut => KairoError::new(
                ErrorKind::Timeout,
                "Timed out waiting for the database to respond.",
            )
            .with_hint("Check that the host and port are reachable, then try again."),
            sqlx::Error::PoolClosed => {
                KairoError::new(ErrorKind::NotConnected, "The connection has been closed.")
            }
            sqlx::Error::Configuration(e) => KairoError::new(
                ErrorKind::InvalidUrl,
                "The connection settings are not valid.",
            )
            .with_detail(e.to_string()),
            sqlx::Error::RowNotFound => {
                KairoError::new(ErrorKind::NotFound, "The query returned no rows.")
            }
            sqlx::Error::ColumnDecode { index, source } => KairoError::new(
                ErrorKind::Unsupported,
                format!("Column {index} holds a value Kairo cannot display."),
            )
            .with_detail(source.to_string()),
            sqlx::Error::Decode(e) => KairoError::new(
                ErrorKind::Unsupported,
                "The result holds a value Kairo cannot display.",
            )
            .with_detail(e.to_string()),
            sqlx::Error::Protocol(e) => KairoError::new(
                ErrorKind::Database,
                "The server sent a response Kairo did not expect.",
            )
            .with_detail(e),
            other => KairoError::new(ErrorKind::Database, "The database reported an error.")
                .with_detail(other.to_string()),
        }
    }
}

fn from_io_error(io: &std::io::Error) -> KairoError {
    use std::io::ErrorKind as Io;
    let (kind, message, hint) = match io.kind() {
        Io::ConnectionRefused => (
            ErrorKind::Network,
            "The server refused the connection.",
            "Check the host and port, and that the server is running.",
        ),
        Io::TimedOut => (
            ErrorKind::Timeout,
            "The connection timed out.",
            "Check the host, your network, and any firewall in between.",
        ),
        Io::ConnectionReset | Io::ConnectionAborted | Io::BrokenPipe | Io::UnexpectedEof => (
            ErrorKind::Network,
            "The connection to the server was lost.",
            "Reconnect and try again.",
        ),
        Io::PermissionDenied => (
            ErrorKind::PermissionDenied,
            "Access was denied.",
            "Check the file or socket permissions.",
        ),
        _ => (
            ErrorKind::Network,
            "The server could not be reached.",
            "Check the host name and your network connection.",
        ),
    };
    KairoError::new(kind, message)
        .with_detail(io.to_string())
        .with_hint(hint)
}

/// Classifies a driver error by its code.
///
/// PostgreSQL reports a SQLSTATE, which is always five characters and is
/// often all digits (`42601`). SQLite reports a numeric result code of at
/// most four digits. Length is what tells them apart.
fn from_database_error(code: &str, message: &str) -> KairoError {
    if !code.is_empty() && code.len() < 5 && code.chars().all(|c| c.is_ascii_digit()) {
        let extended: u32 = code.parse().unwrap_or(1);
        return from_sqlite_code(extended, message);
    }
    from_sqlstate(code, message)
}

fn from_sqlite_code(extended: u32, message: &str) -> KairoError {
    let primary = extended & 0xff;
    match primary {
        5 | 6 => KairoError::new(
            ErrorKind::Busy,
            "The database is locked by another process.",
        )
        .with_detail(message)
        .with_hint("Close the other program using this file, or wait and try again."),
        8 => KairoError::new(ErrorKind::PermissionDenied, "The database is read-only.")
            .with_detail(message)
            .with_hint("Check the file permissions."),
        9 => KairoError::new(
            ErrorKind::Timeout,
            "The statement was stopped because it ran past the time limit.",
        )
        .with_hint("Narrow the query, or raise the time limit in Settings."),
        11 | 26 => KairoError::new(
            ErrorKind::InvalidDatabase,
            "The file is not a valid database or is corrupted.",
        )
        .with_detail(message),
        14 => KairoError::new(
            ErrorKind::NotFound,
            "The database file could not be opened.",
        )
        .with_detail(message)
        .with_hint("Check that the file exists and that you may read it."),
        19 => KairoError::new(ErrorKind::Constraint, "A constraint rejected the change.")
            .with_detail(message),
        23 => KairoError::new(
            ErrorKind::PermissionDenied,
            "The operation is not authorised.",
        )
        .with_detail(message),
        1 => KairoError::new(ErrorKind::Syntax, message),
        _ => KairoError::new(ErrorKind::Database, message),
    }
}

fn from_sqlstate(code: &str, message: &str) -> KairoError {
    let class = code.get(..2).unwrap_or("");
    match (code, class) {
        ("28P01", _) | ("28000", _) => KairoError::new(
            ErrorKind::AuthFailed,
            "The server rejected the user name or password.",
        )
        .with_detail(message)
        .with_hint("Check the credentials and that this user may connect from your address."),
        ("3D000", _) => KairoError::new(ErrorKind::NotFound, "That database does not exist.")
            .with_detail(message),
        ("42501", _) => KairoError::new(
            ErrorKind::PermissionDenied,
            "This user is not allowed to do that.",
        )
        .with_detail(message),
        ("57014", _) => KairoError::new(
            ErrorKind::Timeout,
            "The statement was cancelled because it ran past the time limit.",
        )
        .with_hint("Narrow the query, or raise the time limit in Settings."),
        ("55P03", _) | ("40P01", _) | ("40001", _) => KairoError::new(
            ErrorKind::Busy,
            "The statement could not get the lock it needed.",
        )
        .with_detail(message)
        .with_hint("Another session is holding the lock. Try again."),
        ("53300", _) => {
            KairoError::new(ErrorKind::Busy, "The server has no free connection slots.")
                .with_detail(message)
        }
        (_, "08") => KairoError::new(ErrorKind::Network, "The connection to the server failed.")
            .with_detail(message),
        (_, "23") => KairoError::new(ErrorKind::Constraint, "A constraint rejected the change.")
            .with_detail(message),
        (_, "42") => KairoError::new(ErrorKind::Syntax, message),
        (_, "0A") => KairoError::new(ErrorKind::Unsupported, message),
        _ => KairoError::new(ErrorKind::Database, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_codes_map_to_kinds() {
        assert_eq!(
            from_database_error("26", "x").kind,
            ErrorKind::InvalidDatabase
        );
        assert_eq!(from_database_error("5", "x").kind, ErrorKind::Busy);
        // 261 = SQLITE_BUSY_RECOVERY, an extended form of BUSY.
        assert_eq!(from_database_error("261", "x").kind, ErrorKind::Busy);
        // 2067 = SQLITE_CONSTRAINT_UNIQUE.
        assert_eq!(from_database_error("2067", "x").kind, ErrorKind::Constraint);
        assert_eq!(
            from_database_error("1", "near \"SELEC\"").kind,
            ErrorKind::Syntax
        );
        assert_eq!(
            from_database_error("9", "interrupted").kind,
            ErrorKind::Timeout
        );
    }

    #[test]
    fn sqlstates_map_to_kinds() {
        assert_eq!(
            from_database_error("28P01", "x").kind,
            ErrorKind::AuthFailed
        );
        assert_eq!(from_database_error("3D000", "x").kind, ErrorKind::NotFound);
        assert_eq!(from_database_error("42601", "x").kind, ErrorKind::Syntax);
        assert_eq!(from_database_error("42P01", "x").kind, ErrorKind::Syntax);
        assert_eq!(
            from_database_error("23505", "x").kind,
            ErrorKind::Constraint
        );
        assert_eq!(from_database_error("57014", "x").kind, ErrorKind::Timeout);
        assert_eq!(from_database_error("XX000", "x").kind, ErrorKind::Database);
    }

    #[test]
    fn messages_are_redacted_on_construction() {
        let err = KairoError::new(
            ErrorKind::Network,
            "could not reach postgres://admin:hunter2@db.example.com/app",
        )
        .with_detail("dsn was host=db password=hunter2 user=admin");
        let text = format!("{err} {:?}", err);
        assert!(!text.contains("hunter2"), "{text}");
        assert!(text.contains("db.example.com"));
    }
}
