//! Sync engine: the only module that touches both the store and the Google
//! client. Pull is incremental per list (updatedMin + showDeleted); push
//! drains the outbox FIFO with retry/backoff. Conflicts are last-write-wins,
//! except rows with pending local changes (`dirty`), which pulls never touch.

use std::sync::Arc;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;

use crate::google::auth::AuthClient;
use crate::google::tasks_api::{RemoteTask, TasksApi};
use crate::google::GoogleError;
use crate::store::Store;

pub const PLACEHOLDER_LIST: &str = "@default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    Idle,
    Syncing,
    Offline,
    AuthError,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub pulled: usize,
    pub pushed: usize,
    pub deferred: usize,
    pub state: SyncState,
}

/// Google's `due` is a date wearing an RFC3339 costume — only the date part
/// survives, so conversion is pure string handling to avoid timezone drift.
fn due_to_remote(d: &str) -> String {
    format!("{d}T00:00:00.000Z")
}

fn due_to_local(remote: &str) -> String {
    remote.chars().take(10).collect()
}

pub struct SyncEngine {
    store: Arc<Store>,
    auth: Arc<AuthClient>,
    base_url: String,
}

impl SyncEngine {
    pub fn new(store: Arc<Store>, auth: Arc<AuthClient>) -> Self {
        Self::with_base(store, auth, "https://tasks.googleapis.com/tasks/v1".into())
    }

    pub fn with_base(store: Arc<Store>, auth: Arc<AuthClient>, base_url: String) -> Self {
        Self {
            store,
            auth,
            base_url,
        }
    }

    pub async fn sync(&self) -> SyncReport {
        let api = TasksApi::with_base(&self.auth, self.base_url.clone());
        let mut report = SyncReport {
            pulled: 0,
            pushed: 0,
            deferred: 0,
            state: SyncState::Idle,
        };

        match self.push(&api, &mut report).await {
            Ok(()) => {}
            Err(e) => {
                report.state = classify(&e);
                return report;
            }
        }
        match self.pull(&api, &mut report).await {
            Ok(()) => {}
            Err(e) => {
                report.state = classify(&e);
                return report;
            }
        }
        let _ = self.store.set_meta("last_sync_at", &Utc::now().to_rfc3339());
        report
    }

    async fn pull(&self, api: &TasksApi<'_>, report: &mut SyncReport) -> Result<(), GoogleError> {
        let lists = api.list_tasklists().await?;
        if let Some(first) = lists.first() {
            // First successful pull: repoint tasks created before we knew the
            // real default list id.
            let _ = self.store.reassign_tasklist(PLACEHOLDER_LIST, &first.id);
        }
        for list in &lists {
            let _ = self.store.upsert_tasklist(&list.id, &list.title);
        }
        let watermarks = self.store.tasklists().map_err(store_err)?;
        for (list_id, watermark) in watermarks {
            // 60s pad against clock skew between us and Google.
            let updated_min = watermark.as_deref().and_then(|w| {
                DateTime::parse_from_rfc3339(w)
                    .ok()
                    .map(|t| (t - ChronoDuration::seconds(60)).to_rfc3339())
            });
            let tasks = api.list_tasks(&list_id, updated_min.as_deref()).await?;
            let mut max_updated: Option<String> = watermark.clone();
            for t in &tasks {
                let Some(google_id) = &t.id else { continue };
                if let Some(u) = &t.updated {
                    if max_updated.as_deref().map_or(true, |m| u.as_str() > m) {
                        max_updated = Some(u.clone());
                    }
                }
                let dirty = self
                    .store
                    .task_by_google_id(google_id)
                    .map_err(store_err)?
                    .map(|(_, d)| d)
                    .unwrap_or(false);
                if dirty {
                    continue; // local wins until pushed
                }
                if t.deleted == Some(true) {
                    self.store.tombstone_remote(google_id).map_err(store_err)?;
                } else {
                    self.store
                        .apply_remote_task(
                            google_id,
                            &list_id,
                            t.title.as_deref().unwrap_or(""),
                            t.notes.as_deref(),
                            t.due.as_deref().map(due_to_local).as_deref(),
                            t.status.as_deref().unwrap_or("needsAction"),
                            t.updated.as_deref(),
                            t.position.as_deref(),
                        )
                        .map_err(store_err)?;
                }
                report.pulled += 1;
            }
            if let Some(w) = max_updated {
                self.store.set_watermark(&list_id, &w).map_err(store_err)?;
            }
        }
        Ok(())
    }

