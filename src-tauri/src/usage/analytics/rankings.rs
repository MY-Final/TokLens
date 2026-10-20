//! Model and project grouping queries.

use super::{AnalyticsFilter, GroupUsage};
use crate::usage::client::Client;
use crate::usage::store::{UsageError, UsageStore};

impl UsageStore {
    /// 模型排行：按会话主模型聚合（Codex 逐请求模型未落库，见 `docs/data-model.md` 第 8 节）。
    pub fn model_ranking(
        &self,
        filter: &AnalyticsFilter<'_>,
        limit: u32,
    ) -> Result<Vec<GroupUsage>, UsageError> {
        self.group_ranking("model", filter, limit)
    }

    /// 项目排行：按会话的 `project_path` 聚合。
    pub fn project_ranking(
        &self,
        filter: &AnalyticsFilter<'_>,
        limit: u32,
    ) -> Result<Vec<GroupUsage>, UsageError> {
        self.group_ranking("project_path", filter, limit)
    }

    fn group_ranking(
        &self,
        column: &str,
        filter: &AnalyticsFilter<'_>,
        limit: u32,
    ) -> Result<Vec<GroupUsage>, UsageError> {
        let client = filter.client.map(Client::as_str);
        // 归集口径见 `attribution.rs`：用量按请求时间归集，分组键仍取会话自身的模型 / 项目。
        let cte = super::attribution::attributed_cte(&super::attribution::AttributionParams {
            from: 1,
            to: 2,
            client: Some(3),
            project_path: Some(4),
        });
        let sql = format!(
            "{cte}
             SELECT {column},
                    COUNT(DISTINCT session_id),
                    COALESCE(SUM(CASE WHEN from_request = 1 THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(total_tokens), 0),
                    COUNT(DISTINCT CASE WHEN session_total_tokens IS NULL THEN session_id END)
             FROM attributed
             GROUP BY {column}
             ORDER BY SUM(CASE WHEN total_tokens IS NULL THEN 1 ELSE 0 END) > 0, COALESCE(SUM(total_tokens), 0) DESC, COUNT(DISTINCT session_id) DESC
             LIMIT ?5"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params![
                filter.from_ms,
                filter.to_ms,
                client,
                filter.project_path,
                limit
            ],
            |row| {
                Ok(GroupUsage {
                    key: row.get(0)?,
                    sessions: row.get(1)?,
                    messages: row.get(2)?,
                    total_tokens: row.get(3)?,
                    sessions_without_usage: row.get(4)?,
                })
            },
        )?;
        let mut groups = Vec::new();
        for row in rows {
            groups.push(row?);
        }
        Ok(groups)
    }
}
