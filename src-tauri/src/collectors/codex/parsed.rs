//! Metadata-only parse result shared by parser, aggregation and sync.

use crate::usage::model::MessageRecord;
use crate::usage::tokens::RawTokenCounts;
use std::path::{Path, PathBuf};

/// 单文件解析结果（只含白名单字段）。
#[derive(Debug, Clone)]
pub(super) struct ParsedFile {
    pub(super) path: PathBuf,
    pub(super) size_bytes: i64,
    pub(super) mtime_ms: i64,
    pub(super) thread_id: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) parent_thread_id: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) model: Option<String>,
    pub(super) started_at: Option<String>,
    pub(super) ended_at: Option<String>,
    pub(super) first_ordinal: Option<i64>,
    /// 新版记录里的线程累计（`thread_token_usage`），可能只覆盖新格式窗口。
    pub(super) new_cumulative: Option<RawTokenCounts>,
    /// 旧版记录里的线程累计（`info.total_token_usage`），可能覆盖更早的历史。
    pub(super) legacy_cumulative: Option<RawTokenCounts>,
    pub(super) unparsed_lines: usize,
    pub(super) messages: Vec<MessageRecord>,
}

impl ParsedFile {
    pub(super) fn new(path: &Path, size_bytes: i64, mtime_ms: i64) -> Self {
        Self {
            path: path.to_path_buf(),
            size_bytes,
            mtime_ms,
            thread_id: None,
            session_id: None,
            parent_thread_id: None,
            cwd: None,
            model: None,
            started_at: None,
            ended_at: None,
            first_ordinal: None,
            new_cumulative: None,
            legacy_cumulative: None,
            unparsed_lines: 0,
            messages: Vec::new(),
        }
    }

    pub(super) fn mark_time(&mut self, timestamp: Option<&str>) {
        let Some(timestamp) = timestamp else {
            return;
        };
        if self
            .started_at
            .as_deref()
            .is_none_or(|start| timestamp < start)
        {
            self.started_at = Some(timestamp.to_string());
        }
        if self.ended_at.as_deref().is_none_or(|end| timestamp > end) {
            self.ended_at = Some(timestamp.to_string());
        }
    }

    pub(super) fn mark_ordinal(&mut self, ordinal: i64) {
        self.first_ordinal = Some(
            self.first_ordinal
                .map_or(ordinal, |first| first.min(ordinal)),
        );
    }
}
