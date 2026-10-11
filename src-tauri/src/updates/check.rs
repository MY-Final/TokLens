use super::{UpdateManager, UpdatePhase, UpdateState, broadcast, error_message};
use crate::commands::CommandError;
use crate::settings::SettingsStore;
use crate::usage::time::now_ms;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
#[tauri::command]
pub fn get_update_state(app: AppHandle) -> Result<UpdateState, CommandError> {
    Ok(app
        .state::<UpdateManager>()
        .lock()
        .map_err(CommandError::new)?
        .view
        .clone())
}

#[tauri::command]
pub async fn check_updates(app: AppHandle) -> Result<UpdateState, CommandError> {
    {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.lock().map_err(CommandError::new)?;
        if session.view.phase == UpdatePhase::Disabled {
            return Ok(session.view.clone());
        }
        if session.view.busy() {
            return Err(CommandError::new("已有更新操作正在执行"));
        }
        session.view = UpdateState {
            phase: UpdatePhase::Checking,
            ..UpdateState::default()
        };
        session.update = None;
        session.bytes = None;
    }
    broadcast(&app);
    let result = async {
        app.state::<SettingsStore>()
            .record_update_check(now_ms())
            .map_err(|message| (message, false))?;
        let updater = app
            .updater_builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|error| {
                (
                    error_message(&error),
                    matches!(error, tauri_plugin_updater::Error::ReleaseNotFound),
                )
            })?;
        updater.check().await.map_err(|error| {
            (
                error_message(&error),
                matches!(error, tauri_plugin_updater::Error::ReleaseNotFound),
            )
        })
    }
    .await;
    let view = {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.lock().map_err(CommandError::new)?;
        match result {
            Ok(Some(mut update)) => {
                // Metadata cannot redirect the app to an arbitrary download host.
                if update.download_url.scheme() != "https"
                    || update.download_url.host_str() != Some("github.com")
                    || !update
                        .download_url
                        .path()
                        .starts_with("/MY-Final/TokLens/releases/download/")
                {
                    session.view.phase = UpdatePhase::Failed;
                    session.view.error = Some("更新包地址不属于 TokLens 发布源，已拒绝下载".into());
                } else {
                    update.timeout = Some(Duration::from_secs(300));
                    session.view.phase = UpdatePhase::Available;
                    session.view.version = Some(update.version.clone());
                    session.view.notes = update.body.clone();
                    session.update = Some(update);
                }
            }
            Ok(None) => session.view.phase = UpdatePhase::UpToDate,
            Err((message, unavailable)) => {
                session.view.phase = if unavailable {
                    UpdatePhase::Unavailable
                } else {
                    UpdatePhase::Failed
                };
                session.view.error = Some(message);
            }
        }
        session.view.clone()
    };
    broadcast(&app);
    Ok(view)
}
