//! codex record normalization.

use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
};
use crate::usage::time::parse_iso8601_utc_ms;
use crate::usage::tokens::{Client, RawTokenCounts, normalize_usage, semantics_for};

/// Codex 请求级用量记录（`token_usage_record`，或旧版 `event_msg.token_count` 的一条）。
#[derive(Debug, Clone)]
pub struct CodexUsageRecord<'a> {
    pub thread_id: &'a str,
    pub response_id: Option<&'a str>,
    pub ordinal: i64,
    /// ISO 8601 UTC 时间戳。
    pub timestamp: Option<&'a str>,
    pub usage: RawTokenCounts,
}

/// Codex 请求级去重键：优先 `response_id`，旧格式回落到 `token_count:<ordinal>`。
pub fn codex_dedup_key(record: &CodexUsageRecord<'_>) -> String {
    match record.response_id {
        Some(response_id) if !response_id.is_empty() => response_id.to_string(),
        _ => format!("token_count:{}", record.ordinal),
    }
}

/// Codex 线程累计快照（`thread_token_usage` / `total_token_usage` + 线程元数据）。
#[derive(Debug, Clone)]
pub struct CodexThreadSnapshot<'a> {
    pub thread_id: &'a str,
    /// `history_base.thread_id`（续接来源）；与自身 id 相同或缺失时为 `None`。
    pub parent_thread_id: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub model: Option<&'a str>,
    pub started_at: Option<&'a str>,
    pub ended_at: Option<&'a str>,
    /// 该文件首条记录的 ordinal；不为 0 说明文件是分页/续接的一部分。
    pub first_ordinal: Option<i64>,
    /// 已看到的请求级记录数。
    pub message_count: usize,
    /// 线程累计用量（`thread_token_usage`，由采集器选取该线程最大值）。
    pub cumulative: RawTokenCounts,
}

