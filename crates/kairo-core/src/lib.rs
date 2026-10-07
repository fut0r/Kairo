//! Shared core of KairoDB.
//!
//! - [`schema`]: the `.kairo` grammar, parser, SQL generation and rendering
//! - [`db`]: connection targets, SQLite and PostgreSQL adapters, typed values
//! - [`services`]: the use cases both the CLI and the desktop app call
//!
//! Nothing here prints or formats for a terminal, except [`services::report`].

pub mod db;
pub mod error;
pub mod redact;
pub mod schema;
pub mod services;

pub use error::{ErrorKind, KairoError, Result};

/// Version of the core crate, shared by every front end.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
