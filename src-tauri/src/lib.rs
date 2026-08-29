mod commands;
mod models;
mod nudges;
mod store;
mod window;

use std::sync::Mutex;

use tauri::Manager;

use store::Store;
use window::DismissGate;

pub struct AppState {
    pub store: Store,
    pub gate: Mutex<DismissGate>,
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
        .invoke_handler(tauri::generate_handler![
            commands::get_boot_view,
            commands::add_task,
            commands::complete_task,
            commands::list_nudges,
            commands::add_nudge,
            commands::update_nudge,
            commands::delete_nudge,
            commands::ack_nudge,
            commands::nothing_today,
            commands::dismiss_window,
            commands::get_dismiss_state,
            commands::get_settings,
            commands::update_settings,
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = Store::open(&data_dir.join("taskdesk.db"))?;
            app.manage(AppState {
                store,
                gate: Mutex::new(DismissGate::new()),
            });

            let handle = app.handle().clone();
            window::attach_close_guard(&handle);
            window::setup_tray(&handle)?;
            window::show_main_window(&handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