/// Codex 归一化：会话总量使用累计值，绝不把多个文件的累计值相加。
pub fn codex_session(snapshot: &CodexThreadSnapshot<'_>) -> SessionRecord {
    let semantics = semantics_for(Client::Codex);
    let coverage = match (snapshot.first_ordinal, snapshot.message_count) {
        (_, 0) => MessageCoverage::NoMessages,
        (Some(0), _) => MessageCoverage::Complete,
        // 分页/续接文件只保留部分请求明细（本机 20 个文件 ordinal 不从 0 开始）。
        _ => MessageCoverage::Partial,
    };
    let parent = snapshot
        .parent_thread_id
        .filter(|parent| !parent.is_empty() && *parent != snapshot.thread_id);
    SessionRecord {
        client: Client::Codex,
        client_session_id: snapshot.thread_id.to_string(),
        parent_client_session_id: parent.map(str::to_string),
        project_path: snapshot.cwd.map(str::to_string),
        model: snapshot.model.map(str::to_string),
        started_at_ms: snapshot.started_at.and_then(parse_iso8601_utc_ms),
        ended_at_ms: snapshot.ended_at.and_then(parse_iso8601_utc_ms),
        semantics_id: semantics.id.to_string(),
        total_basis: SessionTotalBasis::ClientCumulative,
        coverage,
        usage: normalize_usage(semantics, &snapshot.cumulative),
        source_kind: SourceKind::Measured,
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

/// Codex 请求级归一化。
pub fn codex_message(record: &CodexUsageRecord<'_>) -> MessageRecord {
    let semantics = semantics_for(Client::Codex);
    MessageRecord {
        client: Client::Codex,
        client_session_id: record.thread_id.to_string(),
        dedup_key: codex_dedup_key(record),
        seq: Some(record.ordinal),
        created_at_ms: record.timestamp.and_then(parse_iso8601_utc_ms),
        completed_at_ms: None,
        model: None,
        agent: None,
        usage: normalize_usage(semantics, &record.usage),
        is_sidechain: false,
        semantics_id: semantics.id.to_string(),
        source_kind: SourceKind::Measured,
        source_ref: Some(format!("ordinal={}", record.ordinal)),
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::tokens::UNKNOWN_CACHE_WRITE_RELATION;
    fn codex_raw(input: u64, output: u64, cache_read: u64, reasoning: u64) -> RawTokenCounts {
        RawTokenCounts {
            input: Some(input),
            output: Some(output),
            cache_read: Some(cache_read),
            cache_write: Some(0),
            reasoning: Some(reasoning),
            client_total: Some(input + output),
        }
    }

    fn snapshot(first_ordinal: Option<i64>, message_count: usize) -> CodexThreadSnapshot<'static> {
        CodexThreadSnapshot {
            thread_id: "01a129d9-6720-7020-9402-a0a5930c37e6",
            parent_thread_id: None,
            cwd: Some("D:\\WorkSpace\\TokLens"),
            model: Some("gpt-5.6-luna"),
            started_at: Some("2026-10-11T07:24:36.001Z"),
            ended_at: Some("2026-10-11T07:37:14.576Z"),
            first_ordinal,
            message_count,
            cumulative: codex_raw(4_941_884, 0, 4_091_776, 0),
        }
    }

    #[test]
    fn codex_session_uses_cumulative_and_records_basis() {
        let session = codex_session(&snapshot(Some(0), 61));
        assert_eq!(session.total_basis, SessionTotalBasis::ClientCumulative);
        assert_eq!(session.coverage, MessageCoverage::Complete);
        assert_eq!(session.usage.total_tokens, Some(4_941_884));
        assert_eq!(
            session.project_path.as_deref(),
            Some("D:\\WorkSpace\\TokLens")
        );
        assert_eq!(session.cost_source, CostSource::None);
        assert!(session.client_cost_usd.is_none());
    }

    #[test]
    fn codex_paginated_file_is_partial_coverage_not_a_new_total() {
        // 分页/续接文件：ordinal 不从 0 开始 → 明细不完整，但累计值仍是会话总量。
        let session = codex_session(&snapshot(Some(807), 5));
        assert_eq!(session.coverage, MessageCoverage::Partial);
        assert_eq!(session.usage.total_tokens, Some(4_941_884));
    }

    #[test]
    fn codex_continuation_records_parent_and_keeps_own_file_totals_apart() {
        let mut child = snapshot(Some(0), 5);
        child.thread_id = "01a129bc-9252-7473-b583-b50b88ccef10";
        child.parent_thread_id = Some("01a129d8-f6be-72b1-a243-c141d54e2791");
        child.cumulative = codex_raw(4_833_032, 0, 4_091_776, 0);
        let session = codex_session(&child);
        assert_eq!(
            session.parent_client_session_id.as_deref(),
            Some("01a129d8-f6be-72b1-a243-c141d54e2791")
        );
        // 会话总量是客户端累计值，不是"把两个文件的累计值相加"。
        assert_eq!(session.usage.total_tokens, Some(4_833_032));
    }

    #[test]
    fn codex_thread_without_parent_does_not_self_reference() {
        let mut same = snapshot(Some(0), 3);
        same.parent_thread_id = Some(same.thread_id);
        assert!(codex_session(&same).parent_client_session_id.is_none());
    }

    #[test]
    fn codex_messages_use_response_id_or_fallback_ordinal() {
        let record = CodexUsageRecord {
            thread_id: "t1",
            response_id: Some("resp_123"),
            ordinal: 42,
            timestamp: Some("2026-10-11T07:24:36.001Z"),
            usage: codex_raw(90_065, 161, 13_568, 56),
        };
        let message = codex_message(&record);
        assert_eq!(message.dedup_key, "resp_123");
        assert_eq!(message.seq, Some(42));
        assert_eq!(message.usage.buckets.input, Some(76_497));
        assert_eq!(message.source_ref.as_deref(), Some("ordinal=42"));
        assert!(
            message
                .usage
                .unknown_relations
                .contains(&UNKNOWN_CACHE_WRITE_RELATION)
        );

        let legacy = CodexUsageRecord {
            response_id: None,
            ..record
        };
        assert_eq!(codex_message(&legacy).dedup_key, "token_count:42");
    }
}
