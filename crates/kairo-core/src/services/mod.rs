//! Use cases shared by the CLI and the desktop app.

pub mod export;
pub mod project;
pub mod query;
pub mod report;
pub mod safety;
pub mod schema_apply;

use crate::db::{Connection, ConnectionTarget, Engine};
use crate::error::Result;
use serde::Serialize;
use std::time::Instant;

/// The result of trying a connection without keeping it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionCheck {
    pub engine: Engine,
    pub server_version: String,
    /// The location, with any password masked.
    pub location: String,
    pub table_count: usize,
    pub latency_ms: u64,
}

/// Opens the target, reads its catalog, and closes it again.
pub async fn test_connection(target: &ConnectionTarget) -> Result<ConnectionCheck> {
    let started = Instant::now();
    let conn = Connection::open(target).await?;
    let tables = conn.list_tables().await;
    conn.close().await;
    let tables = tables?;

    let info = conn.info();
    Ok(ConnectionCheck {
        engine: info.engine,
        server_version: info.server_version.clone(),
        location: info.location.clone(),
        table_count: tables.len(),
        latency_ms: started.elapsed().as_millis() as u64,
    })
}
