//! Token 字段语义与去重叠规则。
//!
//! 三个客户端的 token 字段语义并不相同，直接相加会重复计数。这里的规则全部来自
//! `docs/data-source-research.md` 的本机实测结论（每个客户端给出判定依据），
//! 无法判定的关系一律标记为 [`FieldRelation::Unknown`]，不计入总量并在记录上标注，
//! 不做任何推测。

use std::fmt;

/// TokLens 支持的客户端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Client {
    Codex,
    ClaudeCode,
    OpenCode,
}

impl Client {
    pub const ALL: [Client; 3] = [Client::Codex, Client::ClaudeCode, Client::OpenCode];

    pub fn as_str(self) -> &'static str {
        match self {
            Client::Codex => "codex",
            Client::ClaudeCode => "claude_code",
            Client::OpenCode => "opencode",
        }
    }

    pub fn parse(value: &str) -> Option<Client> {
        match value {
            "codex" => Some(Client::Codex),
            "claude_code" => Some(Client::ClaudeCode),
            "opencode" => Some(Client::OpenCode),
            _ => None,
        }
    }
}

impl fmt::Display for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 一个字段相对另一个字段的关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRelation {
    /// 是另一个字段的子集：求和前必须扣除（例如 Codex 的 cache_read 属于 input）。
    IncludedIn,
    /// 与其它字段互不重叠：本机数据已证伪"包含"假设（例如存在 cache_read > input 的记录）。
    Separate,
    /// 客户端不提供该字段：缺失是预期行为，不算数据不完整。
    NotProvided,
    /// 无法判定：不计入总量，并在记录的 `unknown_relations` 中标注。
    Unknown,
}

/// 客户端口径标识与字段关系。
#[derive(Debug, Clone, Copy)]
pub struct TokenSemantics {
    /// 口径标识，随记录写入数据库，便于日后按口径重算。
    pub id: &'static str,
    /// `cache_read` 相对 `input` 的关系。
    pub cache_read: FieldRelation,
    /// `reasoning` 相对 `output` 的关系。
    pub reasoning: FieldRelation,
    /// `cache_write` 相对 `input` 的关系。
    pub cache_write_in_input: FieldRelation,
    /// `cache_write` 是否计入 TokLens 规范总量。
    pub cache_write_counts_in_total: bool,
    /// 客户端是否直接提供总量字段。
    pub client_reports_total: bool,
    /// 判定依据（本机实测数字，写入文档与代码注释）。
    pub basis: &'static str,
}

/// Codex（`token_usage_record` 与旧版 `event_msg.token_count` 语义一致）。
///
/// 依据：本机 19 385 条 `token_usage_record` 中 `total == input + output` 恒成立，
/// 且 `cached_input <= input`、`reasoning_output <= output` 恒成立，说明
/// cache_read 与 reasoning 分别是 input / output 的子集；`cache_write` 非 0 时
/// 总量仍等于 input + output，故不计入总量，其与 input 的关系无法判定。
pub const CODEX_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "codex.usage.v1",
    cache_read: FieldRelation::IncludedIn,
    reasoning: FieldRelation::IncludedIn,
    cache_write_in_input: FieldRelation::Unknown,
    cache_write_counts_in_total: false,
    client_reports_total: true,
    basis: "local measurement: total == input + output for all 19385 records",
};

/// Claude Code（Anthropic `message.usage`）。
///
/// 依据：本机 4 956 条 assistant 记录中，`cache_read_input_tokens > input_tokens` 出现在
/// 3 927 条、`cache_creation_input_tokens > input_tokens` 出现在 112 条，
/// 二者不可能是 input 的子集；客户端不提供 reasoning 与总量字段。
pub const CLAUDE_CODE_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "claude_code.anthropic_usage.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::NotProvided,
    cache_write_in_input: FieldRelation::Separate,
    cache_write_counts_in_total: true,
    client_reports_total: false,
    basis: "local measurement: cache_read > input on 3927/4956 records",
};

/// OpenCode（`message.data.tokens`）。
///
/// 依据：本机 5 490 条消息中 `total == input + output + reasoning + cache.read` 恒成立，
/// 且 `cache.read > input` 出现在 5 099 条，说明 cache_read 与 reasoning 均为独立字段；
/// `cache.write` 本机全为 0，与 input 的关系无法判定，故不计入总量。
pub const OPENCODE_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "opencode.message.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::Separate,
    cache_write_in_input: FieldRelation::Unknown,
    cache_write_counts_in_total: false,
    client_reports_total: true,
    basis: "local measurement: total == input + output + reasoning + cache.read",
};

