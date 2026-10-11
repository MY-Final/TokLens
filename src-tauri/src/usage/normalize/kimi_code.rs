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

pub fn kimi_message(
    session_id: &str,
    agent: &str,
    line: i64,
    time_ms: Option<i64>,
    model: Option<String>,
    raw: &RawTokenCounts,
) -> MessageRecord {
    MessageRecord {
        client: Client::KimiCode,
        client_session_id: session_id.to_string(),
        dedup_key: format!("agent:{agent}:line:{line}"),
        seq: Some(line),
        created_at_ms: time_ms,
        completed_at_ms: time_ms,
        model,
        agent: Some(agent.to_string()),
        usage: normalize_usage(semantics_for(Client::KimiCode), raw),
        is_sidechain: agent != "main",
        semantics_id: semantics_for(Client::KimiCode).id.to_string(),
        source_kind: SourceKind::Measured,
        source_ref: Some(format!("line={line}")),
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
    SessionRecord {
        client: Client::KimiCode,
        client_session_id: meta.id,
        parent_client_session_id: None,
        project_path: meta.project,
        model,
        started_at_ms: meta.start_ms,
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
