//! Live application state: open connections and the persistent store.

use crate::store::Store;
use kairo_core::db::{Connection, ConnectionInfo, Engine};
use kairo_core::{ErrorKind, KairoError, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// An open connection as the front end sees it: an id plus its description.
/// The password stays inside the driver; it is not part of this.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    pub id: String,
    #[serde(flatten)]
    pub info: ConnectionInfo,
}

pub struct Session {
    pub id: String,
    pub conn: Connection,
}

impl Session {
    pub fn view(&self) -> ConnectionView {
        let mut info = self.conn.info().clone();
        // The size at open time goes stale as soon as anything is written.
        if info.engine == Engine::Sqlite {
            info.size_bytes = std::fs::metadata(&info.location).ok().map(|m| m.len());
        }
        ConnectionView {
            id: self.id.clone(),
            info,
        }
    }
}

pub struct AppState {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    store: Mutex<Store>,
    next_id: AtomicU64,
}

impl AppState {
    pub fn new(store: Store) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            store: Mutex::new(store),
            next_id: AtomicU64::new(1),
        }
    }

    // A panic while holding either lock leaves plain data behind, not a
    // broken invariant, so a poisoned lock is safe to keep using.
    fn sessions(&self) -> MutexGuard<'_, HashMap<String, Arc<Session>>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The store. Never hold this guard across an `.await`.
    pub fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn session(&self, id: &str) -> Result<Arc<Session>> {
        self.sessions().get(id).cloned().ok_or_else(|| {
            KairoError::new(
                ErrorKind::NotConnected,
                "That connection is no longer open.",
            )
            .with_hint("Reconnect from the sidebar.")
        })
    }

    /// Registers a connection. Returns it along with any earlier session for
    /// the same database, which the caller should close.
    pub fn open(&self, conn: Connection) -> (Arc<Session>, Option<Arc<Session>>) {
        let key = conn.info().workspace_key.clone();
        let session = Arc::new(Session {
            id: format!("c{}", self.next_id.fetch_add(1, Ordering::Relaxed)),
            conn,
        });

        let mut sessions = self.sessions();
        let replaced_id = sessions
            .values()
            .find(|existing| existing.conn.info().workspace_key == key)
            .map(|existing| existing.id.clone());
        let replaced = replaced_id.and_then(|id| sessions.remove(&id));
        sessions.insert(session.id.clone(), session.clone());

        (session, replaced)
    }

    pub fn close(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions().remove(id)
    }

    /// Open connections, oldest first.
    pub fn list(&self) -> Vec<ConnectionView> {
        let mut views: Vec<ConnectionView> = self.sessions().values().map(|s| s.view()).collect();
        views.sort_by_key(|view| view.id[1..].parse::<u64>().unwrap_or(0));
        views
    }
}
