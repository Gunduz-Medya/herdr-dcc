//! The SQLite file. One connection per process, WAL mode, a busy timeout (several dcc processes
//! can write at once, for example a terminal and a detached job) and numbered migrations kept in
//! `PRAGMA user_version`. The file never holds conversation content or secrets.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::util::{create_private_dir, now_ms};

const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// Migration N (index N-1) takes the schema from version N-1 to N. Never edit a shipped entry,
/// add a new one.
const MIGRATIONS: &[&str] = &[
    // 1, workspaces, contract test results, small key value state
    "CREATE TABLE workspaces (
        name       TEXT PRIMARY KEY,
        folder     TEXT NOT NULL,
        kind       TEXT NOT NULL,
        definition TEXT NOT NULL,
        saved_ms   INTEGER NOT NULL
     );
     CREATE TABLE contract_runs (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        ts_ms      INTEGER NOT NULL,
        herdr_app  TEXT NOT NULL,
        herdr_core TEXT NOT NULL,
        passed     INTEGER NOT NULL,
        detail     TEXT
     );
     CREATE TABLE herdr_state (
        key        TEXT PRIMARY KEY,
        value      TEXT NOT NULL,
        updated_ms INTEGER NOT NULL
     );",
    // 2, agent runs (a process in a terminal), their sessions and their events
    "CREATE TABLE agent_runs (
        id                  TEXT PRIMARY KEY,
        agent_kind          TEXT NOT NULL,
        launch_mode         TEXT NOT NULL,
        launch_session_id   TEXT,
        started_ms          INTEGER NOT NULL,
        ended_ms            INTEGER,
        last_state          TEXT NOT NULL,
        last_state_ms       INTEGER NOT NULL,
        herdr_status        TEXT,
        end_reason          TEXT,
        exit_code           INTEGER,
        cwd                 TEXT,
        repo_root           TEXT,
        branch              TEXT,
        worktree_kind       TEXT,
        worktree_path       TEXT,
        changed_files       INTEGER,
        staged              INTEGER,
        unstaged            INTEGER,
        untracked           INTEGER,
        herdr_session       TEXT,
        terminal_addr       TEXT,
        terminal_created_ms INTEGER,
        pane_id             TEXT,
        space_name          TEXT,
        tab_name            TEXT
     );
     CREATE INDEX agent_runs_open ON agent_runs (ended_ms, last_state_ms);
     CREATE TABLE agent_sessions (
        id          TEXT PRIMARY KEY,
        run_id      TEXT NOT NULL REFERENCES agent_runs(id),
        id_source   TEXT NOT NULL,
        started_ms  INTEGER NOT NULL,
        ended_ms    INTEGER,
        end_source  TEXT
     );
     CREATE INDEX agent_sessions_run ON agent_sessions (run_id, started_ms);
     CREATE TABLE agent_events (
        id                INTEGER PRIMARY KEY AUTOINCREMENT,
        run_id            TEXT NOT NULL REFERENCES agent_runs(id),
        session_id        TEXT,
        ts_ms             INTEGER NOT NULL,
        kind              TEXT NOT NULL,
        state             TEXT,
        tool_name         TEXT,
        notification_type TEXT
     );
     CREATE INDEX agent_events_run ON agent_events (run_id, ts_ms);",
];

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub name: String,
    pub folder: String,
    pub kind: String,
    pub definition: String,
    pub saved_ms: i64,
}

impl Store {
    /// Opens (and creates) `<data_dir>/dcc.sqlite` and brings it to the latest schema.
    pub fn open(data_dir: &Path) -> Result<Self> {
        Self::open_with_timeout(data_dir, BUSY_TIMEOUT)
    }

