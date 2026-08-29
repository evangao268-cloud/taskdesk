use tauri::{AppHandle, Emitter, State};

use crate::models::*;
use crate::window;
use crate::AppState;

fn today_str() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn mark_engaged(state: &AppState) {
    state.gate.lock().unwrap().engaged = true;
}

#[tauri::command]
pub fn get_boot_view(state: State<AppState>) -> BootView {
    log::info!("get_boot_view invoked");
    // dismiss_state locks `settings` internally, so take these sequentially:
    // guards created inside one struct expression all live to the end of the
    // statement, and a nested settings.lock() there self-deadlocks.
    let dismiss = window::dismiss_state(&state);
    let settings = state.settings.lock().unwrap().clone();
    let nudges = state.fake_nudges.lock().unwrap().clone();
    let tasks = state.tasks.lock().unwrap();
    let today = today_str();
    let mut view = BootView {
        today: vec![],
        overdue: vec![],
        nudges,
        undated: vec![],
        settings,
        dismiss,
    };
    for t in tasks.iter().filter(|t| t.status == TaskStatus::NeedsAction) {
        match &t.due_date {
            Some(d) if *d < today => view.overdue.push(t.clone()),
            Some(d) if *d == today => view.today.push(t.clone()),
            Some(_) => {} // future-dated tasks are not part of the boot view
            None => view.undated.push(t.clone()),
        }
    }
    view
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
    state.tasks.lock().unwrap().push(task.clone());
    mark_engaged(&state);
    let _ = app.emit("tasks-changed", ());
    Ok(task)
}

#[tauri::command]
pub fn complete_task(
    app: AppHandle,
    state: State<AppState>,
    local_id: String,
) -> Result<TaskDto, String> {
    let mut tasks = state.tasks.lock().unwrap();
    let task = tasks
        .iter_mut()
        .find(|t| t.local_id == local_id)
        .ok_or_else(|| format!("No task with id {local_id}"))?;
    task.status = TaskStatus::Completed;
    let dto = task.clone();
    drop(tasks);
    mark_engaged(&state);
    let _ = app.emit("tasks-changed", ());
    Ok(dto)
}

#[tauri::command]
pub fn ack_nudge(
    app: AppHandle,
    state: State<AppState>,
    nudge_id: String,
) -> Result<(), String> {
    state
        .fake_nudges
        .lock()
        .unwrap()
        .retain(|n| n.id != nudge_id);
    mark_engaged(&state);
    let _ = app.emit("tasks-changed", ());
    Ok(())
}

#[tauri::command]
pub fn nothing_today(state: State<AppState>) {
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
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Settings {
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", settings.clone());
    settings
}
