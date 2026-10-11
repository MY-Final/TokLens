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
        self.group_ranking("s.model", filter, limit)
    }

    /// 项目排行：按会话的 `project_path` 聚合。
    pub fn project_ranking(
        &self,
        filter: &AnalyticsFilter<'_>,
        limit: u32,
    ) -> Result<Vec<GroupUsage>, UsageError> {
        self.group_ranking("s.project_path", filter, limit)
    }

    fn group_ranking(
        &self,
        column: &str,
        filter: &AnalyticsFilter<'_>,
        limit: u32,
    ) -> Result<Vec<GroupUsage>, UsageError> {
        let client = filter.client.map(Client::as_str);
        let sql = format!(
            "SELECT {column}, COUNT(*),
                    (SELECT COUNT(*) FROM messages AS m WHERE m.session_id = s.id),
                    COALESCE(SUM(s.total_tokens), 0)
             FROM sessions AS s
             WHERE (?1 IS NULL OR s.started_at_ms >= ?1)
               AND (?2 IS NULL OR s.started_at_ms < ?2)
               AND (?3 IS NULL OR s.client = ?3)
               AND (?4 IS NULL OR s.project_path = ?4)
             GROUP BY {column}
             ORDER BY COALESCE(SUM(s.total_tokens), 0) DESC, COUNT(*) DESC
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