pub fn semantics_for(client: Client) -> &'static TokenSemantics {
    match client {
        Client::Codex => &CODEX_SEMANTICS,
        Client::ClaudeCode => &CLAUDE_CODE_SEMANTICS,
        Client::OpenCode => &OPENCODE_SEMANTICS,
    }
}

/// 客户端原始计数。`None` 表示"未提供/缺失"，与 `0` 语义不同。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RawTokenCounts {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    pub reasoning: Option<u64>,
    /// 客户端自报总量（Codex/OpenCode 有，Claude Code 无）。
    pub client_total: Option<u64>,
}

impl RawTokenCounts {
    /// 按字段逐项求和。仅在同一客户端、同一口径的记录之间使用（去重叠是线性运算）。
    pub fn sum<'a, I>(items: I) -> RawTokenCounts
    where
        I: IntoIterator<Item = &'a RawTokenCounts>,
    {
        let mut out = RawTokenCounts::default();
        for item in items {
            add_opt(&mut out.input, item.input);
            add_opt(&mut out.output, item.output);
            add_opt(&mut out.cache_read, item.cache_read);
            add_opt(&mut out.cache_write, item.cache_write);
            add_opt(&mut out.reasoning, item.reasoning);
            add_opt(&mut out.client_total, item.client_total);
        }
        out
    }
}

/// 累加到 `Option<u64>`：`None` 表示缺失，缺失字段不会被当成 0 参与求和。
fn add_opt(slot: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *slot = Some(slot.unwrap_or(0).saturating_add(value));
    }
}

impl RawTokenCounts {}

/// 互不重叠的规范分桶：任意两项相加都不会重复计入同一批 token。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenBuckets {
    /// 未命中缓存的输入。
    pub input: Option<u64>,
    /// 不含推理的输出。
    pub output: Option<u64>,
    /// 推理 token（Codex 从 output 中拆出，OpenCode 独立字段，Claude Code 不提供）。
    pub reasoning: Option<u64>,
    /// 缓存读取。
    pub cache_read: Option<u64>,
    /// 缓存写入。仅在口径允许时计入总量（见 [`TokenSemantics::cache_write_counts_in_total`]）。
    pub cache_write: Option<u64>,
}

impl TokenBuckets {
    /// 各分桶之和（不含口径排除项）。任一必需分桶缺失时返回 `None`。
    pub fn sum_with(&self, semantics: &TokenSemantics) -> Option<u64> {
        let mut total = 0u64;
        for (value, required) in [
            (self.input, true),
            (self.output, true),
            (
                self.reasoning,
                semantics.reasoning != FieldRelation::NotProvided,
            ),
            (self.cache_read, true),
            (self.cache_write, semantics.cache_write_counts_in_total),
        ] {
            if !required {
                continue;
            }
            match value {
                Some(value) => total += value,
                None => return None,
            }
        }
        Some(total)
    }
}

/// 规范总量的取值依据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageTotalBasis {
    /// 直接使用客户端自报总量（measured）。
    ClientReported,
    /// 由互不重叠的分桶求和得出（derived）。
    DerivedFromBuckets,
    /// 字段缺失或关系未知，无法给出总量。
    Unavailable,
}

impl UsageTotalBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            UsageTotalBasis::ClientReported => "client_reported",
            UsageTotalBasis::DerivedFromBuckets => "derived_from_buckets",
            UsageTotalBasis::Unavailable => "unavailable",
        }
    }
}

/// 归一化后的用量：分桶 + 规范总量 + 未知项标注。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUsage {
    pub buckets: TokenBuckets,
    pub total_tokens: Option<u64>,
    pub total_basis: UsageTotalBasis,
    /// 无法确认的关系或缺失项（需要在 UI/导出中标注），按字典序稳定输出。
    pub unknown_relations: Vec<&'static str>,
    /// 客户端自报总量与分桶求和不一致（正常情况下应为 false，用于发现口径漂移）。
    pub client_total_mismatch: bool,
}

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

    #[test]
    fn semantics_are_registered_for_every_client() {
        for client in Client::ALL {
            let semantics = semantics_for(client);
            assert!(!semantics.id.is_empty());
            assert!(!semantics.basis.is_empty());
        }
        assert_eq!(Client::parse(Client::Codex.as_str()), Some(Client::Codex));
        assert_eq!(Client::parse("nope"), None);
    }
}
