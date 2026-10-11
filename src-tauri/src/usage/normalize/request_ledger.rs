//! Metadata-only normalization for clients whose sessions sum independent requests.
use super::sum_complete_counts;
use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
};
use crate::usage::tokens::{
    Client, RawTokenCounts, UNKNOWN_TOTAL_INCOMPLETE, UsageTotalBasis, normalize_usage,
    semantics_for,
};

pub struct RequestMeta {
    pub key: String,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub created_ms: Option<i64>,
    pub completed_ms: Option<i64>,
    pub sidechain: bool,
    pub source_ref: String,
}
pub struct SessionMeta {
    pub id: String,
    pub parent: Option<String>,
    pub project: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}
pub fn request(
    client: Client,
    session_id: &str,
    meta: RequestMeta,
    raw: &RawTokenCounts,
) -> MessageRecord {
    MessageRecord {
        client,
        client_session_id: session_id.to_string(),
        dedup_key: meta.key,
        seq: None,
        created_at_ms: meta.created_ms,
        completed_at_ms: meta.completed_ms,
        model: meta.model,
        agent: meta.agent,
        usage: normalize_usage(semantics_for(client), raw),
        is_sidechain: meta.sidechain,
        semantics_id: semantics_for(client).id.to_string(),
        source_kind: SourceKind::Measured,
        source_ref: Some(meta.source_ref),
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}
pub fn session(
    client: Client,
    meta: SessionMeta,
    messages: &[MessageRecord],
    counts: &[RawTokenCounts],
    partial: bool,
) -> SessionRecord {
    let mut raw = sum_complete_counts(counts);
    // Sum each request's chosen total, including older records derived from their buckets.
    // Recomputing the entire session from buckets would erase reported totals on mixed formats.
    raw.client_total = if messages.is_empty() {
        None
    } else {
        messages
            .iter()
            .try_fold(0u64, |sum, message| {
                sum.checked_add(message.usage.total_tokens?)
            })
            .filter(|sum| *sum <= i64::MAX as u64)
    };
    let mut usage = normalize_usage(semantics_for(client), &raw);
    usage.total_tokens = raw.client_total;
    usage.total_basis = if usage.total_tokens.is_some() {
        UsageTotalBasis::DerivedFromBuckets
    } else {
        if !usage.unknown_relations.contains(&UNKNOWN_TOTAL_INCOMPLETE) {
            usage.unknown_relations.push(UNKNOWN_TOTAL_INCOMPLETE);
        }
        UsageTotalBasis::Unavailable
    };
    usage.client_total_mismatch |= messages
        .iter()
        .any(|message| message.usage.client_total_mismatch);
    SessionRecord {
        client,
        client_session_id: meta.id,
        parent_client_session_id: meta.parent,
        project_path: meta.project,
        model: messages
            .iter()
            .filter(|m| m.model.is_some())
            .max_by_key(|m| m.created_at_ms)
            .and_then(|m| m.model.clone()),
        started_at_ms: meta.start_ms,
        ended_at_ms: meta.end_ms.max(
            messages
                .iter()
                .filter_map(|m| m.completed_at_ms.or(m.created_at_ms))
                .max(),
        ),
        semantics_id: semantics_for(client).id.to_string(),
        total_basis: SessionTotalBasis::SumOfMessages,
        coverage: if partial {
            MessageCoverage::Partial
        } else if messages.is_empty() {
            MessageCoverage::NoMessages
        } else {
            MessageCoverage::Complete
        },
        usage,
        source_kind: SourceKind::Measured,
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn meta(key: &str) -> RequestMeta {
        RequestMeta {
            key: key.to_string(),
            model: None,
            agent: None,
            created_ms: None,
            completed_ms: None,
            sidechain: false,
            source_ref: key.to_string(),
        }
    }
    #[test]
    fn mixed_reported_and_derived_totals_preserve_request_values() {
        let reported = RawTokenCounts {
            input: Some(10),
            output: Some(5),
            reasoning: Some(0),
            cache_read: Some(0),
            cache_write: Some(0),
            client_total: Some(20),
        };
        let derived = RawTokenCounts {
            client_total: None,
            ..reported
        };
        let messages = [
            request(Client::MimoCode, "s", meta("a"), &reported),
            request(Client::MimoCode, "s", meta("b"), &derived),
        ];
        let result = session(
            Client::MimoCode,
            SessionMeta {
                id: "s".to_string(),
                parent: None,
                project: None,
                start_ms: None,
                end_ms: None,
            },
            &messages,
            &[reported, derived],
            false,
        );
        assert_eq!(result.usage.total_tokens, Some(35));
        assert!(result.usage.client_total_mismatch);
        assert_eq!(
            result.usage.total_basis,
            UsageTotalBasis::DerivedFromBuckets
        );
        let missing = RawTokenCounts {
            output: None,
            ..derived
        };
        let messages = [
            request(Client::MimoCode, "s", meta("a"), &reported),
            request(Client::MimoCode, "s", meta("b"), &missing),
        ];
        let result = session(
            Client::MimoCode,
            SessionMeta {
                id: "s".to_string(),
                parent: None,
                project: None,
                start_ms: None,
                end_ms: None,
            },
            &messages,
            &[reported, missing],
            false,
        );
        assert_eq!(result.usage.total_tokens, None);
    }
    #[test]
    fn summed_reported_totals_overflow_stays_unknown() {
        let huge = RawTokenCounts {
            input: Some(0),
            output: Some(0),
            reasoning: Some(0),
            cache_read: Some(0),
            cache_write: Some(0),
            client_total: Some(i64::MAX as u64),
        };
        let extra = RawTokenCounts {
            client_total: Some(1),
            ..huge
        };
        let messages = [
            request(Client::MimoCode, "s", meta("a"), &huge),
            request(Client::MimoCode, "s", meta("b"), &extra),
        ];
        let result = session(
            Client::MimoCode,
            SessionMeta {
                id: "s".to_string(),
                parent: None,
                project: None,
                start_ms: None,
                end_ms: None,
            },
            &messages,
            &[huge, extra],
            false,
        );
        assert_eq!(result.usage.total_tokens, None);
        assert_eq!(result.usage.total_basis, UsageTotalBasis::Unavailable);
    }
}
