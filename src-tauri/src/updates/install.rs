use super::{UpdateManager, UpdatePhase, broadcast, error_message};
use crate::commands::AppState;
use crate::commands::CommandError;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), CommandError> {
    let (update, bytes) = {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.lock().map_err(CommandError::new)?;
        if session.view.phase != UpdatePhase::Downloaded {
            return Err(CommandError::new("请先下载并验证更新包"));
        }
        let update = session
            .update
            .clone()
            .ok_or_else(|| CommandError::new("更新信息已失效，请重新检查"))?;
        let bytes = session
            .bytes
            .take()
            .ok_or_else(|| CommandError::new("更新包已失效，请重新下载"))?;
        session.view.phase = UpdatePhase::Installing;
        (update, bytes)
    };
    broadcast(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let desktop = app.state::<crate::desktop::DesktopState>();
        if desktop.exit_requested.swap(true, Ordering::SeqCst) {
            if let Ok(mut session) = app.state::<UpdateManager>().lock() {
                session.bytes = Some(bytes);
                session.view.phase = UpdatePhase::Downloaded;
            }
            broadcast(&app);
            return Err(CommandError::new("应用正在退出"));
        }
        app.state::<AppState>().activity.shutdown();
        crate::desktop::save_window(&app);
        desktop.allow_exit.store(true, Ordering::SeqCst);
        if let Err(error) = update.install(bytes) {
            desktop.allow_exit.store(false, Ordering::SeqCst);
            desktop.exit_requested.store(false, Ordering::SeqCst);
            app.state::<AppState>()
                .activity
                .resume_after_failed_install();
            if let Ok(mut session) = app.state::<UpdateManager>().lock() {
                session.view.phase = UpdatePhase::Failed;
                session.view.error = Some(error_message(&error));
            }
            broadcast(&app);
            return Err(CommandError::new(error_message(&error)));
        }
        #[cfg(target_os = "macos")]
        app.restart();
        #[cfg(not(target_os = "macos"))]
        Ok(())
    })
    .await
    .map_err(|_| CommandError::new("安装任务未能完成"))?
}
