//! 本地 SQLite 持久化：版本化迁移、幂等导入、增量同步水位。
//!
//! 设计要点：
//!
//! - `sessions` 是统计总量的**唯一事实来源**；`messages` 只用于时间序列与明细，
//!   两者绝不相加（Codex 累计值、OpenCode 会话聚合同理）。
//! - 所有写入都是"绝对值快照"而不是"增量累加"，因此重复导入天然幂等。
//! - `source_files` 保存文件大小 / mtime / 已消费字节偏移，支持增量判定与应用重启后的恢复。
//! - 费用只保存客户端自报值，不做价格换算（费用与 Token 采集解耦）。

mod connection;
mod error;
mod import;
mod maintenance;
mod migrations;
mod queries;
mod snapshot;
mod types;
mod writes;

pub use error::UsageError;
pub use snapshot::SessionImport;
pub use types::{
    ImportSummary, SourceFile, SourceFileKind, SourceFileRow, TotalsFilter, UpsertOutcome,
    UsageTotals,
};

/// TokLens 本地用量库。
pub struct UsageStore {
    pub(crate) conn: rusqlite::Connection,
}

#[cfg(test)]
mod tests;