    /// Same, with a shorter wait for a busy file. A hook uses this so it can never hold an agent up.
    pub fn open_with_timeout(data_dir: &Path, busy: Duration) -> Result<Self> {
        create_private_dir(data_dir)?;
        let conn = Connection::open(Self::file(data_dir))?;
        Self::init(conn, busy)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, BUSY_TIMEOUT)
    }

    pub fn file(data_dir: &Path) -> PathBuf {
        data_dir.join("dcc.sqlite")
    }

    fn init(conn: Connection, busy: Duration) -> Result<Self> {
        conn.busy_timeout(busy)?;
        conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    pub fn latest_version() -> i64 {
        MIGRATIONS.len() as i64
    }

    fn migrate(&self) -> Result<()> {
        let current = self.schema_version()?;
        if current > Self::latest_version() {
            return Err(Error::conflict(format!(
                "the database is at schema {current}, this dcc knows up to {}",
                Self::latest_version()
            )));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // workspaces

    pub fn save_workspace(
        &self,
        name: &str,
        folder: &str,
        kind: &str,
        definition: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO workspaces (name, folder, kind, definition, saved_ms) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET folder = ?2, kind = ?3, definition = ?4, saved_ms = ?5",
            params![name, folder, kind, definition, now_ms()],
        )?;
        Ok(())
    }

    pub fn workspace(&self, name: &str) -> Result<Option<WorkspaceRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT name, folder, kind, definition, saved_ms FROM workspaces WHERE name = ?1",
                params![name],
                |r| {
                    Ok(WorkspaceRow {
                        name: r.get(0)?,
                        folder: r.get(1)?,
                        kind: r.get(2)?,
                        definition: r.get(3)?,
                        saved_ms: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn workspaces(&self) -> Result<Vec<WorkspaceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, folder, kind, definition, saved_ms FROM workspaces ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(WorkspaceRow {
                name: r.get(0)?,
                folder: r.get(1)?,
                kind: r.get(2)?,
                definition: r.get(3)?,
                saved_ms: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete_workspace(&self, name: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM workspaces WHERE name = ?1", params![name])?
            > 0)
    }

    // small key value state

    pub fn state(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM herdr_state WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO herdr_state (key, value, updated_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_ms = ?3",
            params![key, value, now_ms()],
        )?;
        Ok(())
    }

    // contract test results

    pub fn record_contract_run(
        &self,
        herdr_app: &str,
        herdr_core: &str,
        passed: bool,
        detail: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO contract_runs (ts_ms, herdr_app, herdr_core, passed, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now_ms(), herdr_app, herdr_core, passed, detail],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(s: &Store) -> Vec<String> {
        let mut stmt = s.conn().prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn new_database_reaches_the_latest_schema() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.schema_version().unwrap(), Store::latest_version());
        assert_eq!(
            tables(&s),
            vec![
                "agent_events",
                "agent_runs",
                "agent_sessions",
                "contract_runs",
                "herdr_state",
                "workspaces"
            ]
        );
    }

    #[test]
    fn migrating_twice_changes_nothing() {
        let dir = crate::util::scratch_dir("store");
        let first = Store::open(&dir).unwrap();
        first.save_workspace("w", "/x", "snapshot", "{}").unwrap();
        drop(first);
        let second = Store::open(&dir).unwrap();
        assert_eq!(second.schema_version().unwrap(), Store::latest_version());
        assert_eq!(second.workspaces().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_database_is_refused() {
        let s = Store::open_in_memory().unwrap();
        s.conn()
            .pragma_update(None, "user_version", Store::latest_version() + 1)
            .unwrap();
        assert_eq!(s.migrate().unwrap_err().kind, "conflict");
    }

    #[test]
    fn workspaces_upsert_and_list() {
        let s = Store::open_in_memory().unwrap();
        s.save_workspace("b", "/b", "snapshot", "{\"v\":1}")
            .unwrap();
        s.save_workspace("a", "/a", "template", "{}").unwrap();
        s.save_workspace("b", "/b2", "snapshot", "{\"v\":2}")
            .unwrap();
        let all = s.workspaces().unwrap();
        assert_eq!(
            all.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(s.workspace("b").unwrap().unwrap().folder, "/b2");
        assert!(s.workspace("zzz").unwrap().is_none());
        assert!(s.delete_workspace("a").unwrap());
        assert!(!s.delete_workspace("a").unwrap());
    }

    #[test]
    fn state_and_contract_runs() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.state("k").unwrap().is_none());
        s.set_state("k", "1").unwrap();
        s.set_state("k", "2").unwrap();
        assert_eq!(s.state("k").unwrap().as_deref(), Some("2"));
        assert_eq!(
            s.record_contract_run("0.1.15", "0.1.20", true, "{}")
                .unwrap(),
            1
        );
    }

    #[test]
    fn two_connections_can_write_to_one_file() {
        let dir = crate::util::scratch_dir("wal");
        let a = Store::open(&dir).unwrap();
        let b = Store::open(&dir).unwrap();
        a.set_state("a", "1").unwrap();
        b.set_state("b", "2").unwrap();
        assert_eq!(a.state("b").unwrap().as_deref(), Some("2"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
