//! Application database and shared synchronization state.

use super::{CommandError, SyncStateDto};
use crate::collectors::{ClientSyncResult, SyncBatch, SyncProgress};
use crate::usage::client::Client;
use crate::usage::store::{UsageError, UsageStore};
use std::collections::BTreeMap;
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
    pub progress: Option<SyncProgress>,
    /// Last completed check for each client during this application run.
    pub client_checks: BTreeMap<Client, (i64, ClientSyncResult)>,
}

/// 应用状态：本地库连接 + 库路径 + 同步状态。
pub struct AppState {
    pub(crate) activity: super::activity::ActivityGate,
    pub(crate) store: Mutex<UsageStore>,
    pub(crate) system_directory: PathBuf,
    pub(crate) sync: Mutex<SyncState>,
}

impl AppState {
    /// 从系统应用目录的定位文件解析统计存储位置，再打开数据库并执行迁移。
    /// 未配置位置时使用系统应用目录；已配置但不可用时拒绝回退创建空库。
    pub fn initialize(app_data_dir: &Path) -> Result<Self, UsageError> {
        Self::initialize_with_roots(
            app_data_dir,
            &crate::collectors::CollectorRoots::from_environment(),
        )
    }

    pub(crate) fn initialize_with_roots(
        app_data_dir: &Path,
        roots: &crate::collectors::CollectorRoots,
    ) -> Result<Self, UsageError> {
        std::fs::create_dir_all(app_data_dir)?;
        let directory = super::storage::resolve_directory(app_data_dir, roots)?;
        let db_path = directory.join("toklens-usage.sqlite3");
        let store = UsageStore::open(&db_path)?;
        Ok(Self {
            activity: super::activity::ActivityGate::default(),
            store: Mutex::new(store),
            system_directory: app_data_dir.to_path_buf(),
            sync: Mutex::new(SyncState::default()),
        })
    }

    pub fn database_path(&self) -> PathBuf {
        self.store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .database_path()
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
        let state =
            AppState::initialize_with_roots(&dir, &crate::collectors::CollectorRoots::default())
                .expect("initialize state");
        assert_eq!(
            std::fs::canonicalize(state.database_path()).expect("actual database path"),
            std::fs::canonicalize(dir.join("toklens-usage.sqlite3"))
                .expect("expected database path")
        );
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
