//! The SQLite database that holds what Git cannot express.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::Result;
use crate::model::{OpDetail, OpState, Operation, Origin, Task, Verification, Workspace};

const SCHEMA: &str = "
CREATE TABLE tasks (
    id         TEXT PRIMARY KEY,
    title      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    closed_at  INTEGER
);

CREATE TABLE workspaces (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    task_id    TEXT REFERENCES tasks(id),
    path       TEXT NOT NULL,
    base       TEXT NOT NULL,
    origin     TEXT NOT NULL CHECK (origin IN ('created', 'adopted')),
    created_at INTEGER NOT NULL,
    removed_at INTEGER
);
CREATE UNIQUE INDEX workspaces_active_name ON workspaces(name) WHERE removed_at IS NULL;
CREATE UNIQUE INDEX workspaces_active_path ON workspaces(path) WHERE removed_at IS NULL;

CREATE TABLE operations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL,
    state       TEXT NOT NULL,
    detail      TEXT NOT NULL,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    note        TEXT
);

CREATE TABLE verifications (
    tree        TEXT NOT NULL,
    command     TEXT NOT NULL,
    passed      INTEGER NOT NULL,
    exit_code   INTEGER,
    duration_ms INTEGER NOT NULL,
    finished_at INTEGER NOT NULL,
    PRIMARY KEY (tree, command)
);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

const SCHEMA_VERSION: i64 = 1;

