//! Native lifecycle. Never reads client data.
pub mod preferences;
mod tray;
pub use tray::{refresh as refresh_tray, update_last_scan, update_usage_summary};

use crate::commands::{AppState, CommandError};
use crate::settings::{CloseBehavior, SettingsPatch, SettingsStore};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt, StateFlags, WindowExt};

pub const WINDOW_FLAGS: StateFlags = StateFlags::SIZE
    .union(StateFlags::POSITION)
    .union(StateFlags::MAXIMIZED);
#[derive(Default)]
pub struct DesktopState {
    pub tray_available: AtomicBool,
    pub exit_requested: AtomicBool,
    pub allow_exit: AtomicBool,
    pub close_pending: AtomicBool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    pub tray_available: bool,
}

pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
pub fn initialize(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let state = DesktopState::default();
    state
        .tray_available
        .store(tray::create(app).is_ok(), Ordering::Relaxed);
    app.manage(state);
    let preferences = preferences::snapshot(app)
        .map_err(std::io::Error::other)?
        .settings;
    if let Some(window) = app.get_webview_window("main") {
        if preferences.remember_window {
            let _ = window.restore_state(WINDOW_FLAGS);
        }
        let autostart = std::env::args().any(|arg| arg == "--autostart");
        if autostart
            && preferences.autostart
            && preferences.autostart_hidden
            && app
                .state::<DesktopState>()
                .tray_available
                .load(Ordering::Relaxed)
        {
            let _ = window.hide();
        } else {
            show_window(app);
        }
    }
    Ok(())
}
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let app = window.app_handle();
        let state = app.state::<DesktopState>();
        let behavior = app
            .state::<SettingsStore>()
            .snapshot()
            .map(|s| s.settings.close_behavior)
            .unwrap_or_default();
        match behavior {
            CloseBehavior::Exit => request_exit(app),
            CloseBehavior::Tray if state.tray_available.load(Ordering::Relaxed) => {
                let _ = window.hide();
            }
            _ => {
                show_window(app);
                state.close_pending.store(true, Ordering::Relaxed);
                let _ = app.emit("close-requested", ());
            }
        }
    }
}
pub fn save_window(app: &AppHandle) {
    if app
        .state::<SettingsStore>()
        .snapshot()
        .is_ok_and(|s| s.settings.remember_window)
    {
        let _ = app.save_window_state(WINDOW_FLAGS);
    }
}
pub fn request_exit(app: &AppHandle) {
    if app
        .state::<DesktopState>()
        .exit_requested
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        app.state::<AppState>().activity.shutdown();
        save_window(&app);
        app.state::<DesktopState>()
            .allow_exit
            .store(true, Ordering::SeqCst);
        app.exit(0);
    });
}
#[tauri::command]
pub fn get_desktop_status(app: AppHandle) -> DesktopStatus {
    DesktopStatus {
        tray_available: app
            .state::<DesktopState>()
            .tray_available
            .load(Ordering::Relaxed),
    }
}
#[tauri::command]
pub fn get_close_pending(app: AppHandle) -> bool {
    app.state::<DesktopState>()
        .close_pending
        .load(Ordering::Relaxed)
}
#[tauri::command]
pub fn resolve_close(
    app: AppHandle,
    action: Option<CloseBehavior>,
    remember: bool,
) -> Result<(), CommandError> {
    let state = app.state::<DesktopState>();
    let Some(action) = action else {
        state.close_pending.store(false, Ordering::Relaxed);
        return Ok(());
    };
    if action == CloseBehavior::Ask {
        return Err(CommandError::new("请选择退出或缩小到托盘"));
    }
    if action == CloseBehavior::Tray && !state.tray_available.load(Ordering::Relaxed) {
        return Err(CommandError::new("托盘不可用，请选择退出或取消"));
    }
    if remember {
        app.state::<SettingsStore>()
            .update(SettingsPatch {
                close_behavior: Some(action),
                ..Default::default()
            })
            .map_err(CommandError::new)?;
        let _ = app.emit("settings-changed", ());
    }
    state.close_pending.store(false, Ordering::Relaxed);
    match action {
        CloseBehavior::Exit => request_exit(&app),
        CloseBehavior::Tray => {
            if let Some(window) = app.get_webview_window("main") {
                window
                    .hide()
                    .map_err(|_| CommandError::new("无法隐藏窗口"))?;
            }
        }
        CloseBehavior::Ask => {}
    }
    Ok(())
}
