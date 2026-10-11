//! Thread metadata merge and maximum cumulative snapshot selection.

use super::parsed::ParsedFile;
use crate::usage::model::SessionRecord;
use crate::usage::normalize::{CodexThreadSnapshot, codex_session};
use crate::usage::tokens::RawTokenCounts;

/// 同一线程的多个 rollout 文件聚合结果。
#[derive(Debug, Default)]
pub(super) struct ThreadAggregate {
    session_id: Option<String>,
    parent_thread_id: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    first_ordinal: Option<i64>,
    message_count: usize,
    /// 线程累计值：在新旧两个计数器（以及同线程的多个文件）中取最大值。
    cumulative: Option<RawTokenCounts>,
    cumulative_key: Option<(u64, u64, u64)>,
    pub(super) files: Vec<ParsedFile>,
}

/// 累计值排序键：先比总量，再比输入与缓存读取（仅用于在多个候选中确定性地取最大值）。
fn cumulative_key(counts: &RawTokenCounts) -> (u64, u64, u64) {
    (
        counts.client_total.unwrap_or(0),
        counts.input.unwrap_or(0),
        counts.cache_read.unwrap_or(0),
    )
}

impl ThreadAggregate {
    pub(super) fn push(&mut self, file: ParsedFile) {
        if self.session_id.is_none() {
            self.session_id = file.session_id.clone();
        }
        if self.parent_thread_id.is_none() {
            self.parent_thread_id = file.parent_thread_id.clone();
        }
        if self.cwd.is_none() {
            self.cwd = file.cwd.clone();
        }
        if self.model.is_none() {
            self.model = file.model.clone();
        }
        if let Some(start) = file.started_at.as_deref()
            && self
                .started_at
                .as_deref()
                .is_none_or(|current| start < current)
        {
            self.started_at = Some(start.to_string());
        }
        if let Some(end) = file.ended_at.as_deref()
            && self.ended_at.as_deref().is_none_or(|current| end > current)
        {
            self.ended_at = Some(end.to_string());
        }
        // 只要有一个文件的明细是从头开始的，整个线程的请求明细就是完整的。
        if let Some(first_ordinal) = file.first_ordinal {
            self.first_ordinal = Some(
                self.first_ordinal
                    .map_or(first_ordinal, |current| current.min(first_ordinal)),
            );
        }
        self.message_count += file.messages.len();
        // 新旧两个累计计数器的起点可能不同（旧格式链可能覆盖更早的历史），
        // 只能取最大值；绝不能把两个计数器或多份文件相加。
        for candidate in [file.new_cumulative, file.legacy_cumulative]
            .into_iter()
            .flatten()
        {
            self.observe_cumulative(candidate);
        }
        self.files.push(file);
    }

    fn observe_cumulative(&mut self, candidate: RawTokenCounts) {
        let key = cumulative_key(&candidate);
        if self.cumulative.is_none() || Some(key) > self.cumulative_key {
            self.cumulative = Some(candidate);
            self.cumulative_key = Some(key);
        }
    }

    /// 由线程聚合结果生成会话快照（累计值 + 明细完整度）。
    pub(super) fn session_record(&self, thread_id: &str) -> SessionRecord {
        codex_session(&CodexThreadSnapshot {
            thread_id,
            parent_thread_id: self.parent_thread_id.as_deref(),
            cwd: self.cwd.as_deref(),
            model: self.model.as_deref(),
            started_at: self.started_at.as_deref(),
            ended_at: self.ended_at.as_deref(),
            first_ordinal: self.first_ordinal,
            message_count: self.message_count,
            cumulative: self.cumulative.unwrap_or_default(),
        })
    }
}
