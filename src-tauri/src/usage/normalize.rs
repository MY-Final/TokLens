//! 各客户端的归一化规则。
//!
//! 这里接收"客户端形状"的输入（与 `docs/data-source-research.md` 记录的字段一一对应），
//! 输出客户端无关的 [`SessionRecord`] / [`MessageRecord`]。本模块不做文件发现、不做读取，
//! 因此可以用合成数据完整覆盖去重、累计值、缺失字段等口径分支。

use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
    parse_iso8601_utc_ms,
};
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
    /// 线程累计用量（`thread_token_usage`，取该会话最新值）。
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
    /// 去重后的用量求和（[`RawTokenCounts::sum`]）。
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

/// OpenCode 消息行（`message` 表 `data` 的字段子集）。
#[derive(Debug, Clone)]
pub struct OpenCodeMessageRow<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub model_id: Option<&'a str>,
    pub provider_id: Option<&'a str>,
    pub agent: Option<&'a str>,
    pub created_ms: Option<i64>,
    pub completed_ms: Option<i64>,
    pub cost_usd: Option<f64>,
    pub usage: RawTokenCounts,
}

/// OpenCode 请求级归一化：去重键取 `message.id`。
pub fn opencode_message(row: &OpenCodeMessageRow<'_>) -> MessageRecord {
    let semantics = semantics_for(Client::OpenCode);
    let model = match (row.model_id, row.provider_id) {
        (Some(model), Some(provider)) => Some(format!("{provider}/{model}")),
        (Some(model), None) => Some(model.to_string()),
        (None, _) => None,
    };
    let cost_source = if row.cost_usd.is_some() {
        CostSource::ClientReported
    } else {
        CostSource::None
    };
    MessageRecord {
        client: Client::OpenCode,
        client_session_id: row.session_id.to_string(),
        dedup_key: row.id.to_string(),
        seq: None,
        created_at_ms: row.created_ms,
        completed_at_ms: row.completed_ms,
        model,
        agent: row.agent.map(str::to_string),
        usage: normalize_usage(semantics, &row.usage),
        is_sidechain: false,
        semantics_id: semantics.id.to_string(),
        source_kind: SourceKind::Measured,
        source_ref: Some(format!("message_id={}", row.id)),
        cost_source,
        client_cost_usd: row.cost_usd,
    }
}

/// OpenCode 会话行（`session` 或 `session_v2`）。
#[derive(Debug, Clone)]
pub struct OpenCodeSessionRow<'a> {
    pub id: &'a str,
    pub parent_id: Option<&'a str>,
    pub directory: Option<&'a str>,
    pub model: Option<&'a str>,
    pub created_ms: Option<i64>,
    pub updated_ms: Option<i64>,
    pub cost_usd: Option<f64>,
    pub usage: RawTokenCounts,
}

/// `session` 与 `session_v2` 双表兼容：按 `time_updated` 取更新的行；时间相同或缺失时取 `session_v2`。
///
/// 依据：本机 47 行重叠且 token 列完全一致，仅 3 行独有，无法判定哪张表更权威，
/// 因此采用"更新时间优先、平局取新表"的确定性规则，并把两表视为同一会话的两个版本，
/// **绝不把两表的值相加**。
pub fn pick_opencode_session<'a>(
    legacy: Option<&'a OpenCodeSessionRow<'a>>,
    current: Option<&'a OpenCodeSessionRow<'a>>,
) -> Option<&'a OpenCodeSessionRow<'a>> {
    match (legacy, current) {
        (Some(legacy), Some(current)) => {
            let legacy_updated = legacy.updated_ms.unwrap_or(i64::MIN);
            let current_updated = current.updated_ms.unwrap_or(i64::MIN);
            if legacy_updated > current_updated {
                Some(legacy)
            } else {
                Some(current)
            }
        }
        (Some(legacy), None) => Some(legacy),
        (None, Some(current)) => Some(current),
        (None, None) => None,
    }
}

