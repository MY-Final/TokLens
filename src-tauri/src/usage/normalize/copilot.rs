//! Copilot's native input includes both caches; output includes reasoning.
use super::request_ledger::{self, RequestMeta, SessionMeta};
use crate::usage::{
    client::Client,
    model::{MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind},
    tokens::{
        NormalizedUsage, RawTokenCounts, TokenBuckets, UNKNOWN_TOTAL_INCOMPLETE, UsageTotalBasis,
    },
};

pub const AGENT_HOST_SEMANTICS_ID: &str = "copilot.agent_host.turn.v1";

/// Agent Host has no cache-write/reasoning counters. Only zero-cache samples have
/// a verified input/output total contract; do not invent a nonzero-cache split.
pub fn agent_host_usage(raw: RawTokenCounts) -> NormalizedUsage {
    let mut unknown = vec!["reasoning.missing", "cache_write.missing"];
    let total = if raw.cache_read == Some(0) {
        raw.input
            .zip(raw.output)
            .and_then(|(i, o)| i.checked_add(o))
            .filter(|n| *n <= i64::MAX as u64)
    } else {
        None
    };
    if raw.cache_read != Some(0) {
        unknown.push("copilot.agent_host.cache_relation_unverified");
    }
    if total.is_none() {
        unknown.push(UNKNOWN_TOTAL_INCOMPLETE);
    }
    NormalizedUsage {
        buckets: TokenBuckets {
            input: if raw.cache_read == Some(0) {
                raw.input
            } else {
                None
            },
            output: raw.output,
            cache_read: raw.cache_read,
            cache_write: None,
            reasoning: None,
        },
        total_tokens: total,
        total_basis: if total.is_some() {
            UsageTotalBasis::DerivedFromBuckets
        } else {
            UsageTotalBasis::Unavailable
        },
        unknown_relations: unknown,
        client_total_mismatch: false,
    }
}

pub fn agent_host_request(
    session_id: &str,
    meta: RequestMeta,
    raw: RawTokenCounts,
) -> MessageRecord {
    let mut record = request_ledger::request(
        Client::Copilot,
        session_id,
        meta,
        &RawTokenCounts::default(),
    );
    record.usage = agent_host_usage(raw);
    record.semantics_id = AGENT_HOST_SEMANTICS_ID.into();
    record.source_kind = SourceKind::Derived;
    record
}

pub fn agent_host_totals(session: &mut SessionRecord, turns: &[RawTokenCounts], complete: bool) {
    let raw = super::sum_complete_counts(turns);
    session.usage = agent_host_usage(raw);
    if !complete {
        session.usage.total_tokens = None;
        session.usage.total_basis = UsageTotalBasis::Unavailable;
        session
            .usage
            .unknown_relations
            .push("copilot.agent_host.turns_missing");
    }
    session.semantics_id = AGENT_HOST_SEMANTICS_ID.into();
    session.total_basis = SessionTotalBasis::ClientReported;
    session.coverage = MessageCoverage::Partial;
    session.source_kind = SourceKind::Derived;
}

pub fn request(
    session_id: &str,
    meta: RequestMeta,
    native: RawTokenCounts,
) -> (MessageRecord, RawTokenCounts) {
    let caches = native
        .cache_read
        .zip(native.cache_write)
        .and_then(|(read, write)| read.checked_add(write));
    let input = native
        .input
        .zip(caches)
        .and_then(|(input, caches)| input.checked_sub(caches));
    let total = native
        .input
        .zip(native.output)
        .and_then(|(input, output)| input.checked_add(output))
        .filter(|total| *total <= i64::MAX as u64);
    let raw = RawTokenCounts {
        input,
        client_total: total,
        ..native
    };
    let mut message = request_ledger::request(Client::Copilot, session_id, meta, &raw);
    message.usage.total_tokens = total;
    if total.is_some() {
        // No native total column: input + output is a deterministic conversion.
        message.usage.total_basis = UsageTotalBasis::DerivedFromBuckets;
        message.source_kind = SourceKind::Derived;
    } else {
        message.usage.total_basis = UsageTotalBasis::Unavailable;
        if !message
            .usage
            .unknown_relations
            .contains(&UNKNOWN_TOTAL_INCOMPLETE)
        {
            message
                .usage
                .unknown_relations
                .push(UNKNOWN_TOTAL_INCOMPLETE);
        }
    }
    if native
        .input
        .zip(caches)
        .is_some_and(|(input, caches)| caches > input)
    {
        message
            .usage
            .unknown_relations
            .push("anomaly.copilot_caches_gt_input");
    }
    if native.input.is_some() && caches.is_none() {
        message
            .usage
            .unknown_relations
            .push("copilot.input_split_missing_caches");
    }
    (message, raw)
}