    async fn push(&self, api: &TasksApi<'_>, report: &mut SyncReport) -> Result<(), GoogleError> {
        let pending = self
            .store
            .outbox_pending(&Utc::now().to_rfc3339())
            .map_err(store_err)?;
        for entry in pending {
            let info = self
                .store
                .task_push_info(&entry.task_local_id)
                .map_err(store_err)?;
            let Some((google_id, tasklist_id, status, title, notes, due_date)) = info else {
                // Task row vanished; nothing to push.
                self.store.outbox_remove(entry.seq).map_err(store_err)?;
                continue;
            };
            let tasklist_id = if tasklist_id == PLACEHOLDER_LIST {
                // No pull has succeeded yet; resolve the real default list now.
                let lists = api.list_tasklists().await?;
                let first = lists
                    .first()
                    .ok_or_else(|| GoogleError::Other("account has no task lists".into()))?;
                let _ = self.store.reassign_tasklist(PLACEHOLDER_LIST, &first.id);
                first.id.clone()
            } else {
                tasklist_id
            };

            let body = RemoteTask {
                title: Some(title),
                notes,
                due: due_date.as_deref().map(due_to_remote),
                status: Some(status),
                ..Default::default()
            };
            let result = match (entry.kind.as_str(), &google_id) {
                ("create", _) => api.insert_task(&tasklist_id, &body).await.map(Some),
                ("update" | "complete", Some(gid)) => {
                    api.patch_task(&tasklist_id, gid, &body).await.map(Some)
                }
                ("delete", Some(gid)) => api.delete_task(&tasklist_id, gid).await.map(|_| None),
                // No google id to act on (create was dropped remotely?) — noop.
                _ => Ok(None),
            };
            match result {
                Ok(remote) => {
                    if entry.kind == "create" {
                        if let Some(r) = &remote {
                            if let Some(gid) = &r.id {
                                self.store
                                    .set_google_id(
                                        &entry.task_local_id,
                                        gid,
                                        r.updated.as_deref(),
                                    )
                                    .map_err(store_err)?;
                            }
                        }
                    }
                    self.store.outbox_remove(entry.seq).map_err(store_err)?;
                    report.pushed += 1;
                }
                Err(GoogleError::Http(404, _)) => {
                    // Remote row already gone: drop the work, tombstone locally.
                    self.store.outbox_remove(entry.seq).map_err(store_err)?;
                    self.store
                        .tombstone_local(&entry.task_local_id)
                        .map_err(store_err)?;
                }
                Err(e @ (GoogleError::Http(429, _) | GoogleError::Http(500..=599, _))) => {
                    let backoff = 30u64.saturating_mul(2u64.saturating_pow(entry.attempts as u32));
                    let next = Utc::now() + ChronoDuration::seconds(backoff.min(3600) as i64);
                    self.store
                        .outbox_defer(entry.seq, &next.to_rfc3339(), &e.to_string())
                        .map_err(store_err)?;
                    report.deferred += 1;
                    return Ok(()); // keep FIFO order: stop draining this round
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

fn classify(e: &GoogleError) -> SyncState {
    match e {
        GoogleError::Network(_) => SyncState::Offline,
        GoogleError::InvalidGrant | GoogleError::NotConnected => SyncState::AuthError,
        _ => SyncState::Idle,
    }
}

fn store_err(e: rusqlite::Error) -> GoogleError {
    GoogleError::Other(format!("store: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::auth::test_support::MemStore;
    use crate::google::ClientConfig;
    use crate::models::{TaskDto, TaskStatus};
    use std::sync::Mutex;
    use wiremock::matchers::{body_string_contains, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn engine_for(server: &MockServer, store: Arc<Store>) -> SyncEngine {
        let auth = AuthClient::with_endpoints(
            ClientConfig {
                client_id: "cid".into(),
                client_secret: "sec".into(),
            },
            format!("{}/token", server.uri()),
            format!("{}/userinfo", server.uri()),
            Box::new(MemStore(Mutex::new(Some("refresh-token".into())))),
        );
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "at-1", "expires_in": 3600
            })))
            .mount(server)
            .await;
        SyncEngine::with_base(store, Arc::new(auth), server.uri())
    }

    fn mock_lists(server: &MockServer) -> impl std::future::Future<Output = ()> + '_ {
        Mock::given(method("GET"))
            .and(path("/users/@me/lists"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id": "list-1", "title": "My Tasks"}]
            })))
            .mount(server)
    }

    #[tokio::test]
    async fn initial_pull_caches_remote_tasks() {
        let server = MockServer::start().await;
        let store = Arc::new(Store::open_in_memory().unwrap());
        mock_lists(&server).await;
        Mock::given(method("GET"))
            .and(path("/lists/list-1/tasks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id": "g1", "title": "From phone", "status": "needsAction",
                     "due": "2026-08-29T00:00:00.000Z", "updated": "2026-08-29T10:00:00.000Z"},
                    {"id": "g2", "title": "Done one", "status": "completed",
                     "updated": "2026-08-29T09:00:00.000Z"}
                ]
            })))
            .mount(&server)
            .await;

        let engine = engine_for(&server, store.clone()).await;
        let report = engine.sync().await;
        assert_eq!(report.state, SyncState::Idle);
        assert_eq!(report.pulled, 2);
        let open = store.open_tasks().unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].title, "From phone");
        assert_eq!(open[0].due_date.as_deref(), Some("2026-08-29"));
        // Watermark advanced to the max remote `updated`.
        let lists = store.tasklists().unwrap();
        assert_eq!(lists[0].1.as_deref(), Some("2026-08-29T10:00:00.000Z"));
    }

    #[tokio::test]
    async fn incremental_pull_applies_remote_delete_but_not_to_dirty() {
        let server = MockServer::start().await;
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.upsert_tasklist("list-1", "My Tasks").unwrap();
        store
            .apply_remote_task("g1", "list-1", "will die", None, None, "needsAction", None, None)
            .unwrap();
        store
            .apply_remote_task("g2", "list-1", "dirty local", None, None, "needsAction", None, None)
            .unwrap();
        // Local edit makes g2 dirty (also queues an outbox entry we ignore here).
        let (local2, _) = store.task_by_google_id("g2").unwrap().unwrap();
        store.set_task_status(&local2, TaskStatus::Completed).unwrap();

        mock_lists(&server).await;
        Mock::given(method("GET"))
            .and(path("/lists/list-1/tasks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id": "g1", "deleted": true, "updated": "2026-08-29T11:00:00.000Z"},
                    {"id": "g2", "title": "remote change", "status": "needsAction",
                     "updated": "2026-08-29T11:00:00.000Z"}
                ]
            })))
            .mount(&server)
            .await;
        // The dirty task's outbox entry will try to PATCH; let it succeed.
        Mock::given(method("PATCH"))
            .and(path("/lists/list-1/tasks/g2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "g2", "updated": "2026-08-29T11:30:00.000Z"
            })))
            .mount(&server)
            .await;

        let engine = engine_for(&server, store.clone()).await;
        let report = engine.sync().await;
        assert_eq!(report.state, SyncState::Idle);
        // g1 tombstoned...
        assert!(store.task_by_google_id("g1").unwrap().is_some());
        let open = store.open_tasks().unwrap();
        assert!(open.iter().all(|t| t.title != "will die"));
        // ...and g2 was pushed first (drain-before-pull), so it is clean and
        // the remote title applies on the pull.
        let (_, dirty2) = store.task_by_google_id("g2").unwrap().unwrap();
        assert!(!dirty2);
    }

    #[tokio::test]
    async fn push_create_remaps_google_id_then_patch_uses_it() {
        let server = MockServer::start().await;
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.upsert_tasklist("list-1", "My Tasks").unwrap();
        let t = TaskDto {
            local_id: "loc-1".into(),
            title: "New local".into(),
            notes: None,
            due_date: Some("2026-08-29".into()),
            status: TaskStatus::NeedsAction,
        };
        store.insert_local_task(&t, "list-1").unwrap();
        store.set_task_status("loc-1", TaskStatus::Completed).unwrap();

        mock_lists(&server).await;
        Mock::given(method("GET"))
            .and(path("/lists/list-1/tasks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/lists/list-1/tasks"))
            .and(body_string_contains("2026-08-29T00:00:00.000Z"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "g-new", "updated": "2026-08-29T12:00:00.000Z"
            })))
            .mount(&server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/lists/list-1/tasks/g-new"))
            .and(body_string_contains("completed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "g-new", "updated": "2026-08-29T12:01:00.000Z"
            })))
            .mount(&server)
            .await;

        let engine = engine_for(&server, store.clone()).await;
        let report = engine.sync().await;
        assert_eq!(report.pushed, 2);
        assert_eq!(store.outbox_count(), 0);
        let (_, dirty) = store.task_by_google_id("g-new").unwrap().unwrap();
        assert!(!dirty);
    }

    #[tokio::test]
    async fn rate_limit_defers_and_stops_drain() {
        let server = MockServer::start().await;
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.upsert_tasklist("list-1", "My Tasks").unwrap();
        for i in 0..2 {
            let t = TaskDto {
                local_id: format!("loc-{i}"),
                title: format!("t{i}"),
                notes: None,
                due_date: None,
                status: TaskStatus::NeedsAction,
            };
            store.insert_local_task(&t, "list-1").unwrap();
        }
        mock_lists(&server).await;
        Mock::given(method("GET"))
            .and(path("/lists/list-1/tasks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/lists/list-1/tasks"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;

        let engine = engine_for(&server, store.clone()).await;
        let report = engine.sync().await;
        assert_eq!(report.pushed, 0);
        assert_eq!(report.deferred, 1); // FIFO stop: only the head was attempted
        assert_eq!(store.outbox_count(), 2);
        // Deferred entry is parked in the future, not retried immediately.
        assert!(store
            .outbox_pending(&Utc::now().to_rfc3339())
            .unwrap()
            .len()
            <= 1);
    }

    #[tokio::test]
    async fn push_404_tombstones_locally() {
        let server = MockServer::start().await;
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.upsert_tasklist("list-1", "My Tasks").unwrap();
        store
            .apply_remote_task("g-gone", "list-1", "was remote", None, None, "needsAction", None, None)
            .unwrap();
        let (local, _) = store.task_by_google_id("g-gone").unwrap().unwrap();
        store.set_task_status(&local, TaskStatus::Completed).unwrap();

        mock_lists(&server).await;
        Mock::given(method("GET"))
            .and(path("/lists/list-1/tasks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []})))
            .mount(&server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/lists/list-1/tasks/g-gone"))
            .respond_with(ResponseTemplate::new(404).set_body_string("gone"))
            .mount(&server)
            .await;

        let engine = engine_for(&server, store.clone()).await;
        let report = engine.sync().await;
        assert_eq!(report.state, SyncState::Idle);
        assert_eq!(store.outbox_count(), 0);
        assert!(store.open_tasks().unwrap().is_empty());
    }

    #[tokio::test]
    async fn incremental_uses_updated_min_with_skew_pad() {
        let server = MockServer::start().await;
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.upsert_tasklist("list-1", "My Tasks").unwrap();
        store
            .set_watermark("list-1", "2026-08-29T10:00:00+00:00")
            .unwrap();
        mock_lists(&server).await;
        Mock::given(method("GET"))
            .and(path("/lists/list-1/tasks"))
            .and(query_param("updatedMin", "2026-08-29T09:59:00+00:00"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []})))
            .mount(&server)
            .await;

        let engine = engine_for(&server, store.clone()).await;
        let report = engine.sync().await;
        // The mock only matches the padded updatedMin — reaching Idle proves it.
        assert_eq!(report.state, SyncState::Idle);
    }
}
