mod commands;
mod nudges;
mod store;
mod models;
mod window;

use std::sync::Mutex;

use models::{DueNudgeDto, Settings, TaskDto, TaskStatus};
use window::DismissGate;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub tasks: Mutex<Vec<TaskDto>>,
    pub fake_nudges: Mutex<Vec<DueNudgeDto>>,
    pub gate: Mutex<DismissGate>,
}

// M1 placeholder data; replaced by the SQLite store in M2.
fn fake_state() -> AppState {
    let today = chrono::Local::now().date_naive();
    let yesterday = (today - chrono::Days::new(1)).format("%Y-%m-%d").to_string();
    let today = today.format("%Y-%m-%d").to_string();
    AppState {
        settings: Mutex::new(Settings::default()),
        tasks: Mutex::new(vec![
            TaskDto {
                local_id: "fake-1".into(),
                title: "Reply to landlord about lease".into(),
                notes: None,
                due_date: Some(today.clone()),
                status: TaskStatus::NeedsAction,
            },
            TaskDto {
                local_id: "fake-2".into(),
                title: "Book dentist appointment".into(),
                notes: Some("Ask about weekend slots".into()),
                due_date: Some(yesterday),
                status: TaskStatus::NeedsAction,
            },
            TaskDto {
                local_id: "fake-3".into(),
                title: "Read chapter 4".into(),
                notes: None,
                due_date: None,
                status: TaskStatus::NeedsAction,
            },
        ]),
        fake_nudges: Mutex::new(vec![DueNudgeDto {
            id: "fake-nudge-1".into(),
            title: "Check in with Mom".into(),
            interval_days: 14,
            days_overdue: 2,
        }]),
        gate: Mutex::new(DismissGate::new()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Single-instance must be the first plugin registered.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            window::show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(fake_state())
        .invoke_handler(tauri::generate_handler![
            commands::get_boot_view,
            commands::add_task,
            commands::complete_task,
            commands::ack_nudge,
            commands::nothing_today,
            commands::dismiss_window,
            commands::get_dismiss_state,
            commands::get_settings,
            commands::update_settings,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            window::attach_close_guard(&handle);
            window::setup_tray(&handle)?;
            window::show_main_window(&handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
