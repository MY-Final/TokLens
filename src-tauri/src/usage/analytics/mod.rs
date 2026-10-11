//! 统计查询：全部基于 `sessions`（总量唯一事实来源）与 `messages`（明细）表。
//!
//! 口径约定（与 `docs/data-model.md` 一致）：
//!
//! - 所有 token 数值来自归一化后的**互不重叠分桶**，求和不会重复计入子集字段；
//! - `cache_write_tokens` 是独立上报值，不计入 `total_tokens`；
//! - 时间范围按会话的 `started_at_ms` 过滤（毫秒，UTC）；
//! - 缺失值保持 `None`/`NULL`，聚合时不被当成 0 参与"总量"，只会体现为"无用量会话数"。

mod aggregates;
mod rankings;
mod records;
mod sql;
mod types;

pub use types::{
    AnalyticsFilter, ClientSourceStatus, ClientUsage, DailyBucket, GroupUsage, RangeTotals,
    SessionSummary, TokenSums,
};

#[cfg(test)]
mod tests;
