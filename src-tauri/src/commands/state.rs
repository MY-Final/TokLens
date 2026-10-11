//! Application database and shared synchronization state.

use super::{CommandError, SyncStateDto};
use crate::collectors::SyncBatch;
use crate::usage::store::{UsageError, UsageStore};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// 同步状态（跨命令共享，供 UI 展示"同步中/上次结果"）。
#[derive(Debug, Clone, Default)]
pub struct SyncState {
    pub running: bool,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub report: Option<SyncBatch>,
}

/// 应用状态：本地库连接 + 库路径 + 同步状态。
pub struct AppState {
    pub(crate) activity: super::activity::ActivityGate,
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
            activity: super::activity::ActivityGate::default(),
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
