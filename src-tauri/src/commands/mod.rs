//! Tauri Command 层：应用状态、Codex 同步调度与前后端 DTO。
//!
//! 分层（见 `docs/architecture.md`）：前端 → command → `usage`（模型/口径/存储）→ `collectors`（只读采集）。
//! 本层只做参数校验、调用领域层、组装 DTO，不内联统计口径。
//!
//! 隐私：只返回元数据与统计值；会话正文、提示词、凭据不进入任何 DTO。

pub mod dto;
pub mod queries;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::collectors::codex::{CodexCollector, SyncOptions, SyncReport};
use crate::usage::model::now_ms;
use crate::usage::store::{UsageError, UsageStore};

/// 同步开始/结束时广播给前端的事件名。
pub const SYNC_EVENT: &str = "usage-sync";

/// 同步状态（跨命令共享，供 UI 展示"同步中/上次结果"）。
#[derive(Debug, Clone, Default)]
pub struct SyncState {
    pub running: bool,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub report: Option<SyncReport>,
}

/// 同步状态 DTO（也是事件负载）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStateDto {
    pub running: bool,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub report: Option<dto::SyncReportDto>,
}

impl From<&SyncState> for SyncStateDto {
    fn from(state: &SyncState) -> Self {
        Self {
            running: state.running,
            started_at_ms: state.started_at_ms,
            finished_at_ms: state.finished_at_ms,
            duration_ms: state.duration_ms,
            error: state.error.clone(),
            report: state.report.clone().map(dto::SyncReportDto::from),
        }
    }
}

/// 命令错误：可序列化、对用户可读。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub message: String,
}

impl CommandError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<UsageError> for CommandError {
    fn from(error: UsageError) -> Self {
        CommandError::new(dto::usage_error_message(&error))
    }
}

/// 应用状态：本地库连接 + 库路径 + 同步状态。
pub struct AppState {
    pub(crate) store: Mutex<UsageStore>,
    pub(crate) db_path: PathBuf,
    pub(crate) sync: Mutex<SyncState>,
}

impl AppState {
    /// 在 TokLens 专属应用数据目录下初始化本地库（不存在则创建），并执行迁移。
    ///
    /// 数据库路径：`<应用数据目录>/toklens-usage.sqlite3`（Windows 为
    /// `%APPDATA%\com.toklens.app\`）。客户端自己的数据目录只读，绝不写入。
    pub fn initialize(app_data_dir: &Path) -> Result<Self, UsageError> {
        std::fs::create_dir_all(app_data_dir)?;
        let db_path = app_data_dir.join("toklens-usage.sqlite3");
        let store = UsageStore::open(&db_path)?;
        Ok(Self {
            store: Mutex::new(store),
            db_path,
            sync: Mutex::new(SyncState::default()),
        })
    }

    pub fn database_path(&self) -> &Path {
        &self.db_path
    }

    /// 获取本地库连接（互斥保护）。锁中毒说明此前有线程 panic，这里继续可用并提示。
    pub(crate) fn store(&self) -> Result<MutexGuard<'_, UsageStore>, CommandError> {
        self.store
            .lock()
            .map_err(|_| CommandError::new("本地数据库状态异常，请重启应用"))
    }

    pub(crate) fn sync_state(&self) -> SyncStateDto {
        match self.sync.lock() {
            Ok(state) => SyncStateDto::from(&*state),
            Err(poisoned) => SyncStateDto::from(&*poisoned.into_inner()),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_database_in_given_directory() {
        let dir = std::env::temp_dir().join(format!("toklens-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let state = AppState::initialize(&dir).expect("initialize state");
        assert_eq!(state.database_path(), dir.join("toklens-usage.sqlite3"));
        assert!(state.database_path().exists());
        assert_eq!(
            state
                .store()
                .expect("store")
                .schema_version()
                .expect("version"),
            1
        );
        let sync = state.sync_state();
        assert!(!sync.running);
        assert_eq!(sync.report, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
