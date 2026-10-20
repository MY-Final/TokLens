//! 用量归集口径：请求级优先，没有请求明细的会话回落到会话级。
//!
//! 会话可以跨天运行（例如 OpenCode 的长会话）。只按 `sessions.started_at_ms` 归集会把
//! 跨天的全部用量堆到会话开始的那一天，"今天的用量"因此对不上。这里统一按下面的规则归集：
//!
//! - **请求级**：会话存在带 `created_at_ms` 的明细时，每条明细按自身时间归集，用量取该条明细的分桶值；
//! - **会话级**：会话没有任何带时间的明细时，按 `started_at_ms` 归集，用量取客户端自报的会话累计值。
//!
//! 两个分支互斥：同一个会话只会落进一个分支，归集总量不会重复计入。
//! 明细求和与客户端自报总量的差异由 `sessions.client_total_mismatch` 标记，不在这里调和。

/// 归集 SQL 使用的参数位置。
pub(super) struct AttributionParams {
    /// 区间起点（闭区间）。
    pub from: usize,
    /// 区间终点（开区间）。
    pub to: usize,
    /// 客户端过滤参数位置（可选）。
    pub client: Option<usize>,
    /// 项目路径过滤参数位置（可选）。
    pub project_path: Option<usize>,
}

impl AttributionParams {
    /// 区间与实体过滤条件；`column` 是被归集到区间里的时间列。
    fn filter(&self, column: &str) -> String {
        let mut parts = vec![
            format!("(?{from} IS NULL OR {column} >= ?{from})", from = self.from),
            format!("(?{to} IS NULL OR {column} < ?{to})", to = self.to),
        ];
        if let Some(index) = self.client {
            parts.push(format!("(?{index} IS NULL OR s.client = ?{index})"));
        }
        if let Some(index) = self.project_path {
            parts.push(format!("(?{index} IS NULL OR s.project_path = ?{index})"));
        }
        parts.join(" AND ")
    }
}

/// 生成归集 CTE：`attributed` 每一行是一个归集单元。
///
/// 列含义：`at_ms` 是归集时间，`from_request` 标记该行来自请求明细（1）还是会话累计（0），
/// `session_total_tokens` 是会话级的总量（用于"未知用量"计数），其余分桶列是参与求和的数值。
pub(super) fn attributed_cte(params: &AttributionParams) -> String {
    let request_filter = params.filter("m.created_at_ms");
    let session_filter = params.filter("s.started_at_ms");
    format!(
        "WITH attributed AS (
             SELECT s.client AS client, s.id AS session_id, s.project_path AS project_path, s.model AS model,
                    s.total_tokens AS session_total_tokens, s.message_coverage AS message_coverage,
                    s.client_total_mismatch AS client_total_mismatch,
                    m.created_at_ms AS at_ms,
                    m.input_tokens AS input_tokens, m.output_tokens AS output_tokens,
                    m.reasoning_tokens AS reasoning_tokens, m.cache_read_tokens AS cache_read_tokens,
                    m.cache_write_tokens AS cache_write_tokens, m.total_tokens AS total_tokens,
                    1 AS from_request
               FROM messages AS m JOIN sessions AS s ON s.id = m.session_id
              WHERE m.created_at_ms IS NOT NULL AND {request_filter}
             UNION ALL
             SELECT s.client, s.id, s.project_path, s.model,
                    s.total_tokens, s.message_coverage, s.client_total_mismatch,
                    s.started_at_ms,
                    s.input_tokens, s.output_tokens, s.reasoning_tokens, s.cache_read_tokens,
                    s.cache_write_tokens, s.total_tokens,
                    0
               FROM sessions AS s
              WHERE NOT EXISTS (SELECT 1 FROM messages AS m WHERE m.session_id = s.id AND m.created_at_ms IS NOT NULL)
                AND {session_filter}
         )"
    )
}
