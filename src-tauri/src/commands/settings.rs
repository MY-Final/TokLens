use super::CommandError;
use crate::settings::{SettingsPatch, SettingsSnapshot, SettingsStore};

#[tauri::command]
pub fn get_settings(
    state: tauri::State<'_, SettingsStore>,
) -> Result<SettingsSnapshot, CommandError> {
    state.snapshot().map_err(CommandError::new)
}

#[tauri::command]
pub fn update_settings(
    patch: SettingsPatch,
    state: tauri::State<'_, SettingsStore>,
) -> Result<SettingsSnapshot, CommandError> {
    state.update(patch).map_err(CommandError::new)
}