/// OpenCode 会话归一化：总量使用客户端聚合列（与消息求和一致，二者不可相加）。
pub fn opencode_session(row: &OpenCodeSessionRow<'_>) -> SessionRecord {
    let semantics = semantics_for(Client::OpenCode);
    let parent = row.parent_id.filter(|parent| !parent.is_empty());
    let cost_source = if row.cost_usd.is_some() {
        CostSource::ClientReported
    } else {
        CostSource::None
    };
    SessionRecord {
        client: Client::OpenCode,
        client_session_id: row.id.to_string(),
        parent_client_session_id: parent.map(str::to_string),
        project_path: row.directory.map(str::to_string),
        model: row.model.map(str::to_string),
        started_at_ms: row.created_ms,
        ended_at_ms: row.updated_ms,
        semantics_id: semantics.id.to_string(),
        total_basis: SessionTotalBasis::ClientReported,
        // 明细由 `message` 表提供；完整度由采集器按实际导入结果给出，这里默认完整。
        coverage: MessageCoverage::Complete,
        usage: normalize_usage(semantics, &row.usage),
        source_kind: SourceKind::Measured,
        cost_source,
        client_cost_usd: row.cost_usd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::tokens::{UNKNOWN_CACHE_WRITE_RELATION, UNKNOWN_TOTAL_INCOMPLETE};

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

    #[test]
    fn opencode_message_merges_provider_and_model_and_keeps_client_cost() {
        let row = OpenCodeMessageRow {
            id: "msg_abc",
            session_id: "ses_ee3e1b96",
            model_id: Some("deepseek-v4.1-flash"),
            provider_id: Some("opencode-go"),
            agent: Some("build"),
            created_ms: Some(1_786_371_183_576),
            completed_ms: Some(1_786_371_183_900),
            cost_usd: Some(0.0123),
            usage: RawTokenCounts {
                input: Some(288_249),
                output: Some(13_418),
                cache_read: Some(756_224),
                cache_write: Some(0),
                reasoning: Some(1_006),
                client_total: Some(288_249 + 13_418 + 756_224 + 1_006),
            },
        };
        let message = opencode_message(&row);
        assert_eq!(message.dedup_key, "msg_abc");
        assert_eq!(
            message.model.as_deref(),
            Some("opencode-go/deepseek-v4.1-flash")
        );
        assert_eq!(message.agent.as_deref(), Some("build"));
        assert_eq!(message.cost_source, CostSource::ClientReported);
        assert_eq!(message.client_cost_usd, Some(0.0123));
        // reasoning 独立于 output，不能被扣减。
        assert_eq!(message.usage.buckets.output, Some(13_418));
        assert_eq!(message.usage.buckets.reasoning, Some(1_006));
        assert!(!message.usage.client_total_mismatch);
    }

    #[test]
    fn opencode_message_without_cost_stores_no_cost() {
        let row = OpenCodeMessageRow {
            id: "msg_def",
            session_id: "ses_1",
            model_id: None,
            provider_id: None,
            agent: None,
            created_ms: None,
            completed_ms: None,
            cost_usd: None,
            usage: RawTokenCounts::default(),
        };
        let message = opencode_message(&row);
        assert_eq!(message.cost_source, CostSource::None);
        assert!(message.client_cost_usd.is_none());
        assert_eq!(message.model, None);
    }

    #[test]
    fn opencode_session_picks_newest_row_and_never_adds_the_two_tables() {
        let legacy = OpenCodeSessionRow {
            id: "ses_1",
            parent_id: None,
            directory: Some("D:\\Repos\\a"),
            model: Some("{}"),
            created_ms: Some(1_786_371_183_576),
            updated_ms: Some(1_791_473_749_139),
            cost_usd: None,
            usage: RawTokenCounts {
                input: Some(288_249),
                output: Some(13_418),
                cache_read: Some(756_224),
                cache_write: Some(0),
                reasoning: Some(1_006),
                client_total: Some(1_058_897),
            },
        };
        let current = OpenCodeSessionRow {
            updated_ms: Some(1_791_351_562_546),
            ..legacy.clone()
        };
        assert!(std::ptr::eq(
            pick_opencode_session(Some(&legacy), Some(&current)).unwrap(),
            &legacy
        ));
        assert!(std::ptr::eq(
            pick_opencode_session(Some(&current), Some(&legacy)).unwrap(),
            &legacy
        ));
        // 时间相同时取新表（session_v2）。
        let tie = OpenCodeSessionRow {
            updated_ms: legacy.updated_ms,
            ..legacy.clone()
        };
        assert!(std::ptr::eq(
            pick_opencode_session(Some(&legacy), Some(&tie)).unwrap(),
            &tie
        ));
        // 只有一张表时用存在的那个。
        assert!(std::ptr::eq(
            pick_opencode_session(None, Some(&current)).unwrap(),
            &current
        ));
        assert!(std::ptr::eq(
            pick_opencode_session(Some(&legacy), None).unwrap(),
            &legacy
        ));
        assert!(pick_opencode_session(None, None).is_none());

        let session = opencode_session(&legacy);
        assert_eq!(session.total_basis, SessionTotalBasis::ClientReported);
        assert_eq!(session.usage.total_tokens, Some(1_058_897));
        assert_eq!(session.ended_at_ms, Some(1_791_473_749_139));
    }

    #[test]
    fn opencode_child_session_keeps_parent_for_dedup_review() {
        let row = OpenCodeSessionRow {
            id: "ses_child",
            parent_id: Some("ses_parent"),
            directory: None,
            model: None,
            created_ms: Some(1),
            updated_ms: Some(2),
            cost_usd: Some(0.5),
            usage: RawTokenCounts::default(),
        };
        let session = opencode_session(&row);
        assert_eq!(
            session.parent_client_session_id.as_deref(),
            Some("ses_parent")
        );
        assert_eq!(session.cost_source, CostSource::ClientReported);
        assert_eq!(session.client_cost_usd, Some(0.5));
    }
}
