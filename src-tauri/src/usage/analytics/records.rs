//! Session summaries and source status queries.

use super::{AnalyticsFilter, ClientSourceStatus, SessionListOptions, SessionSort, SessionSummary};
use crate::usage::client::Client;
use crate::usage::store::{UsageError, UsageStore};

impl UsageStore {
    /// 会话列表（最近优先），只返回元数据与用量，不含任何正文。
    pub fn session_summaries(
        &self,
        filter: &AnalyticsFilter<'_>,
        options: &SessionListOptions<'_>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<SessionSummary>, UsageError> {
        let client = filter.client.map(Client::as_str);
        let mut statement = self.conn.prepare(
            "SELECT s.client, s.client_session_id, s.model, s.project_path, s.started_at_ms,
                    s.ended_at_ms, s.input_tokens, s.output_tokens, s.reasoning_tokens,
                    s.cache_read_tokens, s.cache_write_tokens, s.total_tokens,
                    (SELECT COUNT(*) FROM messages AS m WHERE m.session_id = s.id),
                    s.message_coverage, s.total_basis, s.unknown_relations
             FROM sessions AS s
             WHERE ((?12 = 0 AND (?1 IS NULL OR s.started_at_ms >= ?1)
                      AND (?2 IS NULL OR s.started_at_ms < ?2))
                    OR (?12 = 1 AND EXISTS (
                        SELECT 1 FROM messages AS r WHERE r.session_id = s.id
                        AND r.created_at_ms IS NOT NULL AND r.source_kind != 'estimated'
                        AND (?1 IS NULL OR r.created_at_ms >= ?1)
                        AND (?2 IS NULL OR r.created_at_ms < ?2))))
               AND (?3 IS NULL OR s.client = ?3)
               AND (?4 IS NULL OR s.project_path = ?4)
               AND (?5 IS NULL OR instr(lower(COALESCE(s.project_path, '')), lower(?5)) > 0
                    OR instr(lower(COALESCE(s.model, '')), lower(?5)) > 0
                    OR instr(lower(s.client_session_id), lower(?5)) > 0)
               AND (?9 IS NULL OR s.model = ?9)
               AND (?10 = 0 OR s.model IS NULL)
               AND (?11 = 0 OR s.project_path IS NULL)
             ORDER BY
                CASE WHEN ?6 = 0 THEN s.started_at_ms IS NULL END ASC,
                CASE WHEN ?6 = 0 THEN s.started_at_ms END DESC,
                CASE WHEN ?6 = 1 THEN s.started_at_ms IS NULL END ASC,
                CASE WHEN ?6 = 1 THEN s.started_at_ms END ASC,
                CASE WHEN ?6 = 2 THEN s.total_tokens IS NULL END ASC,
                CASE WHEN ?6 = 2 THEN s.total_tokens END DESC,
                CASE WHEN ?6 = 3 THEN (SELECT COUNT(*) FROM messages AS m WHERE m.session_id = s.id) END DESC,
                s.client_session_id DESC, s.client DESC
             LIMIT ?7 OFFSET ?8",
        )?;
        let rows = statement.query_map(
            rusqlite::params![
                filter.from_ms,
                filter.to_ms,
                client,
                filter.project_path,
                options.search,
                match options.sort {
                    SessionSort::Newest => 0,
                    SessionSort::Oldest => 1,
                    SessionSort::TotalTokens => 2,
                    SessionSort::Requests => 3,
                },
                limit,
                offset,
                options.model,
                options.model_unknown,
                options.project_unknown,
                options.request_time,
            ],
            |row| {
                let client_id: String = row.get(0)?;
                // 无法识别的客户端字符串（来自更新版本或外部数据）不得被伪装成其他客户端；
                // 跳过该行，与 requests.rs 的 `if let Some` 处理保持一致。
                let Some(client) = Client::parse(&client_id) else {
                    return Ok(None);
                };
                let unknown: Option<String> = row.get(15)?;
                Ok(Some(SessionSummary {
                    client,
                    client_session_id: row.get(1)?,
                    model: row.get(2)?,
                    project_path: row.get(3)?,
                    started_at_ms: row.get(4)?,
                    ended_at_ms: row.get(5)?,
                    input_tokens: row.get(6)?,
                    output_tokens: row.get(7)?,
                    reasoning_tokens: row.get(8)?,
                    cache_read_tokens: row.get(9)?,
                    cache_write_tokens: row.get(10)?,
                    total_tokens: row.get(11)?,
                    messages: row.get(12)?,
                    coverage: row.get(13)?,
                    total_basis: row.get(14)?,
                    unknown_relations: crate::usage::model::decode_unknown_relations(
                        unknown.as_deref(),
                    ),
                }))
            },
        )?;
        let mut sessions = Vec::new();
        for row in rows {
            if let Some(summary) = row? {
                sessions.push(summary);
            }
        }
        Ok(sessions)
    }

    /// 各客户端的数据源状态（会话/明细计数、已跟踪文件、未解析记录数、最近扫描时间）。
    pub fn client_source_status(&self) -> Result<Vec<ClientSourceStatus>, UsageError> {
        let mut status = Vec::new();
        for client in Client::ALL {
            let (sessions, sessions_without_usage, messages): (i64, i64, i64) =
                self.conn.query_row(
                    "SELECT (SELECT COUNT(*) FROM sessions WHERE client = ?1),
                        (SELECT COUNT(*) FROM sessions WHERE client = ?1 AND total_tokens IS NULL),
                        (SELECT COUNT(*) FROM messages WHERE client = ?1)",
                    rusqlite::params![client.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
            let (files, unparsed, last_scan): (i64, i64, Option<i64>) = self.conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(unparsed_records), 0), MAX(last_scan_ms)
                 FROM source_files WHERE client = ?1",
                rusqlite::params![client.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let last_request_ms: Option<i64> = self.conn.query_row(
                "SELECT MAX(created_at_ms) FROM messages
                 WHERE client = ?1 AND created_at_ms IS NOT NULL
                   AND source_kind IN ('measured', 'derived')",
                rusqlite::params![client.as_str()],
                |row| row.get(0),
            )?;
            status.push(ClientSourceStatus {
                client,
                sessions,
                sessions_without_usage,
                messages,
                files_tracked: files,
                unparsed_records: unparsed,
                last_scan_ms: last_scan,
                last_request_ms,
            });
        }
        Ok(status)
    }

    /// 已采集数据的时间范围（最早 / 最近事件），用于"全部"区间与空数据判断。
    pub fn data_bounds(&self) -> Result<(Option<i64>, Option<i64>), UsageError> {
        Ok(self.conn.query_row(
            "SELECT MIN(started_at_ms), MAX(COALESCE(ended_at_ms, started_at_ms)) FROM sessions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }
}
