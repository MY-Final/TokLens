//! Background multi-client synchronization and event emission.

use super::{AppState, CommandError, SyncStateDto, dto};
use crate::collectors::{CollectorRoots, SyncBatch, SyncOptions, sync_clients};
use crate::usage::client::Client;
use crate::usage::store::UsageStore;
use crate::usage::time::now_ms;
use tauri::{AppHandle, Emitter, Manager};

/// 同步开始/结束时广播给前端的事件名。
pub const SYNC_EVENT: &str = "usage-sync";

/// 触发一次全部客户端同步。已有同步在跑时直接返回当前状态（幂等）。
///
/// 同步在后台线程执行，并使用**独立数据库连接**：WAL 模式下 UI 查询不会被同步阻塞。
pub fn request_sync(app: &AppHandle, force: bool) -> Result<SyncStateDto, CommandError> {
    request_sync_for(app, force, &Client::ALL)
}

pub(super) fn request_codex_sync(
    app: &AppHandle,
    force: bool,
) -> Result<SyncStateDto, CommandError> {
    request_sync_for(app, force, &[Client::Codex])
}

fn request_sync_for(
    app: &AppHandle,
    force: bool,
    clients: &[Client],
) -> Result<SyncStateDto, CommandError> {
    let state = app.state::<AppState>();
    if !state.activity.start_sync().map_err(CommandError::new)? {
        return Ok(state.sync_state());
    }
    let preferences = match app.state::<crate::settings::SettingsStore>().snapshot() {
        Ok(snapshot) => snapshot.settings,
        Err(error) => {
            state.activity.finish_sync();
            return Err(CommandError::new(error));
        }
    };
    if preferences.collection_paused {
        state.activity.finish_sync();
        return Err(CommandError::new("采集已暂停，请先在设置中恢复采集"));
    }
    let enabled = preferences.clients();
    let clients: Vec<Client> = clients
        .iter()
        .copied()
        .filter(|c| enabled.contains(c))
        .collect();
    let dto = {
        let mut sync = state
            .sync
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    spawn_sync(app.clone(), force, clients);
    Ok(dto)
}

fn spawn_sync(app: AppHandle, force: bool, clients: Vec<Client>) {
    std::thread::spawn(move || {
        let outcome = run_sync(&app, force, &clients);
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
                    sync.error = report.has_errors().then(|| {
                        "部分数据源同步失败，其他数据源已继续处理；请查看各客户端状态".to_string()
                    });
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
        // Clear/exit must not overtake the final event and publish an older snapshot afterwards.
        state.activity.finish_sync();
    });
}

/// Open the TokLens database independently so UI queries remain available during scanning.
fn run_sync(app: &AppHandle, force: bool, clients: &[Client]) -> Result<SyncBatch, String> {
    let db_path = {
        let state = app.state::<AppState>();
        state.database_path().to_path_buf()
    };
    let store = UsageStore::open(&db_path).map_err(|error| dto::usage_error_message(&error))?;
    Ok(sync_clients(
        &store,
        &CollectorRoots::from_environment(),
        &SyncOptions {
            force_rescan: force,
        },
        clients,
    ))
}
