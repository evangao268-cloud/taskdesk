mod commands;
mod google;
mod models;
mod nudges;
mod store;
mod sync;
mod window;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

use google::auth::AuthClient;
use google::ClientConfig;
use store::Store;
use sync::{SyncEngine, SyncState};
use window::DismissGate;

pub struct AppState {
    pub store: Arc<Store>,
    pub gate: Mutex<DismissGate>,
    pub auth: Mutex<Option<Arc<AuthClient>>>,
    pub sync_state: Mutex<SyncState>,
    pub sync_kick: Arc<tokio::sync::Notify>,
    pub data_dir: PathBuf,
}

impl AppState {
    /// The auth client, created lazily so dropping google_client.json into the
    /// app data folder works without a restart.
    pub fn ensure_auth(&self) -> Result<Arc<AuthClient>, google::GoogleError> {
        let mut slot = self.auth.lock().unwrap();
        if let Some(a) = slot.as_ref() {
            return Ok(a.clone());
        }
        let config =
            ClientConfig::load(&self.data_dir).ok_or(google::GoogleError::NoClientConfig)?;
        let auth = Arc::new(AuthClient::new(config));
        *slot = Some(auth.clone());
        Ok(auth)
    }
}

/// Background loop: periodic sync plus the "new calendar day" re-show.
async fn scheduler(app: tauri::AppHandle) {
    let mut last_sync: Option<Instant> = None;
    let mut last_shown_date = chrono::Local::now().date_naive();
    loop {
        let state = app.state::<AppState>();
        let kick = state.sync_kick.clone();
        let kicked = tokio::select! {
            _ = kick.notified() => {
                // Debounce: let a burst of mutations settle before syncing.
                tokio::time::sleep(Duration::from_secs(3)).await;
                true
            }
            _ = tokio::time::sleep(Duration::from_secs(60)) => false,
        };

        // Re-show the window on the first tick of a new day (hour >= 4 keeps
        // midnight owls unbothered). Catches wake-from-sleep mornings without
        // OS power event interop.
        let now = chrono::Local::now();
        if now.date_naive() != last_shown_date && now.hour() >= 4 {
            last_shown_date = now.date_naive();
            window::show_main_window(&app);
        }

        let interval = u64::from(state.store.settings().sync_interval_secs.max(60));
        let due = last_sync.map_or(true, |t| t.elapsed() >= Duration::from_secs(interval));
        if !kicked && !due {
            continue;
        }
        let auth = {
            let slot = state.auth.lock().unwrap();
            slot.clone()
        };
        let Some(auth) = auth else { continue };
        if !auth.is_connected() {
            continue;
        }

        *state.sync_state.lock().unwrap() = SyncState::Syncing;
        let _ = app.emit("sync-status-changed", ());
        let engine = SyncEngine::new(state.store.clone(), auth);
        let report = engine.sync().await;
        last_sync = Some(Instant::now());
        *state.sync_state.lock().unwrap() = report.state;
        let _ = app.emit("sync-status-changed", ());
        if report.pulled > 0 || report.pushed > 0 {
            let _ = app.emit("tasks-changed", ());
        }
    }
}

use chrono::Timelike;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Single-instance must be the first plugin registered.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            window::show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--from-autostart"]),
        ))
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
            commands::get_auth_status,
            commands::start_google_auth,
            commands::disconnect_google,
            commands::sync_now,
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = Arc::new(Store::open(&data_dir.join("taskdesk.db"))?);
            // Pre-create the auth client when a config is already present.
            let auth = ClientConfig::load(&data_dir).map(|c| Arc::new(AuthClient::new(c)));
            app.manage(AppState {
                store,
                gate: Mutex::new(DismissGate::new()),
                auth: Mutex::new(auth),
                sync_state: Mutex::new(SyncState::Idle),
                sync_kick: Arc::new(tokio::sync::Notify::new()),
                data_dir,
            });

            let handle = app.handle().clone();
            window::attach_close_guard(&handle);
            window::setup_tray(&handle)?;
            window::show_main_window(&handle);
            tauri::async_runtime::spawn(scheduler(app.handle().clone()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
