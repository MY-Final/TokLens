use super::CommandError;
use crate::settings::{SettingsPatch, SettingsSnapshot};

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<SettingsSnapshot, CommandError> {
    crate::desktop::preferences::snapshot(&app)
}

#[tauri::command]
pub fn update_settings(
    patch: SettingsPatch,
    app: tauri::AppHandle,
) -> Result<SettingsSnapshot, CommandError> {
    crate::desktop::preferences::update(&app, patch)
}
