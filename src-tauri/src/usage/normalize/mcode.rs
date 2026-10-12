//! MCode stores independent cache buckets, but has no verified reasoning split.
use super::request_ledger::{self, RequestMeta, SessionMeta};
use crate::usage::{
    client::Client,
    model::{MessageRecord, SessionRecord, SourceKind},
    tokens::{RawTokenCounts, UNKNOWN_TOTAL_INCOMPLETE, UsageTotalBasis},
};

pub const UNKNOWN_REASONING_RELATION: &str = "mcode.reasoning_relation_unverified";

pub fn request(session_id: &str, meta: RequestMeta, raw: RawTokenCounts) -> MessageRecord {
    let mut message = request_ledger::request(Client::Mcode, session_id, meta, &raw);
    // All installed-client positive samples have reasoning=0. A future nonzero
    // or missing reasoning value cannot establish disjoint output buckets.
    if raw.reasoning != Some(0) {
        message.usage.buckets.output = None;
        message.usage.total_tokens = None;
        message.usage.total_basis = UsageTotalBasis::Unavailable;
        message
            .usage
            .unknown_relations
            .push(UNKNOWN_REASONING_RELATION);
        message
            .usage
            .unknown_relations
            .push(UNKNOWN_TOTAL_INCOMPLETE);
    }
    if message
        .usage
        .total_tokens
        .is_some_and(|total| total > i64::MAX as u64)
    {
        message.usage.total_tokens = None;
        message.usage.total_basis = UsageTotalBasis::Unavailable;
        message
            .usage
            .unknown_relations
            .push(UNKNOWN_TOTAL_INCOMPLETE);
    }
    if message.usage.total_tokens.is_some() {
        message.source_kind = SourceKind::Derived;
    }
    message.usage.unknown_relations.sort_unstable();
    message.usage.unknown_relations.dedup();
    message
}

pub fn session(
    meta: SessionMeta,
    messages: &[MessageRecord],
    counts: &[RawTokenCounts],
    partial: bool,
) -> SessionRecord {
    let mut session = request_ledger::session(Client::Mcode, meta, messages, counts, partial);
    if messages.iter().any(|message| {
        message
            .usage
            .unknown_relations
            .contains(&UNKNOWN_REASONING_RELATION)
    }) {
        session.usage.buckets.output = None;
    }
    // Invalid ledger rows make the complete session total unknown, rather than
    // presenting the remaining known requests as the entire session's usage.
    if partial {
        session.usage.total_tokens = None;
        session.usage.total_basis = UsageTotalBasis::Unavailable;
        session
            .usage
            .unknown_relations
            .push(UNKNOWN_TOTAL_INCOMPLETE);
    }
    session.usage.unknown_relations.extend(
        messages
            .iter()
            .flat_map(|message| message.usage.unknown_relations.iter().copied()),
    );
    session.usage.unknown_relations.sort_unstable();
    session.usage.unknown_relations.dedup();
    if session.usage.total_tokens.is_some() {
        session.source_kind = SourceKind::Derived;
    }
    session
}

#[cfg(test)]
mod tests {
    use super::*;
    fn meta(key: &str) -> RequestMeta {
        RequestMeta {
            key: key.into(),
            model: None,
            agent: None,
            created_ms: Some(1791849600000),
            completed_ms: None,
            sidechain: false,
            source_ref: key.into(),
        }
    }
    fn counts() -> RawTokenCounts {
        RawTokenCounts {
            input: Some(10),
            output: Some(5),
            cache_read: Some(100),
            cache_write: Some(7),
            reasoning: Some(0),
            client_total: None,
        }
    }
    #[test]
    fn independent_cache_buckets_derive_total_and_keep_model_unknown() {
        let record = request("s", meta("1"), counts());
        assert_eq!(record.model, None);
        assert_eq!(record.usage.buckets.input, Some(10));
        assert_eq!(record.usage.buckets.output, Some(5));
        assert_eq!(record.usage.buckets.cache_read, Some(100));
        assert_eq!(record.usage.buckets.cache_write, Some(7));
        assert_eq!(record.usage.total_tokens, Some(122));
        assert_eq!(
            record.usage.total_basis,
            UsageTotalBasis::DerivedFromBuckets
        );
        assert_eq!(record.source_kind, SourceKind::Derived);
    }
    #[test]
    fn missing_or_positive_reasoning_cannot_invent_output_split_or_total() {
        for reasoning in [None, Some(1), Some(50)] {
            let record = request(
                "s",
                meta("1"),
                RawTokenCounts {
                    reasoning,
                    ..counts()
                },
            );
            assert_eq!(record.usage.buckets.output, None);
            assert_eq!(record.usage.buckets.reasoning, reasoning);
            assert_eq!(record.usage.total_tokens, None);
            assert_eq!(record.usage.total_basis, UsageTotalBasis::Unavailable);
            assert!(
                record
                    .usage
                    .unknown_relations
                    .contains(&UNKNOWN_REASONING_RELATION)
            );
        }
    }
    #[test]
    fn required_bucket_missing_zero_and_overflow_are_distinct() {
        for raw in [
            RawTokenCounts {
                output: None,
                ..counts()
            },
            RawTokenCounts {
                cache_write: None,
                ..counts()
            },
            RawTokenCounts {
                input: Some(i64::MAX as u64),
                ..counts()
            },
            RawTokenCounts {
                input: Some(u64::MAX),
                ..counts()
            },
        ] {
            assert_eq!(request("s", meta("1"), raw).usage.total_tokens, None);
        }
        let zero = RawTokenCounts {
            input: Some(0),
            output: Some(0),
            reasoning: Some(0),
            cache_read: Some(0),
            cache_write: Some(0),
            client_total: None,
        };
        assert_eq!(request("s", meta("1"), zero).usage.total_tokens, Some(0));
        assert_eq!(
            request("s", meta("1"), RawTokenCounts::default())
                .usage
                .total_tokens,
            None
        );
    }
    #[test]
    fn session_propagates_unknown_relations_and_partial_totals() {
        let known = counts();
        let unknown = RawTokenCounts {
            reasoning: Some(2),
            ..known
        };
        let messages = [
            request("s", meta("1"), known),
            request("s", meta("2"), unknown),
        ];
        let session_meta = || SessionMeta {
            id: "s".into(),
            parent: None,
            project: None,
            start_ms: None,
            end_ms: None,
        };
        let result = session(session_meta(), &messages, &[known, unknown], false);
        assert_eq!(result.usage.buckets.output, None);
        assert_eq!(result.usage.total_tokens, None);
        assert!(
            result
                .usage
                .unknown_relations
                .contains(&UNKNOWN_REASONING_RELATION)
        );
        let partial = session(session_meta(), &messages[..1], &[known], true);
        assert_eq!(partial.usage.total_tokens, None);
        let empty = session(session_meta(), &[], &[], false);
        assert_eq!(empty.usage.total_tokens, None);
        assert_eq!(empty.usage.buckets.cache_read, None);
    }
}
