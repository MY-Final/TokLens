//! Background multi-client synchronization and event emission.

use super::{AppState, CommandError, SyncStateDto, dto};
use crate::collectors::{
    CollectorRoots, SyncBatch, SyncOptions, SyncProgress, sync_clients_with_progress,
};
use crate::usage::client::Client;
use crate::usage::store::UsageStore;
use crate::usage::time::now_ms;
use tauri::{AppHandle, Emitter, Manager};

/// 同步开始、进度变化与结束时广播给前端的事件名。
pub const SYNC_EVENT: &str = "usage-sync";

/// 在后台同步线程结束时释放进程级同步标志（`activity.syncing`）。
///
/// 与作用域守卫等价：正常结束会触发，采集器 panic 导致线程展开时同样触发，
/// 因此 `shutdown()` 与后续同步不会因一次失败而永久阻塞。用可测试的闭包承载
/// 释放动作，生产路径传入 `AppHandle` 闭包，单测直接注入断言。
struct FinishSyncGuard(Option<Box<dyn FnOnce() + Send + 'static>>);

impl FinishSyncGuard {
    fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
}

impl Drop for FinishSyncGuard {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            release();
        }
    }
}

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

pub(super) fn request_client_sync(
    app: &AppHandle,
    client: Client,
    force: bool,
) -> Result<SyncStateDto, CommandError> {
    let preferences = app
        .state::<crate::settings::SettingsStore>()
        .snapshot()
        .map_err(CommandError::new)?;
    if !preferences.settings.clients().contains(&client) {
        return Err(CommandError::new("此客户端采集已停用，请先开启后再重扫"));
    }
    request_sync_for(app, force, &[client])
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
        sync.progress = Some(SyncProgress {
            completed_clients: 0,
            total_clients: clients.len(),
            current_client: clients.first().copied(),
        });
        // 守卫在代码块结束时释放，后台线程不会阻塞状态更新。
        SyncStateDto::from(&*sync)
    };
    let _ = app.emit(SYNC_EVENT, dto.clone());
    spawn_sync(app.clone(), force, clients);
    Ok(dto)
}

fn spawn_sync(app: AppHandle, force: bool, clients: Vec<Client>) {
    std::thread::spawn(move || {
        // Drop guard releases the sync flag on every exit path, including a panic
        // unwinding out of a collector. Dropped last, so the final event is emitted first.
        let _finish = FinishSyncGuard::new({
            let app = app.clone();
            move || app.state::<AppState>().activity.finish_sync()
        });
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
                    if let Some(finished_at_ms) = sync.finished_at_ms {
                        for result in &report.clients {
                            sync.client_checks
                                .insert(result.client, (finished_at_ms, result.clone()));
                        }
                    }
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
        // Clear/exit must not overtake the final event and publish an older snapshot afterwards;
        // `_finish` releases the flag when this closure returns or unwinds.
    });
}

/// Open the TokLens database independently so UI queries remain available during scanning.
fn run_sync(app: &AppHandle, force: bool, clients: &[Client]) -> Result<SyncBatch, String> {
    let db_path = {
        let state = app.state::<AppState>();
        state.database_path().to_path_buf()
    };
    let store = UsageStore::open(&db_path).map_err(|error| dto::usage_error_message(&error))?;
    Ok(sync_clients_with_progress(
        &store,
        &CollectorRoots::from_environment(),
        &SyncOptions {
            force_rescan: force,
        },
        clients,
        |progress| {
            let state = app.state::<AppState>();
            let snapshot = {
                let mut sync = state
                    .sync
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                sync.progress = Some(progress);
                SyncStateDto::from(&*sync)
            };
            let _ = app.emit(SYNC_EVENT, snapshot);
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::FinishSyncGuard;
    use crate::commands::activity::ActivityGate;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::Arc;

    #[test]
    fn finish_guard_releases_the_sync_flag_while_unwinding() {
        let gate = Arc::new(ActivityGate::default());
        assert_eq!(gate.start_sync(), Ok(true));
        assert_eq!(
            gate.start_sync(),
            Ok(false),
            "an active scan blocks re-entry"
        );
        let worker = Arc::clone(&gate);
        let outcome = catch_unwind(AssertUnwindSafe(move || {
            let _finish = FinishSyncGuard::new(move || worker.finish_sync());
            panic!("synthetic collector failure");
        }));
        assert!(outcome.is_err(), "the synthetic panic must unwind");
        assert_eq!(
            gate.start_sync(),
            Ok(true),
            "the flag is released even after a panic"
        );
    }
}
