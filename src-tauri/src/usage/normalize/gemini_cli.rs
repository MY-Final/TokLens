//! Gemini prompt contains cached input; candidates and thoughts are separate.
use super::request_ledger::{self, RequestMeta, SessionMeta};
use crate::usage::{
    client::Client,
    model::{MessageRecord, SessionRecord, SourceKind},
    tokens::{
        NormalizedUsage, RawTokenCounts, TokenBuckets, UNKNOWN_TOTAL_INCOMPLETE, UsageTotalBasis,
    },
};
use serde::Deserialize;

#[derive(Default, Debug, Clone, Copy, Deserialize)]
pub struct GeminiUsage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cached: Option<u64>,
    pub thoughts: Option<u64>,
    pub tool: Option<u64>,
    pub total: Option<u64>,
}

impl GeminiUsage {
    pub fn valid(&self) -> bool {
        [
            self.input,
            self.output,
            self.cached,
            self.thoughts,
            self.tool,
            self.total,
        ]
        .into_iter()
        .flatten()
        .all(|count| count <= i64::MAX as u64)
    }
}

pub fn usage(raw: &GeminiUsage) -> NormalizedUsage {
    let mut unknown = Vec::new();
    // Missing cache prevents a disjoint input split; never preserve overlapping raw input.
    let input = raw
        .input
        .zip(raw.cached)
        .and_then(|(input, cache)| input.checked_sub(cache));
    if raw.input.is_some() && raw.cached.is_none() {
        unknown.push("gemini_cli.input_split_missing_cache");
    }
    if raw
        .input
        .zip(raw.cached)
        .is_some_and(|(input, cached)| cached > input)
    {
        unknown.push("anomaly.cache_read_gt_input");
    }
    if raw.tool != Some(0) {
        unknown.push("gemini_cli.tool_relation_unverified");
    }
    let buckets = TokenBuckets {
        input,
        output: raw.output,
        reasoning: raw.thoughts,
        cache_read: raw.cached,
        cache_write: None,
    };
    // Nonzero tool usage has no verified relation or corresponding canonical bucket.
    let sum = if raw.tool == Some(0) {
        input
            .zip(raw.cached)
            .zip(raw.output)
            .zip(raw.thoughts)
            .and_then(|(((input, cached), output), thoughts)| {
                input
                    .checked_add(cached)?
                    .checked_add(output)?
                    .checked_add(thoughts)
            })
            .filter(|sum| *sum <= i64::MAX as u64)
    } else {
        None
    };
    let (total_tokens, total_basis) =
        if let Some(total) = raw.total.filter(|total| *total <= i64::MAX as u64) {
            (Some(total), UsageTotalBasis::ClientReported)
        } else if let Some(sum) = sum {
            (Some(sum), UsageTotalBasis::DerivedFromBuckets)
        } else {
            unknown.push(UNKNOWN_TOTAL_INCOMPLETE);
            (None, UsageTotalBasis::Unavailable)
        };
    unknown.sort_unstable();
    NormalizedUsage {
        buckets,
        total_tokens,
        total_basis,
        unknown_relations: unknown,
        client_total_mismatch: raw.total.zip(sum).is_some_and(|(total, sum)| total != sum),
    }
}

pub fn request(session_id: &str, meta: RequestMeta, raw: &GeminiUsage) -> MessageRecord {
    let mut message = request_ledger::request(
        Client::GeminiCli,
        session_id,
        meta,
        &RawTokenCounts::default(),
    );
    message.usage = usage(raw);
    if message.usage.total_basis == UsageTotalBasis::DerivedFromBuckets {
        message.source_kind = SourceKind::Derived;
    }
    message
}

