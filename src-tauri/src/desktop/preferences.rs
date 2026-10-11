use crate::commands::CommandError;
use crate::settings::{SettingsPatch, SettingsSnapshot, SettingsStore};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

pub fn update(app: &AppHandle, patch: SettingsPatch) -> Result<SettingsSnapshot, CommandError> {
    let desired_autostart = patch.autostart;
    let resume = patch.collection_paused == Some(false);
    let previous_native = if desired_autostart.is_some() {
        Some(
            app.autolaunch()
                .is_enabled()
                .map_err(|_| CommandError::new("无法读取系统开机自启状态"))?,
        )
    } else {
        None
    };
    let store = app.state::<SettingsStore>();
    let snapshot = store.transform(|previous| {
        if patch.collection_paused.is_some()
            && app
                .state::<crate::commands::AppState>()
                .activity
                .is_clearing()
        {
            return Err("正在清除统计，请等待完成后再恢复采集".into());
        }
        let next = previous.patched(patch)?;
        if let Some(enabled) = desired_autostart {
            set_autostart(app, enabled)?;
        }
        Ok(next)
    });
    if snapshot.is_err()
        && let Some(previous) = previous_native
    {
        set_autostart(app, previous).map_err(|_| {
            CommandError::new("设置保存失败，且无法恢复系统自启状态，请检查系统设置")
        })?;
    }
    let snapshot = snapshot.map_err(CommandError::new)?;
    let _ = app.emit("settings-changed", ());
    if resume {
        let _ = crate::commands::request_sync(app, false);
    }
    Ok(snapshot)
}
fn set_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|_| "无法更改系统开机自启设置".into())
}
pub fn snapshot(app: &AppHandle) -> Result<SettingsSnapshot, CommandError> {
    let mut snapshot = app
        .state::<SettingsStore>()
        .snapshot()
        .map_err(CommandError::new)?;
    match app.autolaunch().is_enabled() {
        Ok(enabled) => snapshot.settings.autostart = enabled,
        Err(_) => snapshot.warning = Some("无法读取系统开机自启状态，请稍后重试".into()),
    }
    Ok(snapshot)
}
