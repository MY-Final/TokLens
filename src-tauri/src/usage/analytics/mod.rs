//! 统计查询：会话累计基于 `sessions`，请求时间统计独立基于 `messages`。
//!
//! 口径约定（与 `docs/data-model.md` 一致）：
//!
//! - 所有 token 数值来自归一化后的**互不重叠分桶**，求和不会重复计入子集字段；
//! - 缓存写入是否计入总量遵循各客户端的归一化口径；
//! - 用量按**请求时间优先**归集：会话有请求明细时按明细时间落桶，没有明细时按会话开始时间落桶
//!   （见 `attribution.rs`）；
//! - 缺失值保持 `None`/`NULL`，聚合仅求和已知值，并分别提供缺失计数。

mod aggregates;
mod attribution;
mod rankings;
mod records;
mod report;
mod requests;
mod session_detail;
mod sql;
mod types;

pub use requests::{
    RequestActivity, RequestAnalysis, RequestAnalysisGroup, RequestAnalysisMetrics,
    RequestCoverage, RequestDailyBucket, RequestDailyClient, RequestHourlyBucket, RequestTotals,
};
pub use session_detail::{RequestCoverageDetail, RequestDetail, SessionDetail};
pub use types::{
    AnalyticsFilter, ClientSourceStatus, ClientUsage, DailyBucket, GroupUsage, RangeTotals,
    SessionListOptions, SessionSort, SessionSummary, TokenSums,
};

#[cfg(test)]
mod tests;
