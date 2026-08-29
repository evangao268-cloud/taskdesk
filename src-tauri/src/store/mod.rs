mod migrations;

use std::path::Path;
use std::sync::Mutex;

use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{Settings, TaskDto, TaskStatus};
use crate::nudges::{NudgeAck, NudgeDef};

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct OutboxEntry {
    pub seq: i64,
    pub kind: String,
    pub task_local_id: String,
    pub payload: String,
    pub attempts: i64,
    pub next_attempt_at: Option<String>,
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ---- settings ----

    pub fn settings(&self) -> Settings {
        let conn = self.conn.lock().unwrap();
        let json: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| {
                r.get(0)
            })
            .optional()
            .unwrap_or(None);
        json.and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('app', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![serde_json::to_string(settings).unwrap()],
        )?;
        Ok(())
    }

    /// Arbitrary string state (sync watermarks, last shown date, account email).
    pub fn get_meta(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---- tasks ----

    pub fn open_tasks(&self) -> rusqlite::Result<Vec<TaskDto>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT local_id, title, notes, due_date FROM tasks
             WHERE deleted = 0 AND status = 'needsAction'
             ORDER BY due_date IS NULL, due_date, title",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TaskDto {
                local_id: r.get(0)?,
                title: r.get(1)?,
                notes: r.get(2)?,
                due_date: r.get(3)?,
                status: TaskStatus::NeedsAction,
            })
        })?;
        rows.collect()
    }

    pub fn insert_local_task(&self, task: &TaskDto, tasklist_id: &str) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO tasks (local_id, tasklist_id, title, notes, due_date, status, updated, dirty)
             VALUES (?1, ?2, ?3, ?4, ?5, 'needsAction', ?6, 1)",
            params![task.local_id, tasklist_id, task.title, task.notes, task.due_date, now_rfc3339()],
        )?;
        tx.execute(
            "INSERT INTO outbox (kind, task_local_id, payload, created_at)
             VALUES ('create', ?1, ?2, ?3)",
            params![
                task.local_id,
                serde_json::to_string(task).unwrap(),
                now_rfc3339()
            ],
        )?;
        tx.commit()
    }

    pub fn set_task_status(&self, local_id: &str, status: TaskStatus) -> rusqlite::Result<TaskDto> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let (status_str, completed_at, kind) = match status {
            TaskStatus::Completed => ("completed", Some(now_rfc3339()), "complete"),
            TaskStatus::NeedsAction => ("needsAction", None, "update"),
        };
        let changed = tx.execute(
            "UPDATE tasks SET status = ?1, completed_at = ?2, updated = ?3, dirty = 1
             WHERE local_id = ?4 AND deleted = 0",
            params![status_str, completed_at, now_rfc3339(), local_id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        let task = tx.query_row(
            "SELECT local_id, title, notes, due_date FROM tasks WHERE local_id = ?1",
            params![local_id],
            |r| {
                Ok(TaskDto {
                    local_id: r.get(0)?,
                    title: r.get(1)?,
                    notes: r.get(2)?,
                    due_date: r.get(3)?,
                    status,
                })
            },
        )?;
        tx.execute(
            "INSERT INTO outbox (kind, task_local_id, payload, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                kind,
                local_id,
                serde_json::to_string(&task).unwrap(),
                now_rfc3339()
            ],
        )?;
        tx.commit()?;
        Ok(task)
    }

    // ---- nudges ----

    pub fn nudge_defs(&self) -> rusqlite::Result<Vec<NudgeDef>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, interval_days, anchor_date, create_task_on_ack, enabled
             FROM nudges ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(NudgeDef {
                id: r.get(0)?,
                title: r.get(1)?,
                interval_days: r.get(2)?,
                anchor_date: parse_date(r.get::<_, String>(3)?, 3)?,
                create_task_on_ack: r.get(4)?,
                enabled: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn nudge_acks(&self) -> rusqlite::Result<Vec<NudgeAck>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT nudge_id, acked_on FROM nudge_acks")?;
        let rows = stmt.query_map([], |r| {
            Ok(NudgeAck {
                nudge_id: r.get(0)?,
                acked_on: parse_date(r.get::<_, String>(1)?, 1)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_nudge(&self, def: &NudgeDef) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO nudges (id, title, interval_days, anchor_date, create_task_on_ack, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               interval_days = excluded.interval_days,
               anchor_date = excluded.anchor_date,
               create_task_on_ack = excluded.create_task_on_ack,
               enabled = excluded.enabled",
            params![
                def.id,
                def.title,
                def.interval_days,
                def.anchor_date.to_string(),
                def.create_task_on_ack,
                def.enabled,
                now_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn delete_nudge(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM nudges WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Ack a nudge; optionally create a task (due today) in the same transaction.
    pub fn ack_nudge(
        &self,
        nudge_id: &str,
        today: NaiveDate,
        task: Option<(&TaskDto, &str)>,
    ) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let action = if task.is_some() { "task_created" } else { "done" };
        tx.execute(
            "INSERT INTO nudge_acks (nudge_id, acked_on, action) VALUES (?1, ?2, ?3)
             ON CONFLICT(nudge_id, acked_on) DO UPDATE SET action = excluded.action",
            params![nudge_id, today.to_string(), action],
        )?;
        if let Some((t, tasklist_id)) = task {
            tx.execute(
                "INSERT INTO tasks (local_id, tasklist_id, title, notes, due_date, status, updated, dirty)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'needsAction', ?6, 1)",
                params![t.local_id, tasklist_id, t.title, t.notes, t.due_date, now_rfc3339()],
            )?;
            tx.execute(
                "INSERT INTO outbox (kind, task_local_id, payload, created_at)
                 VALUES ('create', ?1, ?2, ?3)",
                params![t.local_id, serde_json::to_string(t).unwrap(), now_rfc3339()],
            )?;
        }
        tx.commit()
    }


    // ---- sync support ----

    pub fn upsert_tasklist(&self, id: &str, title: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tasklists (id, title) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title",
            params![id, title],
        )?;
        Ok(())
    }

    pub fn tasklists(&self) -> rusqlite::Result<Vec<(String, Option<String>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, sync_watermark FROM tasklists")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    pub fn set_watermark(&self, tasklist_id: &str, watermark: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasklists SET sync_watermark = ?2 WHERE id = ?1",
            params![tasklist_id, watermark],
        )?;
        Ok(())
    }

    /// Is this local row dirty (pending local changes)? None if unknown id.
    pub fn task_by_google_id(&self, google_id: &str) -> rusqlite::Result<Option<(String, bool)>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT local_id, dirty FROM tasks WHERE google_id = ?1",
            params![google_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
    }

    /// Apply a remote (non-deleted) task to the cache. Caller must have
    /// already skipped dirty rows.
    pub fn apply_remote_task(
        &self,
        google_id: &str,
        tasklist_id: &str,
        title: &str,
        notes: Option<&str>,
        due_date: Option<&str>,
        status: &str,
        updated: Option<&str>,
        position: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<String> = conn
            .query_row(
                "SELECT local_id FROM tasks WHERE google_id = ?1",
                params![google_id],
                |r| r.get(0),
            )
            .optional()?;
        match existing {
            Some(local_id) => {
                conn.execute(
                    "UPDATE tasks SET title = ?2, notes = ?3, due_date = ?4, status = ?5,
                     updated = ?6, position = ?7, tasklist_id = ?8, deleted = 0
                     WHERE local_id = ?1",
                    params![local_id, title, notes, due_date, status, updated, position, tasklist_id],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO tasks (local_id, google_id, tasklist_id, title, notes, due_date,
                     status, updated, position, deleted, dirty)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0)",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        google_id,
                        tasklist_id,
                        title,
                        notes,
                        due_date,
                        status,
                        updated,
                        position
                    ],
                )?;
            }
        }
        Ok(())
    }

    /// Tombstone by google id (remote deletion); dirty rows are left alone.
    pub fn tombstone_remote(&self, google_id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET deleted = 1 WHERE google_id = ?1 AND dirty = 0",
            params![google_id],
        )?;
        Ok(())
    }

    pub fn tombstone_local(&self, local_id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET deleted = 1 WHERE local_id = ?1",
            params![local_id],
        )?;
        Ok(())
    }

    pub fn set_google_id(
        &self,
        local_id: &str,
        google_id: &str,
        updated: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET google_id = ?2, updated = COALESCE(?3, updated) WHERE local_id = ?1",
            params![local_id, google_id, updated],
        )?;
        Ok(())
    }

    /// (google_id, tasklist_id, status, title, notes, due_date) for a push.
    pub fn task_push_info(
        &self,
        local_id: &str,
    ) -> rusqlite::Result<Option<(Option<String>, String, String, String, Option<String>, Option<String>)>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT google_id, tasklist_id, status, title, notes, due_date
             FROM tasks WHERE local_id = ?1",
            params![local_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()
    }

    /// Repoint cached tasks from a placeholder list id (e.g. "@default") to the
    /// real Google list id once known.
    pub fn reassign_tasklist(&self, from: &str, to: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET tasklist_id = ?2 WHERE tasklist_id = ?1",
            params![from, to],
        )?;
        Ok(())
    }

    // ---- outbox ----

    pub fn outbox_pending(&self, now: &str) -> rusqlite::Result<Vec<OutboxEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, kind, task_local_id, payload, attempts, next_attempt_at
             FROM outbox
             WHERE next_attempt_at IS NULL OR next_attempt_at <= ?1
             ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![now], |r| {
            Ok(OutboxEntry {
                seq: r.get(0)?,
                kind: r.get(1)?,
                task_local_id: r.get(2)?,
                payload: r.get(3)?,
                attempts: r.get(4)?,
                next_attempt_at: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn outbox_count(&self) -> i64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Remove a drained entry and clear `dirty` when it was the task's last one.
    pub fn outbox_remove(&self, seq: i64) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let task_local_id: Option<String> = tx
            .query_row(
                "SELECT task_local_id FROM outbox WHERE seq = ?1",
                params![seq],
                |r| r.get(0),
            )
            .optional()?;
        tx.execute("DELETE FROM outbox WHERE seq = ?1", params![seq])?;
        if let Some(id) = task_local_id {
            tx.execute(
                "UPDATE tasks SET dirty = 0 WHERE local_id = ?1
                 AND NOT EXISTS (SELECT 1 FROM outbox WHERE task_local_id = ?1)",
                params![id],
            )?;
        }
        tx.commit()
    }

    pub fn outbox_defer(
        &self,
        seq: i64,
        next_attempt_at: &str,
        error: &str,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE outbox SET attempts = attempts + 1, next_attempt_at = ?2, last_error = ?3
             WHERE seq = ?1",
            params![seq, next_attempt_at, error],
        )?;
        Ok(())
    }
}

