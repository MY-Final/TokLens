use super::{UpdateManager, UpdatePhase, UpdateState, broadcast};
use crate::commands::CommandError;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
#[tauri::command]
pub async fn download_update(app: AppHandle) -> Result<UpdateState, CommandError> {
    let update = {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.lock().map_err(CommandError::new)?;
        if !matches!(
            session.view.phase,
            UpdatePhase::Available | UpdatePhase::Failed
        ) {
            return Err(CommandError::new("请先检查并选择可用更新"));
        }
        let update = session
            .update
            .clone()
            .ok_or_else(|| CommandError::new("请重新检查更新"))?;
        session.bytes = None;
        session.view.phase = UpdatePhase::Downloading;
        session.view.downloaded_bytes = 0;
        session.view.total_bytes = None;
        session.view.error = None;
        update
    };
    broadcast(&app);
    let mut downloaded = 0u64;
    let mut last = Instant::now();
    let result = update
        .download(
            |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                if let Ok(mut session) = app.state::<UpdateManager>().lock() {
                    session.view.downloaded_bytes = downloaded;
                    session.view.total_bytes = total;
                }
                if last.elapsed() >= Duration::from_millis(150) {
                    broadcast(&app);
                    last = Instant::now();
                }
            },
            || {},
        )
        .await;
    let view = {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.lock().map_err(CommandError::new)?;
        session.complete_download(result);
        session.view.clone()
    };
    broadcast(&app);
    Ok(view)
}
