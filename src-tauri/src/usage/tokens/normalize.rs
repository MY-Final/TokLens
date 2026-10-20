//! 按声明的字段关系换算用量，并标注缺失、未知关系与总量不一致。

use super::counts::{NormalizedUsage, RawTokenCounts, TokenBuckets, UsageTotalBasis};
use super::semantics::{FieldRelation, TokenSemantics};

/// 未知关系标识：`cache_write` 与 `input` 的关系无法判定（Codex/OpenCode）。
pub const UNKNOWN_CACHE_WRITE_RELATION: &str = "cache_write.relation_to_input";
/// 未知项标识：客户端未提供 `reasoning`，无法把 output 拆成"非推理"部分（Codex）。
pub const UNKNOWN_REASONING_MISSING: &str = "reasoning.missing";
/// 未知项标识：客户端未提供 `cache_read`，无法把 input 拆成"未命中缓存"部分（Codex）。
pub const UNKNOWN_CACHE_READ_MISSING: &str = "cache_read.missing";
/// 未知项标识：字段缺失导致总量不可得。
pub const UNKNOWN_TOTAL_INCOMPLETE: &str = "total.unavailable_due_to_missing_fields";
/// 数据异常标识：`cache_read` 大于 `input`，与声明的包含关系矛盾。
pub const ANOMALY_CACHE_READ_EXCEEDS_INPUT: &str = "anomaly.cache_read_gt_input";
/// 数据异常标识：`reasoning` 大于 `output`，与声明的包含关系矛盾。
pub const ANOMALY_REASONING_EXCEEDS_OUTPUT: &str = "anomaly.reasoning_gt_output";
/// 数据异常标识：`cache_write` 大于 `input`，与声明的包含关系矛盾。
pub const ANOMALY_CACHE_WRITE_EXCEEDS_INPUT: &str = "anomaly.cache_write_gt_input";

