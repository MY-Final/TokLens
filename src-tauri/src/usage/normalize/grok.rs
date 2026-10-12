//! Pure Grok ledger conversion. One record is a turn aggregate, never a model call.
use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
};
use crate::usage::tokens::{
    ANOMALY_CACHE_READ_EXCEEDS_INPUT, ANOMALY_REASONING_EXCEEDS_OUTPUT, Client, NormalizedUsage,
    RawTokenCounts, TokenBuckets, UNKNOWN_CACHE_READ_MISSING, UNKNOWN_CACHE_WRITE_RELATION,
    UNKNOWN_REASONING_MISSING, UNKNOWN_TOTAL_INCOMPLETE, UsageTotalBasis, semantics_for,
};

/// Grok writes nanosecond UTC timestamps, using both Z and +00:00 suffixes.
/// Convert only the observed UTC forms to milliseconds; unsupported offsets stay unknown.
pub fn timestamp_ms(value: &str) -> Option<i64> {
    let value = value
        .strip_suffix('Z')
        .or_else(|| value.strip_suffix("+00:00"))?;
    let (seconds, fraction) = match value.split_once('.') {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (value, None),
    };
    if seconds.len() != 19 {
        return None;
    }
    let normalized = if let Some(fraction) = fraction {
        if fraction.is_empty()
            || fraction.len() > 9
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        format!("{seconds}.{}Z", &fraction[..fraction.len().min(3)])
    } else {
        format!("{seconds}Z")
    };
    crate::usage::time::parse_iso8601_utc_ms(&normalized)
}

/// Native full input/output contain cache read/reasoning. Cache creation relation is unverified.
pub fn usage(raw: &RawTokenCounts) -> NormalizedUsage {
    let mut unknown = Vec::new();
    let input = subset(
        raw.input,
        raw.cache_read,
        UNKNOWN_CACHE_READ_MISSING,
        ANOMALY_CACHE_READ_EXCEEDS_INPUT,
        &mut unknown,
    );
    let output = subset(
        raw.output,
        raw.reasoning,
        UNKNOWN_REASONING_MISSING,
        ANOMALY_REASONING_EXCEEDS_OUTPUT,
        &mut unknown,
    );
    if raw.cache_write.is_some() {
        unknown.push(UNKNOWN_CACHE_WRITE_RELATION);
    }
    // Preserve authoritative reported totals even when component buckets are missing.
    // No fallback from partially understood cache creation to a guessed complete total.
    let total_basis = if raw.client_total.is_some() {
        UsageTotalBasis::ClientReported
    } else {
        unknown.push(UNKNOWN_TOTAL_INCOMPLETE);
        UsageTotalBasis::Unavailable
    };
    let comparable = raw.cache_write == Some(0);
    let native_sum = raw
        .input
        .zip(raw.output)
        .and_then(|(i, o)| i.checked_add(o));
    let client_total_mismatch = comparable
        && raw
            .client_total
            .zip(native_sum)
            .is_some_and(|(reported, sum)| reported != sum);
    unknown.sort_unstable();
    unknown.dedup();
    NormalizedUsage {
        buckets: TokenBuckets {
            input,
            output,
            cache_read: raw.cache_read,
            cache_write: raw.cache_write,
            reasoning: raw.reasoning,
        },
        total_tokens: raw.client_total,
        total_basis,
        unknown_relations: unknown,
        client_total_mismatch,
    }
}

fn subset(
    full: Option<u64>,
    part: Option<u64>,
    missing: &'static str,
    anomaly: &'static str,
    unknown: &mut Vec<&'static str>,
) -> Option<u64> {
    match (full, part) {
        (Some(full), Some(part)) => match full.checked_sub(part) {
            Some(value) => Some(value),
            None => {
                unknown.push(anomaly);
                None
            }
        },
        (Some(_), None) => {
            unknown.push(missing);
            None
        }
        _ => None,
    }
}

pub struct GrokSession<'a> {
    pub id: &'a str,
    pub project: Option<&'a str>,
    pub model: Option<&'a str>,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    pub raw: RawTokenCounts,
    pub coverage: MessageCoverage,
    pub incomplete: bool,
}

