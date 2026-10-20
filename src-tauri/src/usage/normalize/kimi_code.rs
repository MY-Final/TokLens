//! Kimi's provider adapters expose four disjoint usage buckets; reasoning is not separate.

use super::sum_complete_counts;
use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
};
use crate::usage::tokens::{Client, RawTokenCounts, normalize_usage, semantics_for};

pub struct KimiSessionMeta {
    pub id: String,
    pub project: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

/// 一条用量记录的定位信息。
pub struct KimiMessageMeta {
    /// 去重键：旧版按 agent 与行号，当前版本按消息 id。
    pub key: String,
    /// 旧版布局里承载该记录的 agent 目录名。
    pub agent: Option<String>,
    pub seq: Option<i64>,
    pub source_ref: String,
}

pub fn kimi_message(
    session_id: &str,
    meta: KimiMessageMeta,
    time_ms: Option<i64>,
    model: Option<String>,
    raw: &RawTokenCounts,
) -> MessageRecord {
    MessageRecord {
        client: Client::KimiCode,
        client_session_id: session_id.to_string(),
        dedup_key: meta.key,
        seq: meta.seq,
        created_at_ms: time_ms,
        completed_at_ms: time_ms,
        model,
        agent: meta.agent.clone(),
        usage: normalize_usage(semantics_for(Client::KimiCode), raw),
        // 旧版布局里除 main 之外的 agent 都是子代理；当前版本没有 agent 层。
        is_sidechain: meta.agent.as_deref().is_some_and(|agent| agent != "main"),
        semantics_id: semantics_for(Client::KimiCode).id.to_string(),
        source_kind: SourceKind::Measured,
        source_ref: Some(meta.source_ref),
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

pub fn kimi_session(
    meta: KimiSessionMeta,
    messages: &[MessageRecord],
    counts: &[RawTokenCounts],
    partial: bool,
) -> SessionRecord {
    let model = messages
        .iter()
        .filter(|message| message.model.is_some())
        .max_by_key(|message| message.created_at_ms)
        .and_then(|message| message.model.clone());
    // 没有会话元数据时，用最早的一条用量记录补上开始时间。
    let first_ms = messages
        .iter()
        .filter_map(|message| message.created_at_ms)
        .min();
    SessionRecord {
        client: Client::KimiCode,
        client_session_id: meta.id,
        parent_client_session_id: None,
        project_path: meta.project,
        model,
        started_at_ms: meta.start_ms.or(first_ms),
        ended_at_ms: meta.end_ms.max(
            messages
                .iter()
                .filter_map(|message| message.created_at_ms)
                .max(),
        ),
        semantics_id: semantics_for(Client::KimiCode).id.to_string(),
        total_basis: SessionTotalBasis::SumOfMessages,
        coverage: if partial {
            MessageCoverage::Partial
        } else if messages.is_empty() {
            MessageCoverage::NoMessages
        } else {
            MessageCoverage::Complete
        },
        usage: normalize_usage(
            semantics_for(Client::KimiCode),
            &sum_complete_counts(counts),
        ),
        source_kind: SourceKind::Measured,
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}