fn parse_date(s: String, col: usize) -> Result<NaiveDate, rusqlite::Error> {
    s.parse().map_err(|e: chrono::ParseError| {
        rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, Box::new(e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TaskStatus;

    fn task(id: &str, due: Option<&str>) -> TaskDto {
        TaskDto {
            local_id: id.into(),
            title: format!("task {id}"),
            notes: None,
            due_date: due.map(String::from),
            status: TaskStatus::NeedsAction,
        }
    }

    #[test]
    fn settings_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let mut s = store.settings();
        assert_eq!(s.pause_seconds, 7); // default when unset
        s.pause_seconds = 12;
        store.save_settings(&s).unwrap();
        assert_eq!(store.settings().pause_seconds, 12);
    }

    #[test]
    fn insert_creates_outbox_and_dirty() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_local_task(&task("t1", Some("2026-08-29")), "@default")
            .unwrap();
        let pending = store.outbox_pending(&now_rfc3339()).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, "create");
        assert_eq!(pending[0].task_local_id, "t1");
    }

    #[test]
    fn outbox_fifo_and_dirty_lifecycle() {
        let store = Store::open_in_memory().unwrap();
        store.insert_local_task(&task("t1", None), "@default").unwrap();
        store.set_task_status("t1", TaskStatus::Completed).unwrap();
        let pending = store.outbox_pending(&now_rfc3339()).unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].kind, "create");
        assert_eq!(pending[1].kind, "complete");

        // Draining the first entry keeps the task dirty; draining both clears it.
        store.outbox_remove(pending[0].seq).unwrap();
        {
            let conn = store.conn.lock().unwrap();
            let dirty: bool = conn
                .query_row("SELECT dirty FROM tasks WHERE local_id = 't1'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert!(dirty);
        }
        store.outbox_remove(pending[1].seq).unwrap();
        let conn = store.conn.lock().unwrap();
        let dirty: bool = conn
            .query_row("SELECT dirty FROM tasks WHERE local_id = 't1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(!dirty);
    }

    #[test]
    fn deferred_outbox_entry_not_pending() {
        let store = Store::open_in_memory().unwrap();
        store.insert_local_task(&task("t1", None), "@default").unwrap();
        let pending = store.outbox_pending(&now_rfc3339()).unwrap();
        store
            .outbox_defer(pending[0].seq, "2999-01-01T00:00:00Z", "rate limited")
            .unwrap();
        assert_eq!(store.outbox_pending(&now_rfc3339()).unwrap().len(), 0);
        assert_eq!(store.outbox_count(), 1);
    }

    #[test]
    fn nudge_ack_with_task_is_atomic() {
        let store = Store::open_in_memory().unwrap();
        let def = crate::nudges::NudgeDef {
            id: "n1".into(),
            title: "Call Mom".into(),
            interval_days: 14,
            anchor_date: "2026-08-01".parse().unwrap(),
            create_task_on_ack: true,
            enabled: true,
        };
        store.upsert_nudge(&def).unwrap();
        let t = task("nt1", Some("2026-08-29"));
        store
            .ack_nudge("n1", "2026-08-29".parse().unwrap(), Some((&t, "@default")))
            .unwrap();
        assert_eq!(store.nudge_acks().unwrap().len(), 1);
        assert_eq!(store.open_tasks().unwrap().len(), 1);
        assert_eq!(store.outbox_count(), 1);
    }

    #[test]
    fn migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrations::migrate(&conn).unwrap();
        migrations::migrate(&conn).unwrap();
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }
}
