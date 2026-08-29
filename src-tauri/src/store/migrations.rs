use rusqlite::Connection;

/// Sequential migrations; `PRAGMA user_version` records the last applied index.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE tasklists (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  updated TEXT,
  sync_watermark TEXT,
  is_default INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tasks (
  local_id TEXT PRIMARY KEY,
  google_id TEXT UNIQUE,
  tasklist_id TEXT NOT NULL,
  title TEXT NOT NULL,
  notes TEXT,
  due_date TEXT,
  status TEXT NOT NULL DEFAULT 'needsAction',
  completed_at TEXT,
  updated TEXT,
  position TEXT,
  deleted INTEGER NOT NULL DEFAULT 0,
  dirty INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_tasks_due ON tasks(due_date, status) WHERE deleted = 0;

CREATE TABLE nudges (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  interval_days INTEGER NOT NULL CHECK (interval_days >= 1),
  anchor_date TEXT NOT NULL,
  create_task_on_ack INTEGER NOT NULL DEFAULT 0,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL
);

CREATE TABLE nudge_acks (
  nudge_id TEXT NOT NULL REFERENCES nudges(id) ON DELETE CASCADE,
  acked_on TEXT NOT NULL,
  action TEXT NOT NULL,
  PRIMARY KEY (nudge_id, acked_on)
);

CREATE TABLE outbox (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  task_local_id TEXT NOT NULL,
  payload TEXT NOT NULL,
  created_at TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TEXT,
  last_error TEXT
);
"#];

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        conn.execute_batch(sql)?;
        conn.pragma_update(None, "user_version", i as i64 + 1)?;
    }
    Ok(())
}
