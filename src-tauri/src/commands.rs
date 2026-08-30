use tauri::{AppHandle, Emitter, Manager, State};

use crate::google::GoogleError;
use crate::models::*;
use crate::nudges::{self, NudgeDef};
use crate::window;
use crate::AppState;

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

fn today_str() -> String {
    today().format("%Y-%m-%d").to_string()
}

fn mark_engaged(state: &AppState) {
    state.gate.lock().unwrap().engaged = true;
}

fn err_str(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn default_tasklist(settings: &Settings) -> String {
    settings
        .default_tasklist_id
        .clone()
        .unwrap_or_else(|| "@default".into())
}

#[tauri::command]
pub fn get_boot_view(state: State<AppState>) -> Result<BootView, String> {
    let dismiss = window::dismiss_state(&state);
    let settings = state.store.settings();
    let defs = state.store.nudge_defs().map_err(err_str)?;
    let acks = state.store.nudge_acks().map_err(err_str)?;
    let due = nudges::due_nudges(&defs, &acks, today());
    let tasks = state.store.open_tasks().map_err(err_str)?;
    let sync = SyncSummary {
        state: *state.sync_state.lock().unwrap(),
        pending_outbox: state.store.outbox_count(),
        last_sync_at: state.store.get_meta("last_sync_at"),
        connected: state
            .auth
            .lock()
            .unwrap()
            .as_ref()
            .map(|a| a.is_connected())
            .unwrap_or(false),
        email: settings.google_account_email.clone(),
    };

    let today = today_str();
    let mut view = BootView {
        today: vec![],
        overdue: vec![],
        nudges: due,
        undated: vec![],
        settings,
        dismiss,
        sync,
    };
    for t in tasks {
        match &t.due_date {
            Some(d) if *d < today => view.overdue.push(t),
            Some(d) if *d == today => view.today.push(t),
            Some(_) => {} // future-dated tasks are not part of the boot view
            None => view.undated.push(t),
        }
    }
    Ok(view)
}

#[tauri::command]
pub fn add_task(
    app: AppHandle,
    state: State<AppState>,
    title: String,
    notes: Option<String>,
    due_date: Option<String>,
) -> Result<TaskDto, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Task title cannot be empty".into());
    }
    let task = TaskDto {
        local_id: uuid::Uuid::new_v4().to_string(),
        title,
        notes,
        due_date,
        status: TaskStatus::NeedsAction,
    };
    let tasklist = default_tasklist(&state.store.settings());
    state
        .store
        .insert_local_task(&task, &tasklist)
        .map_err(err_str)?;
    mark_engaged(&state);
    state.sync_kick.notify_one();
    let _ = app.emit("tasks-changed", ());
    Ok(task)
}

#[tauri::command]
pub fn complete_task(
    app: AppHandle,
    state: State<AppState>,
    local_id: String,
) -> Result<TaskDto, String> {
    let task = state
        .store
        .set_task_status(&local_id, TaskStatus::Completed)
        .map_err(err_str)?;
    mark_engaged(&state);
    state.sync_kick.notify_one();
    let _ = app.emit("tasks-changed", ());
    Ok(task)
}

#[tauri::command]
pub fn list_nudges(state: State<AppState>) -> Result<Vec<NudgeDef>, String> {
    state.store.nudge_defs().map_err(err_str)
}

#[tauri::command]
pub fn add_nudge(
    app: AppHandle,
    state: State<AppState>,
    title: String,
    interval_days: u32,
    create_task_on_ack: bool,
) -> Result<NudgeDef, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Nudge title cannot be empty".into());
    }
    if interval_days < 1 {
        return Err("Interval must be at least 1 day".into());
    }
    let def = NudgeDef {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        interval_days,
        anchor_date: today(),
        create_task_on_ack,
        enabled: true,
    };
    state.store.upsert_nudge(&def).map_err(err_str)?;
    let _ = app.emit("tasks-changed", ());
    Ok(def)
}

#[tauri::command]
pub fn update_nudge(
    app: AppHandle,
    state: State<AppState>,
    def: NudgeDef,
) -> Result<(), String> {
    state.store.upsert_nudge(&def).map_err(err_str)?;
    let _ = app.emit("tasks-changed", ());
    Ok(())
}

#[tauri::command]
pub fn delete_nudge(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    state.store.delete_nudge(&id).map_err(err_str)?;
    let _ = app.emit("tasks-changed", ());
    Ok(())
}

