use std::time::Instant;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, UserAttentionType, WindowEvent};

use crate::models::{DismissMode, DismissStateDto};
use crate::AppState;

/// Per-show dismissal gate. Reset every time the window is (re)shown so the
/// pause/engage requirements apply to each appearance, not once per process.
pub struct DismissGate {
    pub shown_at: Instant,
    pub engaged: bool,
}

impl DismissGate {
    pub fn new() -> Self {
        Self {
            shown_at: Instant::now(),
            engaged: false,
        }
    }
}

pub fn dismiss_state(state: &AppState) -> DismissStateDto {
    let settings = state.store.settings();
    let gate = state.gate.lock().unwrap();
    let blocked_for_ms = match settings.dismiss_mode {
        DismissMode::Instant => 0,
        DismissMode::Engage => 0,
        DismissMode::Pause => {
            let pause_ms = u64::from(settings.pause_seconds) * 1000;
            pause_ms.saturating_sub(gate.shown_at.elapsed().as_millis() as u64)
        }
    };
    let allowed = match settings.dismiss_mode {
        DismissMode::Instant => true,
        DismissMode::Pause => blocked_for_ms == 0,
        DismissMode::Engage => gate.engaged,
    };
    DismissStateDto {
        mode: settings.dismiss_mode,
        blocked_for_ms,
        engaged: gate.engaged,
        allowed,
    }
}

pub fn show_main_window(app: &AppHandle) {
    let state = app.state::<AppState>();
    *state.gate.lock().unwrap() = DismissGate::new();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.center();
        // Re-assert on every show: always-on-top ties are z-ordered by creation.
        let _ = window.set_always_on_top(true);
        let _ = window.show();
        let _ = window.set_focus();
        // Windows suppresses focus-stealing at login; the taskbar flash is the fallback.
        let _ = window.request_user_attention(Some(UserAttentionType::Informational));
        let _ = window.emit("window-shown", ());
    }
}

/// Hide if the dismiss policy allows it. Returns the state either way so the
/// frontend can show why a dismissal was refused.
pub fn try_dismiss(app: &AppHandle) -> DismissStateDto {
    let state = app.state::<AppState>();
    let dto = dismiss_state(&state);
    if dto.allowed {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }
    dto
}

pub fn attach_close_guard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let handle = app.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Never let the window actually close (that would exit the app
                // and bypass the pause/engage modes via Alt+F4). Route through
                // the policy; hide-to-tray on success.
                api.prevent_close();
                let dto = try_dismiss(&handle);
                if !dto.allowed {
                    // Let the frontend show the refusal (shake + countdown),
                    // otherwise an Alt+F4 during pause/engage fails silently.
                    let _ = handle.emit("dismiss-denied", dto);
                }
            }
        });
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show tasks", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &sync, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("TaskDesk")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "sync" => {
                // Kick the scheduler; it syncs when a connected auth client exists.
                app.state::<AppState>().sync_kick.notify_one();
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