pub fn session(meta: SessionMeta, messages: &[MessageRecord], partial: bool) -> SessionRecord {
    let mut session = request_ledger::session(Client::GeminiCli, meta, messages, &[], partial);
    let sum = |field: fn(&NormalizedUsage) -> Option<u64>| {
        if messages.is_empty() {
            return None;
        }
        messages
            .iter()
            .try_fold(0u64, |sum, message| sum.checked_add(field(&message.usage)?))
            .filter(|sum| *sum <= i64::MAX as u64)
    };
    session.usage.buckets = TokenBuckets {
        input: sum(|usage| usage.buckets.input),
        output: sum(|usage| usage.buckets.output),
        reasoning: sum(|usage| usage.buckets.reasoning),
        cache_read: sum(|usage| usage.buckets.cache_read),
        cache_write: None,
    };
    session.usage.total_tokens = sum(|usage| usage.total_tokens);
    session.usage.total_basis = if session.usage.total_tokens.is_some() {
        UsageTotalBasis::DerivedFromBuckets
    } else {
        UsageTotalBasis::Unavailable
    };
    session.usage.unknown_relations = messages
        .iter()
        .flat_map(|message| message.usage.unknown_relations.iter().copied())
        .collect();
    if session.usage.total_tokens.is_none() {
        session
            .usage
            .unknown_relations
            .push(UNKNOWN_TOTAL_INCOMPLETE);
    }
    session.usage.unknown_relations.sort_unstable();
    session.usage.unknown_relations.dedup();
    session.source_kind = SourceKind::Derived;
    session
}

#[cfg(test)]
mod tests {
    use super::*;
    fn counts() -> GeminiUsage {
        GeminiUsage {
            input: Some(100),
            output: Some(3),
            cached: Some(40),
            thoughts: Some(20),
            tool: Some(0),
            total: Some(123),
        }
    }
    #[test]
    fn cache_is_in_prompt_and_thoughts_are_separate() {
        let usage = usage(&counts());
        assert_eq!(usage.buckets.input, Some(60));
        assert_eq!(usage.buckets.reasoning, Some(20));
        assert_eq!(usage.total_tokens, Some(123));
        assert!(!usage.client_total_mismatch);
    }
    #[test]
    fn missing_cache_and_nonzero_tool_never_invent_disjoint_buckets_or_totals() {
        let missing = usage(&GeminiUsage {
            cached: None,
            total: None,
            ..counts()
        });
        assert_eq!(missing.buckets.input, None);
        assert_eq!(missing.total_tokens, None);
        for tool in [None, Some(2)] {
            let unverified = usage(&GeminiUsage {
                tool,
                total: None,
                ..counts()
            });
            assert_eq!(unverified.total_tokens, None);
            assert!(
                unverified
                    .unknown_relations
                    .contains(&"gemini_cli.tool_relation_unverified")
            );
            let reported = usage(&GeminiUsage { tool, ..counts() });
            assert_eq!(reported.total_tokens, Some(123));
            assert_eq!(reported.buckets.cache_write, None);
        }
    }
    #[test]
    fn zeros_mismatch_cache_anomaly_and_overflow_are_distinct() {
        let zero = GeminiUsage {
            input: Some(0),
            output: Some(0),
            cached: Some(0),
            thoughts: Some(0),
            tool: Some(0),
            total: Some(0),
        };
        assert_eq!(usage(&zero).total_tokens, Some(0));
        assert_eq!(usage(&GeminiUsage::default()).total_tokens, None);
        assert!(
            usage(&GeminiUsage {
                total: Some(999),
                ..counts()
            })
            .client_total_mismatch
        );
        assert_eq!(
            usage(&GeminiUsage {
                cached: Some(101),
                total: None,
                ..counts()
            })
            .buckets
            .input,
            None
        );
        assert_eq!(
            usage(&GeminiUsage {
                input: Some(i64::MAX as u64),
                cached: Some(0),
                total: None,
                ..counts()
            })
            .total_tokens,
            None
        );
        assert!(
            !GeminiUsage {
                total: Some(u64::MAX),
                ..counts()
            }
            .valid()
        );
    }
}
