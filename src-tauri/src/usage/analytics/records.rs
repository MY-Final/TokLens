//! Session summaries and source status queries.

use super::{AnalyticsFilter, ClientSourceStatus, SessionSummary};
use crate::usage::client::Client;
use crate::usage::store::{UsageError, UsageStore};

impl UsageStore {
    /// 会话列表（最近优先），只返回元数据与用量，不含任何正文。
    pub fn session_summaries(
        &self,
        filter: &AnalyticsFilter<'_>,
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
             WHERE (?1 IS NULL OR s.started_at_ms >= ?1)
               AND (?2 IS NULL OR s.started_at_ms < ?2)
               AND (?3 IS NULL OR s.client = ?3)
               AND (?4 IS NULL OR s.project_path = ?4)
             ORDER BY COALESCE(s.started_at_ms, 0) DESC, s.client_session_id DESC
             LIMIT ?5 OFFSET ?6",
        )?;
        let rows = statement.query_map(
            rusqlite::params![
                filter.from_ms,
                filter.to_ms,
                client,
                filter.project_path,
                limit,
                offset
            ],
            |row| {
                let client: String = row.get(0)?;
                let unknown: Option<String> = row.get(15)?;
                Ok(SessionSummary {
                    client: Client::parse(&client).unwrap_or(Client::Codex),
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
                })
            },
        )?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }

    /// 各客户端的数据源状态（会话/明细计数、已跟踪文件、未解析记录数、最近扫描时间）。
    pub fn client_source_status(&self) -> Result<Vec<ClientSourceStatus>, UsageError> {
        let mut status = Vec::new();
        for client in Client::ALL {
            let (sessions, messages): (i64, i64) = self.conn.query_row(
                "SELECT (SELECT COUNT(*) FROM sessions WHERE client = ?1),
                        (SELECT COUNT(*) FROM messages WHERE client = ?1)",
                rusqlite::params![client.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let (files, unparsed, last_scan): (i64, i64, Option<i64>) = self.conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(unparsed_records), 0), MAX(last_scan_ms)
                 FROM source_files WHERE client = ?1",
                rusqlite::params![client.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            status.push(ClientSourceStatus {
                client,
                sessions,
                messages,
                files_tracked: files,
                unparsed_records: unparsed,
                last_scan_ms: last_scan,
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
