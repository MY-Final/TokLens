//! Background Codex synchronization and event emission.

use super::{AppState, CommandError, SyncStateDto, dto};
use crate::collectors::codex::{CodexCollector, SyncOptions, SyncReport};
use crate::usage::store::UsageStore;
use crate::usage::time::now_ms;
use tauri::{AppHandle, Emitter, Manager};

/// 同步开始/结束时广播给前端的事件名。
pub const SYNC_EVENT: &str = "usage-sync";

/// 触发一次 Codex 同步。已有同步在跑时直接返回当前状态（幂等）。
///
/// 同步在后台线程执行，并使用**独立数据库连接**：WAL 模式下 UI 查询不会被同步阻塞。
pub fn request_sync(app: &AppHandle, force: bool) -> Result<SyncStateDto, CommandError> {
    let state = app.state::<AppState>();
    let dto = {
        let mut sync = state
            .sync
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if sync.running {
            return Ok(SyncStateDto::from(&*sync));
        }
        sync.running = true;
        sync.started_at_ms = Some(now_ms());
        sync.finished_at_ms = None;
        sync.duration_ms = None;
        sync.error = None;
        sync.report = None;
        // 守卫在代码块结束时释放，后台线程不会阻塞状态更新。
        SyncStateDto::from(&*sync)
    };
    let _ = app.emit(SYNC_EVENT, dto.clone());
    spawn_sync(app.clone(), force);
    Ok(dto)
}

fn spawn_sync(app: AppHandle, force: bool) {
    std::thread::spawn(move || {
        let outcome = run_sync(&app, force);
        let state = app.state::<AppState>();
        {
            let mut sync = state
                .sync
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            sync.running = false;
            sync.finished_at_ms = Some(now_ms());
            sync.duration_ms = match (sync.started_at_ms, sync.finished_at_ms) {
                (Some(started), Some(finished)) => Some(finished - started),
                _ => None,
            };
            match outcome {
                Ok(report) => {
                    sync.error = None;
                    sync.report = Some(report);
                }
                Err(message) => {
                    sync.error = Some(message);
                    sync.report = None;
                }
            }
        }
        let dto = state.sync_state();
        let _ = app.emit(SYNC_EVENT, dto);
    });
}

/// 同步实现：定位 Codex 数据目录 → 增量扫描 → 写入本地库。
fn run_sync(app: &AppHandle, force: bool) -> Result<SyncReport, String> {
    let Some(root) = CodexCollector::default_root() else {
        return Err("未找到 USERPROFILE，无法定位 Codex 数据目录".to_string());
    };
    let db_path = {
        let state = app.state::<AppState>();
        state.database_path().to_path_buf()
    };
    let store = UsageStore::open(&db_path).map_err(|error| dto::usage_error_message(&error))?;
    let collector = CodexCollector::new(root);
    collector
        .sync(
            &store,
            &SyncOptions {
                force_rescan: force,
            },
        )
        .map_err(|error| format!("同步失败：{error}"))
}