pub struct Store {
    db: Connection,
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs() as i64
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        let db = Connection::open(path)?;
        // WAL lets the apps read while a command writes. The busy timeout
        // covers the short moments two writers meet outside the repo lock.
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "foreign_keys", true)?;
        db.busy_timeout(std::time::Duration::from_secs(10))?;

        let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version == 0 {
            db.execute_batch(&format!(
                "BEGIN; {SCHEMA} PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"
            ))?;
        }
        Ok(Store { db })
    }

    pub fn new_id(&self) -> Result<String> {
        Ok(self
            .db
            .query_row("SELECT lower(hex(randomblob(6)))", [], |row| row.get(0))?)
    }

    // meta

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    // tasks

    pub fn insert_task(&self, task: &Task) -> Result<()> {
        self.db.execute(
            "INSERT INTO tasks (id, title, created_at, closed_at) VALUES (?1, ?2, ?3, ?4)",
            params![task.id, task.title, task.created_at, task.closed_at],
        )?;
        Ok(())
    }

    pub fn tasks(&self) -> Result<Vec<Task>> {
        let mut statement = self.db.prepare(
            "SELECT id, title, created_at, closed_at FROM tasks ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Task {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                closed_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn close_task(&self, id: &str, at: Option<i64>) -> Result<()> {
        self.db.execute(
            "UPDATE tasks SET closed_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;
        Ok(())
    }

    pub fn delete_task(&self, id: &str) -> Result<()> {
        self.db.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(())
    }

    // workspaces

    pub fn insert_workspace(&self, workspace: &Workspace) -> Result<()> {
        self.db.execute(
            "INSERT INTO workspaces (id, name, task_id, path, base, origin, created_at, removed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                workspace.id,
                workspace.name,
                workspace.task_id,
                path_text(&workspace.path),
                workspace.base,
                match workspace.origin {
                    Origin::Created => "created",
                    Origin::Adopted => "adopted",
                },
                workspace.created_at,
                workspace.removed_at,
            ],
        )?;
        Ok(())
    }

    /// Every workspace, including removed ones, oldest first.
    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        let mut statement = self.db.prepare(
            "SELECT id, name, task_id, path, base, origin, created_at, removed_at
             FROM workspaces ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], workspace_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn workspace(&self, id: &str) -> Result<Option<Workspace>> {
        Ok(self
            .db
            .query_row(
                "SELECT id, name, task_id, path, base, origin, created_at, removed_at
                 FROM workspaces WHERE id = ?1",
                [id],
                workspace_from_row,
            )
            .optional()?)
    }

    pub fn set_workspace_removed(&self, id: &str, at: Option<i64>) -> Result<()> {
        self.db.execute(
            "UPDATE workspaces SET removed_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;
        Ok(())
    }

    // operations

    pub fn insert_operation(&self, detail: &OpDetail) -> Result<i64> {
        self.db.execute(
            "INSERT INTO operations (kind, state, detail, started_at) VALUES (?1, 'started', ?2, ?3)",
            params![detail.kind(), serde_json::to_string(detail)?, now()],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn update_operation_detail(&self, id: i64, detail: &OpDetail) -> Result<()> {
        self.db.execute(
            "UPDATE operations SET detail = ?2 WHERE id = ?1",
            params![id, serde_json::to_string(detail)?],
        )?;
        Ok(())
    }

    pub fn finish_operation(&self, id: i64, state: OpState, note: Option<&str>) -> Result<()> {
        self.db.execute(
            "UPDATE operations SET state = ?2, note = ?3, finished_at = ?4 WHERE id = ?1",
            params![id, state.as_str(), note, now()],
        )?;
        Ok(())
    }

    pub fn set_operation_state(&self, id: i64, state: OpState) -> Result<()> {
        self.db.execute(
            "UPDATE operations SET state = ?2 WHERE id = ?1",
            params![id, state.as_str()],
        )?;
        Ok(())
    }

    /// Operations, newest first.
    pub fn operations(&self, limit: u32) -> Result<Vec<Operation>> {
        self.query_operations("1 = 1 ORDER BY id DESC LIMIT ?1", params![limit])
    }

    pub fn operation(&self, id: i64) -> Result<Option<Operation>> {
        Ok(self
            .query_operations("id = ?1", params![id])?
            .into_iter()
            .next())
    }

    /// Operations a process started and did not finish, oldest first.
    pub fn started_operations(&self) -> Result<Vec<Operation>> {
        self.query_operations("state = 'started' ORDER BY id", params![])
    }

    /// The operation `tsugi undo` would revert.
    pub fn last_undoable_operation(&self) -> Result<Option<Operation>> {
        Ok(self
            .query_operations(
                "state = 'done' AND kind != 'undo' ORDER BY id DESC LIMIT 1",
                params![],
            )?
            .into_iter()
            .next())
    }

    fn query_operations(
        &self,
        condition: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<Operation>> {
        let mut statement = self.db.prepare(&format!(
            "SELECT id, state, detail, started_at, finished_at, note
             FROM operations WHERE {condition}"
        ))?;
        let rows = statement.query_map(params, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut operations = Vec::new();
        for row in rows {
            let (id, state, detail, started_at, finished_at, note) = row?;
            operations.push(Operation {
                id,
                state: OpState::parse(&state),
                detail: serde_json::from_str(&detail)?,
                started_at,
                finished_at,
                note,
            });
        }
        Ok(operations)
    }

    // verifications

    pub fn verification(&self, tree: &str, command: &str) -> Result<Option<Verification>> {
        Ok(self
            .db
            .query_row(
                "SELECT passed, exit_code, duration_ms FROM verifications
                 WHERE tree = ?1 AND command = ?2",
                [tree, command],
                |row| {
                    Ok(Verification {
                        command: command.to_string(),
                        passed: row.get(0)?,
                        exit_code: row.get(1)?,
                        duration_ms: row.get(2)?,
                        cached: true,
                    })
                },
            )
            .optional()?)
    }

    pub fn record_verification(&self, tree: &str, result: &Verification) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO verifications
             (tree, command, passed, exit_code, duration_ms, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                tree,
                result.command,
                result.passed,
                result.exit_code,
                result.duration_ms,
                now()
            ],
        )?;
        Ok(())
    }
}

fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        task_id: row.get(2)?,
        path: PathBuf::from(row.get::<_, String>(3)?),
        base: row.get(4)?,
        origin: if row.get::<_, String>(5)? == "created" {
            Origin::Created
        } else {
            Origin::Adopted
        },
        created_at: row.get(6)?,
        removed_at: row.get(7)?,
    })
}

fn path_text(path: &Path) -> String {
    path.to_str()
        .expect("workspace paths are validated as UTF-8 before they are stored")
        .to_string()
}
