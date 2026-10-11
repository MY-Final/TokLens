//! Storage inputs, watermarks and results.

use crate::usage::client::Client;

/// 源文件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFileKind {
    /// Text JSON family, including session metadata sidecars (stable schema v1 id).
    Jsonl,
    Sqlite,
}

impl SourceFileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceFileKind::Jsonl => "jsonl",
            SourceFileKind::Sqlite => "sqlite",
        }
    }

    pub fn parse(value: &str) -> Option<SourceFileKind> {
        match value {
            "jsonl" => Some(SourceFileKind::Jsonl),
            "sqlite" => Some(SourceFileKind::Sqlite),
            _ => None,
        }
    }
}

/// 源文件水位（增量同步与重启恢复的依据）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub client: Client,
    pub path: String,
    pub kind: SourceFileKind,
    pub size_bytes: Option<i64>,
    pub mtime_ms: Option<i64>,
    /// 已消费的字节偏移（当前 Codex 整文件重解析，偏移用于水位判定）。
    pub content_offset: i64,
    /// 解析器版本；版本变化时应重新解析该文件。
    pub parser_version: i64,
    pub last_scan_ms: Option<i64>,
    /// 未识别/无法解析的记录数（供 UI 提示，不做静默丢弃）。
    pub unparsed_records: i64,
}

/// 读回的源文件行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileRow {
    pub id: i64,
    pub client: String,
    pub path: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
    pub mtime_ms: Option<i64>,
    pub content_offset: i64,
    pub parser_version: i64,
    pub last_scan_ms: Option<i64>,
    pub unparsed_records: i64,
}

/// 写入结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted,
    Updated,
    /// 输入比库中已有数据更旧（或累计值回退），已忽略。
    SkippedStale,
}

/// 一次导入的结果摘要。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportSummary {
    pub session: Option<UpsertOutcome>,
    pub messages_inserted: usize,
    pub messages_updated: usize,
    pub messages_skipped_stale: usize,
    /// 覆盖式导入（`replace_file_import`）中删除的、该源文件此前写入的明细数。
    pub messages_pruned: usize,
}

/// 统计过滤条件。
#[derive(Debug, Clone, Copy, Default)]
pub struct TotalsFilter<'a> {
    pub client: Option<Client>,
    pub project_path: Option<&'a str>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
}

/// 统计结果（从 `sessions` 表求和）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageTotals {
    pub sessions: i64,
    pub messages: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
}