pub fn session(
    meta: SessionMeta,
    messages: &[MessageRecord],
    counts: &[RawTokenCounts],
    partial: bool,
) -> SessionRecord {
    let mut session = request_ledger::session(Client::Copilot, meta, messages, counts, partial);
    session.usage.unknown_relations.extend(
        messages
            .iter()
            .flat_map(|message| message.usage.unknown_relations.iter().copied()),
    );
    session.usage.unknown_relations.sort_unstable();
    session.usage.unknown_relations.dedup();
    session
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_host_zero_missing_cache_and_overflow_are_distinct() {
        let zero = RawTokenCounts {
            input: Some(0),
            output: Some(0),
            cache_read: Some(0),
            ..RawTokenCounts::default()
        };
        let usage = agent_host_usage(zero);
        assert_eq!(usage.total_tokens, Some(0));
        assert_eq!(usage.buckets.reasoning, None);
        assert_eq!(usage.buckets.cache_write, None);
        for raw in [
            RawTokenCounts {
                input: None,
                ..zero
            },
            RawTokenCounts {
                cache_read: None,
                ..zero
            },
            RawTokenCounts {
                input: Some(100),
                cache_read: Some(20),
                ..zero
            },
            RawTokenCounts {
                input: Some(i64::MAX as u64),
                output: Some(1),
                ..zero
            },
        ] {
            assert_eq!(agent_host_usage(raw).total_tokens, None);
        }
    }
    fn normalize(raw: RawTokenCounts) -> MessageRecord {
        request(
            "s",
            RequestMeta {
                key: "usage:1".into(),
                model: None,
                agent: None,
                created_ms: None,
                completed_ms: None,
                sidechain: false,
                source_ref: "usage:1".into(),
            },
            raw,
        )
        .0
    }
    #[test]
    fn cache_split_missing_and_anomalous_counters_preserve_known_totals() {
        let native = RawTokenCounts {
            input: Some(100),
            output: Some(20),
            cache_read: Some(40),
            cache_write: Some(10),
            reasoning: Some(5),
            client_total: None,
        };
        let usage = normalize(native).usage;
        assert_eq!(usage.buckets.input, Some(50));
        assert_eq!(usage.buckets.output, Some(15));
        assert_eq!(usage.total_tokens, Some(120));
        assert_eq!(usage.total_basis, UsageTotalBasis::DerivedFromBuckets);
        let missing = normalize(RawTokenCounts {
            cache_write: None,
            reasoning: None,
            ..native
        })
        .usage;
        assert_eq!(missing.buckets.input, None);
        assert_eq!(missing.total_tokens, Some(120));
        assert!(
            missing
                .unknown_relations
                .contains(&"copilot.input_split_missing_caches")
        );
        let anomaly = normalize(RawTokenCounts {
            cache_read: Some(150),
            ..native
        })
        .usage;
        assert_eq!(anomaly.buckets.input, None);
        assert_eq!(anomaly.total_tokens, Some(120));
        assert!(
            anomaly
                .unknown_relations
                .contains(&"anomaly.copilot_caches_gt_input")
        );
    }
    #[test]
    fn missing_required_fields_and_overflow_never_create_a_total() {
        for native in [
            RawTokenCounts::default(),
            RawTokenCounts {
                input: Some(i64::MAX as u64),
                output: Some(1),
                cache_read: Some(0),
                cache_write: Some(0),
                reasoning: Some(0),
                ..RawTokenCounts::default()
            },
            RawTokenCounts {
                input: Some(u64::MAX),
                output: Some(1),
                ..RawTokenCounts::default()
            },
        ] {
            assert_eq!(normalize(native).usage.total_tokens, None);
        }
    }
}