#[tauri::command]
pub fn ack_nudge(
    app: AppHandle,
    state: State<AppState>,
    nudge_id: String,
    create_task: bool,
) -> Result<(), String> {
    let task_and_list = if create_task {
        let defs = state.store.nudge_defs().map_err(err_str)?;
        let def = defs
            .iter()
            .find(|d| d.id == nudge_id)
            .ok_or_else(|| format!("No nudge with id {nudge_id}"))?;
        Some((
            TaskDto {
                local_id: uuid::Uuid::new_v4().to_string(),
                title: def.title.clone(),
                notes: None,
                due_date: Some(today_str()),
                status: TaskStatus::NeedsAction,
            },
            default_tasklist(&state.store.settings()),
        ))
    } else {
        None
    };
    state
        .store
        .ack_nudge(
            &nudge_id,
            today(),
            task_and_list.as_ref().map(|(t, l)| (t, l.as_str())),
        )
        .map_err(err_str)?;
    mark_engaged(&state);
    state.sync_kick.notify_one();
    let _ = app.emit("tasks-changed", ());
    Ok(())
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub connected: bool,
    pub email: Option<String>,
    pub config_present: bool,
}

#[tauri::command]
pub fn get_auth_status(state: State<AppState>) -> AuthStatus {
    let config_present = crate::google::ClientConfig::load(&state.data_dir).is_some();
    let connected = state
        .auth
        .lock()
        .unwrap()
        .as_ref()
        .map(|a| a.is_connected())
        .unwrap_or(false);
    AuthStatus {
        connected,
        email: state.store.settings().google_account_email,
        config_present,
    }
}

#[tauri::command]
pub async fn start_google_auth(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AuthStatus, String> {
    let auth = state.ensure_auth().map_err(err_str)?;
    let opener = app.clone();
    // The consent page opens in the system browser behind this always-on-top
    // window; stop occluding it for the duration of the sign-in.
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(false);
    }
    let result = auth
        .interactive_signin(move |url| {
            use tauri_plugin_opener::OpenerExt;
            opener
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|e| e.to_string())
        })
        .await;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(true);
        let _ = w.set_focus();
    }
    let email = result.map_err(err_str)?;
    let mut settings = state.store.settings();
    settings.google_account_email = if email.is_empty() { None } else { Some(email) };
    state.store.save_settings(&settings).map_err(err_str)?;
    state.sync_kick.notify_one();
    let _ = app.emit("sync-status-changed", ());
    Ok(get_auth_status(state))
}

#[tauri::command]
pub fn disconnect_google(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    if let Some(auth) = state.auth.lock().unwrap().as_ref() {
        auth.disconnect().map_err(err_str)?;
    }
    let mut settings = state.store.settings();
    settings.google_account_email = None;
    state.store.save_settings(&settings).map_err(err_str)?;
    let _ = app.emit("sync-status-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::sync::SyncReport, String> {
    let auth = state.ensure_auth().map_err(err_str)?;
    if !auth.is_connected() {
        return Err(GoogleError::NotConnected.to_string());
    }
    *state.sync_state.lock().unwrap() = crate::sync::SyncState::Syncing;
    let _ = app.emit("sync-status-changed", ());
    let engine = crate::sync::SyncEngine::new(state.store.clone(), auth);
    let report = engine.sync().await;
    *state.sync_state.lock().unwrap() = report.state;
    let _ = app.emit("sync-status-changed", ());
    let _ = app.emit("tasks-changed", ());
    Ok(report)
}

#[tauri::command]
pub fn nothing_today(state: State<AppState>) {
    mark_engaged(&state);
}

/// Mark the engage gate satisfied without any other side effect. The frontend
/// calls this on an optimistic (undoable) action so the close button unlocks
/// immediately instead of waiting for the deferred commit.
#[tauri::command]
pub fn engage(state: State<AppState>) {
    mark_engaged(&state);
}

#[tauri::command]
pub fn dismiss_window(app: AppHandle) -> DismissStateDto {
    window::try_dismiss(&app)
}

#[tauri::command]
pub fn get_dismiss_state(state: State<AppState>) -> DismissStateDto {
    window::dismiss_state(&state)
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.store.settings()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    {
        use tauri_plugin_autostart::ManagerExt;
        let autolaunch = app.autolaunch();
        let result = if settings.autostart_enabled {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if let Err(e) = result {
            log::warn!("autostart toggle failed: {e}");
        }
    }
    state.store.save_settings(&settings).map_err(err_str)?;
    let _ = app.emit("settings-changed", settings.clone());
    Ok(settings)
}
