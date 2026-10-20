//! Token 字段语义与去重叠规则。
//!
//! 各客户端的 token 字段语义并不相同，直接相加会重复计数。这里的规则全部来自
//! `docs/data-source-research.md` 的本机实测结论（每个客户端给出判定依据），
//! 无法判定的关系一律标记为 [`FieldRelation::Unknown`]，不计入总量并在记录上标注，
//! 不做任何推测。

// 各子模块只对外暴露稳定的 Token 领域 API。
mod counts;
mod normalize;
mod semantics;

// 兼容现有调用方；客户端标识的唯一实现位于 usage::client。
pub use crate::usage::client::Client;
pub use counts::{NormalizedUsage, RawTokenCounts, TokenBuckets, UsageTotalBasis};
pub use normalize::{
    ANOMALY_CACHE_READ_EXCEEDS_INPUT, ANOMALY_CACHE_WRITE_EXCEEDS_INPUT,
    ANOMALY_REASONING_EXCEEDS_OUTPUT, UNKNOWN_CACHE_READ_MISSING, UNKNOWN_CACHE_WRITE_RELATION,
    UNKNOWN_REASONING_MISSING, UNKNOWN_TOTAL_INCOMPLETE, normalize_usage,
};
pub use semantics::{
    CLAUDE_CODE_SEMANTICS, CLINE_SEMANTICS, CODEX_SEMANTICS, FieldRelation, OPENCODE_SEMANTICS,
    TokenSemantics, semantics_for,
};
