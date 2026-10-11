//! 统计查询：会话累计基于 `sessions`，请求时间统计独立基于 `messages`。
//!
//! 口径约定（与 `docs/data-model.md` 一致）：
//!
//! - 所有 token 数值来自归一化后的**互不重叠分桶**，求和不会重复计入子集字段；
//! - 缓存写入是否计入总量遵循各客户端的归一化口径；
//! - 会话累计按 `started_at_ms` 筛选，请求统计按 `created_at_ms` 筛选；
//! - 缺失值保持 `None`/`NULL`，聚合仅求和已知值，并分别提供缺失计数。

mod aggregates;
mod rankings;
mod records;
mod requests;
mod session_detail;
mod sql;
mod types;

pub use requests::{
    RequestActivity, RequestCoverage, RequestDailyBucket, RequestDailyClient, RequestTotals,
};
pub use session_detail::{RequestCoverageDetail, RequestDetail, SessionDetail};
pub use types::{
    AnalyticsFilter, ClientSourceStatus, ClientUsage, DailyBucket, GroupUsage, RangeTotals,
    SessionSummary, TokenSums,
};

#[cfg(test)]
mod tests;
