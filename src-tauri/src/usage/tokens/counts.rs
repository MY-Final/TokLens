//! 原始计数、互不重叠的分桶与归一化结果。

use super::semantics::{FieldRelation, TokenSemantics};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summing_counts_preserves_missing_fields_and_recorded_zero() {
        assert_eq!(RawTokenCounts::sum([]), RawTokenCounts::default());
        let items = [
            RawTokenCounts::default(),
            RawTokenCounts {
                input: Some(0),
                output: Some(12),
                ..RawTokenCounts::default()
            },
            RawTokenCounts {
                output: Some(8),
                ..RawTokenCounts::default()
            },
        ];
        assert_eq!(
            RawTokenCounts::sum(&items),
            RawTokenCounts {
                input: Some(0),
                output: Some(20),
                ..RawTokenCounts::default()
            }
        );
    }

    #[test]
    fn summing_counts_saturates_instead_of_wrapping() {
        let items = [
            RawTokenCounts {
                input: Some(u64::MAX),
                ..RawTokenCounts::default()
            },
            RawTokenCounts {
                input: Some(1),
                ..RawTokenCounts::default()
            },
        ];
        assert_eq!(RawTokenCounts::sum(&items).input, Some(u64::MAX));
    }
}
