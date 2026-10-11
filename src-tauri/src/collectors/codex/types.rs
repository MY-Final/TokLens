//! Collector options, errors, scan reasons and reports.

use crate::usage::store::UsageError;
use std::collections::BTreeMap;

/// 解析器版本：白名单字段或归一化规则变化时 +1，水位中的旧版本会触发重新解析。
pub const PARSER_VERSION: i64 = 1;

/// 采集器错误。
#[derive(Debug)]
pub enum SyncError {
    /// 根目录无法枚举。
    Directory(std::io::Error),
    /// 本地库写入失败。
    Store(UsageError),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Directory(error) => write!(f, "cannot read collector root: {error}"),
            SyncError::Store(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<UsageError> for SyncError {
    fn from(error: UsageError) -> Self {
        SyncError::Store(error)
    }
}

/// 同步选项。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncOptions {
    /// 忽略水位，全量重新解析所有文件（用于强制修复或用户主动重扫）。
    pub force_rescan: bool,
}

/// 单个文件的处理原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScanReason {
    New,
    Appended,
    TruncatedOrReplaced,
    ParserVersionChanged,
    Forced,
    Unchanged,
}

/// 一次同步的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// 根目录不存在（视为"无数据"而不是错误）。
    pub root_missing: bool,
    pub files_seen: usize,
    pub files_parsed: usize,
    pub files_skipped_unchanged: usize,
    /// 解析成功但没有会话标识（无法归属，未写库、未推进水位）。
    pub files_without_identity: usize,
    /// 读取失败的文件数（保留水位，下次重试）。
    pub files_failed: usize,
    /// 无法枚举的子目录数。
    pub directories_failed: usize,
    /// 解析失败的行数（已计入对应文件水位）。
    pub unparsed_lines: usize,
    pub sessions_imported: usize,
    pub sessions_skipped_stale: usize,
    pub messages_inserted: usize,
    pub messages_updated: usize,
    pub messages_skipped_stale: usize,
    pub messages_pruned: usize,
    /// 各处理原因的文件数，便于诊断增量行为。
    pub reasons: BTreeMap<ScanReason, usize>,
}
