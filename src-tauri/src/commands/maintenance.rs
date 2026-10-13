use super::{AppState, CommandError, SyncState};
use crate::settings::{AppSettings, SettingsPatch, SettingsSnapshot, SettingsStore};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn clear_statistics(app: AppHandle) -> Result<SettingsSnapshot, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.activity.begin_clear().map_err(CommandError::new)?;
        let snapshot = app
            .state::<SettingsStore>()
            .update(SettingsPatch {
                collection_paused: Some(true),
                ..Default::default()
            })
            .map_err(CommandError::new)?;
        let _ = app.emit("settings-changed", ());
        state.store()?.clear_statistics()?;
        *state
            .sync
            .lock()
            .map_err(|_| CommandError::new("同步状态异常，请重启应用"))? = SyncState::default();
        super::tray_usage::invalidate(&app);
        drop(_guard);
        let _ = app.emit(super::SYNC_EVENT, state.sync_state());
        Ok(snapshot)
    })
    .await
    .map_err(|_| CommandError::new("清除任务未能完成"))?
}

#[tauri::command]
pub fn reset_settings(app: AppHandle) -> Result<SettingsSnapshot, CommandError> {
    crate::desktop::preferences::update(&app, AppSettings::default().into())
}

#[tauri::command]
pub fn open_app_data_dir(app: AppHandle) -> Result<(), CommandError> {
    let path = app.state::<AppState>().database_path();
    let directory = path
        .parent()
        .ok_or_else(|| CommandError::new("无法定位应用数据目录"))?;
    app.opener()
        .open_path(super::storage::display_path(directory), None::<&str>)
        .map_err(|_| CommandError::new("无法打开应用数据目录"))
}
