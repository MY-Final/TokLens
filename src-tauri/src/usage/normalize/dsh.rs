//! DSH DeepSeek adapter counts: input already excludes cache reads; reasoning is in output.

use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
};
use crate::usage::tokens::{
    Client, NormalizedUsage, RawTokenCounts, UsageTotalBasis, normalize_usage, semantics_for,
};

pub fn dsh_usage(raw: &RawTokenCounts) -> NormalizedUsage {
    let mut counts = *raw;
    // DSH token-meter sums the four disjoint buckets; reasoning is already in output.
    counts.client_total = counts
        .input
        .zip(counts.output)
        .zip(counts.cache_read)
        .zip(counts.cache_write)
        .and_then(|(((input, output), cache), write)| {
            input
                .checked_add(output)?
                .checked_add(cache)?
                .checked_add(write)
        })
        .filter(|total| *total <= i64::MAX as u64);
    let mut usage = normalize_usage(semantics_for(Client::Dsh), &counts);
    if usage.total_tokens.is_some() {
        usage.total_basis = UsageTotalBasis::DerivedFromBuckets;
    }
    usage
}

pub struct DshMessage<'a> {
    pub session_id: &'a str,
    pub seq: i64,
    pub turn: i64,
    pub step: i64,
    pub attempt: u64,
    pub time_ms: Option<i64>,
    pub model: Option<&'a str>,
    pub usage: RawTokenCounts,
    pub is_sidechain: bool,
}

pub fn dsh_message(raw: &DshMessage<'_>) -> MessageRecord {
    MessageRecord {
        client: Client::Dsh,
        client_session_id: raw.session_id.to_string(),
        dedup_key: if raw.attempt == 0 {
            format!("step:{}:{}", raw.turn, raw.step)
        } else {
            format!("step:{}:{}:attempt:{}", raw.turn, raw.step, raw.attempt)
        },
        seq: Some(raw.seq),
        created_at_ms: raw.time_ms,
        completed_at_ms: None,
        model: raw.model.map(str::to_string),
        agent: None,
        usage: dsh_usage(&raw.usage),
        is_sidechain: raw.is_sidechain,
        semantics_id: semantics_for(Client::Dsh).id.to_string(),
        source_kind: SourceKind::Measured,
        source_ref: Some(format!("seq={}", raw.seq)),
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

pub struct DshSession<'a> {
    pub id: &'a str,
    pub parent: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub model: Option<&'a str>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub message_count: usize,
    pub usage: RawTokenCounts,
}

pub fn dsh_session(raw: &DshSession<'_>) -> SessionRecord {
    SessionRecord {
        client: Client::Dsh,
        client_session_id: raw.id.to_string(),
        parent_client_session_id: raw.parent.map(str::to_string),
        project_path: raw.cwd.map(str::to_string),
        model: raw.model.map(str::to_string),
        started_at_ms: raw.start_ms,
        ended_at_ms: raw.end_ms,
        semantics_id: semantics_for(Client::Dsh).id.to_string(),
        total_basis: SessionTotalBasis::SumOfMessages,
        coverage: if raw.message_count == 0 {
            MessageCoverage::NoMessages
        } else {
            MessageCoverage::Complete
        },
        usage: dsh_usage(&raw.usage),
        source_kind: SourceKind::Measured,
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}