pub fn session(value: &GrokSession<'_>) -> SessionRecord {
    let mut normalized = usage(&value.raw);
    if value.incomplete {
        normalized.unknown_relations.push("grok.usage_incomplete");
        normalized.unknown_relations.sort_unstable();
    }
    SessionRecord {
        client: Client::Grok,
        client_session_id: value.id.into(),
        parent_client_session_id: None,
        project_path: value.project.map(str::to_owned),
        model: value.model.map(str::to_owned),
        started_at_ms: value.started_at_ms,
        ended_at_ms: value.ended_at_ms,
        semantics_id: semantics_for(Client::Grok).id.into(),
        total_basis: SessionTotalBasis::ClientReported,
        coverage: value.coverage,
        usage: normalized,
        source_kind: SourceKind::Measured,
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

pub fn turn(
    session_id: &str,
    number: i64,
    ended_at_ms: Option<i64>,
    model: Option<&str>,
    raw: &RawTokenCounts,
    incomplete: bool,
) -> MessageRecord {
    let mut normalized = usage(raw);
    if incomplete {
        normalized.unknown_relations.push("grok.usage_incomplete");
        normalized.unknown_relations.sort_unstable();
    }
    MessageRecord {
        client: Client::Grok,
        client_session_id: session_id.into(),
        dedup_key: format!("turn:{number}"),
        seq: Some(number),
        // Grok accounting event occurs at native turn end; not individual call start.
        created_at_ms: ended_at_ms,
        completed_at_ms: ended_at_ms,
        model: model.map(str::to_owned),
        agent: None,
        usage: normalized,
        is_sidechain: false,
        semantics_id: semantics_for(Client::Grok).id.into(),
        source_kind: SourceKind::Measured,
        source_ref: Some(format!(
            "grok.turn.v1:turnNumber={number};timeBasis=turnEndedAt"
        )),
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_nanoseconds_and_utc_offset_preserve_millisecond_time() {
        assert_eq!(
            timestamp_ms("1970-01-01T00:00:00.123456789+00:00"),
            Some(123)
        );
        assert_eq!(timestamp_ms("1970-01-01T00:00:00.123456789Z"), Some(123));
        assert_eq!(timestamp_ms("1970-01-01T00:00:00.1+00:00"), Some(100));
        assert_eq!(timestamp_ms("1970-01-01T00:00:00+00:00"), Some(0));
        for invalid in [
            "1970-01-01T00:00:00.1234567890Z",
            "1970-01-01T00:00:00+08:00",
            "1970-01-01T00:00:00.badZ",
            "invalid",
        ] {
            assert_eq!(timestamp_ms(invalid), None);
        }
    }

    #[test]
    fn subsets_do_not_double_count_and_unverified_write_is_excluded() {
        let normalized = usage(&RawTokenCounts {
            input: Some(100),
            output: Some(20),
            cache_read: Some(60),
            reasoning: Some(5),
            cache_write: Some(10),
            client_total: Some(120),
        });
        assert_eq!(normalized.buckets.input, Some(40));
        assert_eq!(normalized.buckets.output, Some(15));
        assert_eq!(normalized.total_tokens, Some(120));
        assert!(
            normalized
                .unknown_relations
                .contains(&UNKNOWN_CACHE_WRITE_RELATION)
        );
    }

    #[test]
    fn missing_and_zero_are_distinct_and_anomalies_never_underflow() {
        let missing = usage(&RawTokenCounts {
            input: Some(100),
            output: Some(20),
            client_total: Some(120),
            ..Default::default()
        });
        assert_eq!(missing.buckets.input, None);
        assert_eq!(missing.buckets.output, None);
        assert_eq!(missing.total_tokens, Some(120));
        let zero = usage(&RawTokenCounts {
            input: Some(0),
            output: Some(0),
            cache_read: Some(0),
            reasoning: Some(0),
            cache_write: Some(0),
            client_total: Some(0),
        });
        assert_eq!(zero.buckets.input, Some(0));
        assert_eq!(zero.buckets.output, Some(0));
        assert_eq!(zero.total_tokens, Some(0));
        let invalid = usage(&RawTokenCounts {
            input: Some(1),
            output: Some(1),
            cache_read: Some(2),
            reasoning: Some(2),
            ..Default::default()
        });
        assert_eq!(invalid.buckets.input, None);
        assert_eq!(invalid.buckets.output, None);
        assert_eq!(invalid.total_tokens, None);
        assert!(
            invalid
                .unknown_relations
                .contains(&ANOMALY_CACHE_READ_EXCEEDS_INPUT)
        );
        assert!(
            invalid
                .unknown_relations
                .contains(&ANOMALY_REASONING_EXCEEDS_OUTPUT)
        );
    }

    #[test]
    fn mismatch_and_unknown_total_are_visible() {
        let raw = RawTokenCounts {
            input: Some(100),
            output: Some(20),
            cache_write: Some(0),
            client_total: Some(999),
            ..Default::default()
        };
        assert!(usage(&raw).client_total_mismatch);
        assert_eq!(
            usage(&RawTokenCounts {
                client_total: None,
                ..raw
            })
            .total_tokens,
            None
        );
    }
}