/// 把客户端原始计数按口径换算成互不重叠的分桶与规范总量。
pub fn normalize_usage(semantics: &TokenSemantics, raw: &RawTokenCounts) -> NormalizedUsage {
    let mut unknown = Vec::new();

    // input / cache_read
    let (input_bucket, cache_read_bucket) = match (raw.input, raw.cache_read) {
        (Some(input), Some(cache_read)) => match semantics.cache_read {
            FieldRelation::IncludedIn => {
                if cache_read > input {
                    unknown.push(ANOMALY_CACHE_READ_EXCEEDS_INPUT);
                    (Some(0), Some(cache_read))
                } else {
                    (Some(input - cache_read), Some(cache_read))
                }
            }
            _ => (Some(input), Some(cache_read)),
        },
        (Some(input), None) => {
            if semantics.cache_read == FieldRelation::IncludedIn {
                // 无法区分 input 中命中缓存的部分，保留原值并标注未知。
                unknown.push(UNKNOWN_CACHE_READ_MISSING);
            }
            (Some(input), None)
        }
        (None, cache_read) => (None, cache_read),
    };

    // 缓存写入若也包含在 input 内，必须与缓存读取一起扣除，
    // 否则"未命中缓存的输入"会重复包含缓存写入，规范总量也会多算一次。
    let input_bucket = match (input_bucket, raw.cache_write) {
        (Some(input), Some(cache_write))
            if semantics.cache_write_in_input == FieldRelation::IncludedIn =>
        {
            if cache_write > input {
                unknown.push(ANOMALY_CACHE_WRITE_EXCEEDS_INPUT);
                Some(0)
            } else {
                Some(input - cache_write)
            }
        }
        (input, _) => input,
    };

    // output / reasoning
    let (output_bucket, reasoning_bucket) = match (raw.output, raw.reasoning) {
        (Some(output), Some(reasoning)) => match semantics.reasoning {
            FieldRelation::IncludedIn => {
                if reasoning > output {
                    unknown.push(ANOMALY_REASONING_EXCEEDS_OUTPUT);
                    (Some(0), Some(reasoning))
                } else {
                    (Some(output - reasoning), Some(reasoning))
                }
            }
            _ => (Some(output), Some(reasoning)),
        },
        (Some(output), None) => {
            if semantics.reasoning == FieldRelation::IncludedIn {
                // 无法把 output 拆成"非推理"部分，保留原值并标注未知。
                unknown.push(UNKNOWN_REASONING_MISSING);
            }
            (Some(output), None)
        }
        (None, reasoning) => (None, reasoning),
    };

    if semantics.cache_write_in_input == FieldRelation::Unknown && raw.cache_write.is_some() {
        unknown.push(UNKNOWN_CACHE_WRITE_RELATION);
    }

    let buckets = TokenBuckets {
        input: input_bucket,
        output: output_bucket,
        reasoning: reasoning_bucket,
        cache_read: cache_read_bucket,
        cache_write: raw.cache_write,
    };

    let bucket_sum = buckets.sum_with(semantics);
    let (total_tokens, total_basis, mismatch) = match (raw.client_total, bucket_sum) {
        (Some(client_total), Some(sum)) => (
            Some(client_total),
            UsageTotalBasis::ClientReported,
            client_total != sum,
        ),
        (Some(client_total), None) => (Some(client_total), UsageTotalBasis::ClientReported, false),
        (None, Some(sum)) => (Some(sum), UsageTotalBasis::DerivedFromBuckets, false),
        (None, None) => (None, UsageTotalBasis::Unavailable, false),
    };
    if total_basis == UsageTotalBasis::Unavailable {
        unknown.push(UNKNOWN_TOTAL_INCOMPLETE);
    }

    unknown.sort_unstable();
    unknown.dedup();

    NormalizedUsage {
        buckets,
        total_tokens,
        total_basis,
        unknown_relations: unknown,
        client_total_mismatch: mismatch,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        CLAUDE_CODE_SEMANTICS, CLINE_SEMANTICS, CODEX_SEMANTICS, OPENCODE_SEMANTICS,
    };
    use super::*;

    fn raw(
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
        reasoning: u64,
        client_total: Option<u64>,
    ) -> RawTokenCounts {
        RawTokenCounts {
            input: Some(input),
            output: Some(output),
            cache_read: Some(cache_read),
            cache_write: Some(cache_write),
            reasoning: Some(reasoning),
            client_total,
        }
    }

    #[test]
    fn codex_subtracts_subsets_and_matches_client_total() {
        // input 含 cache_read，output 含 reasoning（本机实测关系）。
        let usage = normalize_usage(
            &CODEX_SEMANTICS,
            &raw(90_065, 161, 13_568, 0, 56, Some(90_226)),
        );
        assert_eq!(usage.buckets.input, Some(76_497));
        assert_eq!(usage.buckets.cache_read, Some(13_568));
        assert_eq!(usage.buckets.output, Some(105));
        assert_eq!(usage.buckets.reasoning, Some(56));
        assert_eq!(usage.total_tokens, Some(90_226));
        assert_eq!(usage.total_basis, UsageTotalBasis::ClientReported);
        assert!(!usage.client_total_mismatch);
        // 求和等于客户端总量，且没有重复计入缓存/推理。
        assert_eq!(usage.buckets.sum_with(&CODEX_SEMANTICS), Some(90_226));
    }

    #[test]
    fn codex_cache_write_is_excluded_and_flagged_unknown() {
        let usage = normalize_usage(&CODEX_SEMANTICS, &raw(1_000, 500, 400, 250, 100, None));
        assert_eq!(usage.buckets.cache_write, Some(250));
        // 总量不含 cache_write（客户端总量也等于 input + output）。
        assert_eq!(usage.total_tokens, Some(1_500));
        assert_eq!(usage.total_basis, UsageTotalBasis::DerivedFromBuckets);
        assert!(
            usage
                .unknown_relations
                .contains(&UNKNOWN_CACHE_WRITE_RELATION)
        );
    }

    #[test]
    fn cline_subtracts_both_caches_from_input_and_counts_each_bucket_once() {
        // Cline 的 input 同时包含缓存读取与缓存写入，两者都要从输入里扣除。
        let usage = normalize_usage(&CLINE_SEMANTICS, &raw(350, 35, 190, 5, 0, None));
        assert_eq!(usage.buckets.input, Some(155));
        assert_eq!(usage.buckets.cache_read, Some(190));
        assert_eq!(usage.buckets.cache_write, Some(5));
        assert_eq!(usage.buckets.output, Some(35));
        // 总量等于客户端的 input + output，缓存写入没有被漏掉也没有被重复计入。
        assert_eq!(usage.total_tokens, Some(385));
        assert_eq!(usage.buckets.sum_with(&CLINE_SEMANTICS), Some(385));
        assert!(
            usage.unknown_relations.is_empty(),
            "{:?}",
            usage.unknown_relations
        );
    }

    #[test]
    fn cache_write_larger_than_the_remaining_input_is_an_anomaly() {
        let usage = normalize_usage(&CLINE_SEMANTICS, &raw(100, 10, 90, 20, 0, None));
        assert!(
            usage
                .unknown_relations
                .contains(&ANOMALY_CACHE_WRITE_EXCEEDS_INPUT)
        );
        assert_eq!(usage.buckets.input, Some(0));
    }

    #[test]
    fn claude_code_keeps_cache_fields_separate() {
        // cache_read > input 是本机真实出现过的情况，不得被当成子集扣除。
        let mut counts = raw(55, 60, 435_712, 1_024, 0, None);
        // Claude Code 不提供 reasoning 字段（缺失，而不是 0）。
        counts.reasoning = None;
        let usage = normalize_usage(&CLAUDE_CODE_SEMANTICS, &counts);
        assert_eq!(usage.buckets.input, Some(55));
        assert_eq!(usage.buckets.cache_read, Some(435_712));
        assert_eq!(usage.buckets.cache_write, Some(1_024));
        assert_eq!(usage.buckets.reasoning, None);
        assert_eq!(usage.total_tokens, Some(55 + 60 + 435_712 + 1_024));
        assert_eq!(usage.total_basis, UsageTotalBasis::DerivedFromBuckets);
        assert!(usage.unknown_relations.is_empty());
    }

    #[test]
    fn reasoning_not_provided_by_client_is_never_added_to_total() {
        // 即使某天客户端开始报 reasoning，只要口径声明为 NotProvided，
        // 就不会把它叠加到总量上（避免与 output 重复）。
        let usage = normalize_usage(&CLAUDE_CODE_SEMANTICS, &raw(100, 50, 0, 0, 7, None));
        assert_eq!(usage.buckets.reasoning, Some(7));
        assert_eq!(usage.total_tokens, Some(150));
    }

    #[test]
    fn claude_code_missing_reasoning_is_not_treated_as_incomplete() {
        let mut raw = raw(10, 20, 30, 40, 0, None);
        raw.reasoning = None;
        let usage = normalize_usage(&CLAUDE_CODE_SEMANTICS, &raw);
        assert_eq!(usage.total_tokens, Some(100));
        assert!(!usage.unknown_relations.contains(&UNKNOWN_TOTAL_INCOMPLETE));
    }

    #[test]
    fn opencode_reasoning_is_separate_and_total_matches_client() {
        let usage = normalize_usage(
            &OPENCODE_SEMANTICS,
            &raw(748_061, 82_422, 41_984_493, 0, 1_234, Some(42_816_210)),
        );
        assert_eq!(usage.buckets.output, Some(82_422));
        assert_eq!(usage.buckets.reasoning, Some(1_234));
        assert_eq!(usage.total_tokens, Some(42_816_210));
        assert!(!usage.client_total_mismatch);
        assert_eq!(
            usage.buckets.sum_with(&OPENCODE_SEMANTICS),
            Some(42_816_210)
        );
    }

    #[test]
    fn missing_fields_are_not_zero() {
        let raw = RawTokenCounts {
            input: Some(100),
            output: None,
            cache_read: None,
            cache_write: None,
            reasoning: None,
            client_total: None,
        };
        let usage = normalize_usage(&CLAUDE_CODE_SEMANTICS, &raw);
        assert_eq!(usage.buckets.input, Some(100));
        assert_eq!(usage.buckets.output, None);
        assert_eq!(usage.total_tokens, None);
        assert_eq!(usage.total_basis, UsageTotalBasis::Unavailable);
        assert!(usage.unknown_relations.contains(&UNKNOWN_TOTAL_INCOMPLETE));
    }

    #[test]
    fn client_total_mismatch_is_reported_not_hidden() {
        let usage = normalize_usage(&CODEX_SEMANTICS, &raw(1_000, 0, 0, 0, 0, Some(1_200)));
        assert!(usage.client_total_mismatch);
        assert_eq!(usage.total_tokens, Some(1_200));
    }

    #[test]
    fn anomalies_are_flagged_and_never_underflow() {
        // 与声明的包含关系矛盾的数据：只标注异常，不 panic、不产生回绕值。
        let usage = normalize_usage(&CODEX_SEMANTICS, &raw(10, 5, 99, 0, 99, None));
        assert_eq!(usage.buckets.input, Some(0));
        assert_eq!(usage.buckets.output, Some(0));
        assert!(
            usage
                .unknown_relations
                .contains(&ANOMALY_CACHE_READ_EXCEEDS_INPUT)
        );
        assert!(
            usage
                .unknown_relations
                .contains(&ANOMALY_REASONING_EXCEEDS_OUTPUT)
        );
    }
    #[test]
    fn summing_raw_counts_matches_summing_buckets() {
        let items = [
            raw(100, 10, 40, 0, 5, Some(110)),
            raw(200, 20, 80, 0, 7, Some(220)),
        ];
        let sum = RawTokenCounts::sum(items.iter());
        let from_sum = normalize_usage(&CODEX_SEMANTICS, &sum);
        assert_eq!(from_sum.buckets.input, Some(180));
        assert_eq!(from_sum.total_tokens, Some(330));
    }
}
