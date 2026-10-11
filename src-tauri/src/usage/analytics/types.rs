//! Analytics filters and result records.

use crate::usage::client::Client;

/// 查询过滤条件。
#[derive(Debug, Clone, Copy, Default)]
pub struct AnalyticsFilter<'a> {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub client: Option<Client>,
    /// 项目路径精确匹配（用于按项目下钻）。
    pub project_path: Option<&'a str>,
}

/// 聚合结果（token 分桶之和）。缺失值不参与求和，单独由 `sessions_without_usage` 统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenSums {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub counted_cache_write_tokens: i64,
    pub total_tokens: i64,
}

/// 一段区间内的统计快照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RangeTotals {
    pub sessions: i64,
    pub messages: i64,
    pub sums: TokenSums,
    /// 完全没有用量数据的会话数（`total_tokens` 为 NULL）。
    pub sessions_without_usage: i64,
    /// 请求明细不完整的会话数（`message_coverage = 'partial'`）。
    pub sessions_partial: i64,
    /// 客户端自报总量与其分项之和不一致的会话数（已按客户端值计入并标记）。
    pub sessions_total_mismatch: i64,
}

/// 单日聚合。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DailyBucket {
    pub day_start_ms: i64,
    pub sessions: i64,
    pub messages: i64,
    pub sums: TokenSums,
}

/// 按客户端聚合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientUsage {
    pub share_bps: i64,
    pub client: Client,
    pub sessions: i64,
    pub messages: i64,
    pub sums: TokenSums,
}

/// 分组排行（模型 / 项目）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupUsage {
    /// 分组键；`None` 表示客户端未记录该字段（UI 需显示为"未知"）。
    pub key: Option<String>,
    pub sessions: i64,
    pub messages: i64,
    pub total_tokens: i64,
}

/// 会话摘要（不含任何正文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub client: Client,
    pub client_session_id: String,
    pub model: Option<String>,
    pub project_path: Option<String>,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub messages: i64,
    pub coverage: String,
    pub total_basis: String,
    pub unknown_relations: Vec<String>,
}

/// 单个数据源（客户端）的状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSourceStatus {
    pub client: Client,
    pub sessions: i64,
    pub messages: i64,
    pub files_tracked: i64,
    pub unparsed_records: i64,
    pub last_scan_ms: Option<i64>,
}
