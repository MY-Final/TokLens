//! claude_code record normalization.

use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
};
use crate::usage::time::parse_iso8601_utc_ms;
use crate::usage::tokens::{Client, RawTokenCounts, normalize_usage, semantics_for};

/// Claude Code assistant 记录（`type = "assistant"`）。
#[derive(Debug, Clone)]
pub struct ClaudeMessage<'a> {
    pub session_id: &'a str,
    /// `message.id`：同一 `(sessionId, message.id)` 会被重复写入。
    pub message_id: &'a str,
    pub timestamp: Option<&'a str>,
    pub model: Option<&'a str>,
    pub cwd: Option<&'a str>,
    /// 子代理转录（`subagents/agent-*.jsonl`）中的记录。
    pub is_sidechain: bool,
    pub usage: RawTokenCounts,
}

/// Claude Code 请求级归一化：去重键取 `message.id`。
pub fn claude_code_message(record: &ClaudeMessage<'_>) -> MessageRecord {
    let semantics = semantics_for(Client::ClaudeCode);
    MessageRecord {
        client: Client::ClaudeCode,
        client_session_id: record.session_id.to_string(),
        dedup_key: record.message_id.to_string(),
        seq: None,
        created_at_ms: record.timestamp.and_then(parse_iso8601_utc_ms),
        completed_at_ms: None,
        model: record.model.map(str::to_string),
        agent: None,
        usage: normalize_usage(semantics, &record.usage),
        is_sidechain: record.is_sidechain,
        semantics_id: semantics.id.to_string(),
        source_kind: SourceKind::Measured,
        source_ref: None,
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

/// Claude Code 会话汇总输入：由采集器对**去重后**的消息做逐字段求和。
#[derive(Debug, Clone)]
pub struct ClaudeSessionRollup<'a> {
    pub session_id: &'a str,
    pub cwd: Option<&'a str>,
    pub model: Option<&'a str>,
    pub started_at: Option<&'a str>,
    pub ended_at: Option<&'a str>,
    /// 去重后的消息数。
    pub message_count: usize,
    /// 去重后的用量求和（[`sum_complete_counts`](super::sum_complete_counts)）：
    /// 任一请求缺少某字段时，会话该字段保持未知，不被当成 0 累加。
    pub usage: RawTokenCounts,
}

/// Claude Code 会话归一化：总量 = 去重后消息求和（客户端没有会话级聚合）。
pub fn claude_code_session(rollup: &ClaudeSessionRollup<'_>) -> SessionRecord {
    let semantics = semantics_for(Client::ClaudeCode);
    let coverage = if rollup.message_count == 0 {
        MessageCoverage::NoMessages
    } else {
        MessageCoverage::Complete
    };
    SessionRecord {
        client: Client::ClaudeCode,
        client_session_id: rollup.session_id.to_string(),
        parent_client_session_id: None,
        project_path: rollup.cwd.map(str::to_string),
        model: rollup.model.map(str::to_string),
        started_at_ms: rollup.started_at.and_then(parse_iso8601_utc_ms),
        ended_at_ms: rollup.ended_at.and_then(parse_iso8601_utc_ms),
        semantics_id: semantics.id.to_string(),
        total_basis: SessionTotalBasis::SumOfMessages,
        coverage,
        usage: normalize_usage(semantics, &rollup.usage),
        source_kind: SourceKind::Measured,
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::tokens::UNKNOWN_TOTAL_INCOMPLETE;
    #[test]
    fn claude_code_message_keeps_cache_separate_and_marks_sidechain() {
        let record = ClaudeMessage {
            session_id: "cfbc9166-dbf9-46d9-bbda-e2ef59b32a3c",
            message_id: "msg_01",
            timestamp: Some("2026-08-09T03:07:04.001Z"),
            model: Some("claude-sonnet-5"),
            cwd: Some("D:\\Repos\\example"),
            is_sidechain: true,
            usage: RawTokenCounts {
                input: Some(1_608),
                output: Some(113),
                cache_read: Some(35_200),
                cache_write: Some(0),
                reasoning: None,
                client_total: None,
            },
        };
        let message = claude_code_message(&record);
        assert_eq!(message.dedup_key, "msg_01");
        assert!(message.is_sidechain);
        // cache_read 大于 input 也不会被扣除（本机真实出现过该情况）。
        assert_eq!(message.usage.buckets.input, Some(1_608));
        assert_eq!(message.usage.total_tokens, Some(1_608 + 113 + 35_200));
        assert_eq!(
            message.usage.total_basis,
            crate::usage::tokens::UsageTotalBasis::DerivedFromBuckets
        );
    }

    #[test]
    fn claude_code_session_sums_only_deduped_messages() {
        // 同一 message.id 被重复写入两次，求和前已由 store 去重；这里验证滚动汇总的形态。
        let deduped = [
            RawTokenCounts {
                input: Some(1_608),
                output: Some(113),
                cache_read: Some(35_200),
                cache_write: Some(0),
                reasoning: None,
                client_total: None,
            },
            RawTokenCounts {
                input: Some(55),
                output: Some(60),
                cache_read: Some(0),
                cache_write: Some(1_024),
                reasoning: None,
                client_total: None,
            },
        ];
        let rollup = ClaudeSessionRollup {
            session_id: "s1",
            cwd: Some("D:\\Repos\\example"),
            model: Some("claude-sonnet-5"),
            started_at: Some("2026-08-09T03:07:04.000Z"),
            ended_at: Some("2026-08-09T03:10:00.000Z"),
            message_count: deduped.len(),
            usage: RawTokenCounts::sum(deduped.iter()),
        };
        let session = claude_code_session(&rollup);
        assert_eq!(session.total_basis, SessionTotalBasis::SumOfMessages);
        assert_eq!(session.coverage, MessageCoverage::Complete);
        assert_eq!(
            session.usage.total_tokens,
            // 去重后两条消息：1 条主转录 + 1 条子代理。
            Some(1_608 + 113 + 35_200 + 55 + 60 + 1_024)
        );
        assert!(
            !session
                .usage
                .unknown_relations
                .contains(&UNKNOWN_TOTAL_INCOMPLETE)
        );
    }

    #[test]
    fn claude_code_session_without_messages_is_marked_none() {
        let rollup = ClaudeSessionRollup {
            session_id: "s2",
            cwd: None,
            model: None,
            started_at: None,
            ended_at: None,
            message_count: 0,
            usage: RawTokenCounts::default(),
        };
        let session = claude_code_session(&rollup);
        assert_eq!(session.coverage, MessageCoverage::NoMessages);
        assert_eq!(session.usage.total_tokens, None);
        assert!(
            session
                .usage
                .unknown_relations
                .contains(&UNKNOWN_TOTAL_INCOMPLETE)
        );
    }
}
