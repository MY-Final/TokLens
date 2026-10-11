//! Connection setup and schema version access.

use super::migrations::migrate;
use super::{UsageError, UsageStore};
use rusqlite::Connection;
use std::path::Path;

impl UsageStore {
    /// 打开（必要时创建）本地用量库并执行迁移。
    pub fn open(path: &Path) -> Result<Self, UsageError> {
        Self::from_connection(Connection::open(path)?)
    }

    /// 内存库（测试用）。
    pub fn open_in_memory() -> Result<Self, UsageError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, UsageError> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    /// 已应用的迁移版本。
    pub fn schema_version(&self) -> Result<u32, UsageError> {
        let version: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;
        Ok(version as u32)
    }

    /// 仅测试使用：直接访问连接以校验存储内容（不进入生产 API）。
    #[cfg(test)]
    pub(crate) fn connection_for_tests(&self) -> &Connection {
        &self.conn
    }
}
