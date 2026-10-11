//! Watermark reads and usage reconciliation queries.

use super::{SourceFileRow, TotalsFilter, UsageError, UsageStore, UsageTotals};
use crate::usage::client::Client;
use crate::usage::model::{decode_unknown_relations, session_id};
use rusqlite::{OptionalExtension, params};

impl UsageStore {
    pub fn session_count(&self) -> Result<i64, UsageError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?)
    }

    pub fn message_count(&self) -> Result<i64, UsageError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?)
    }

    /// 读取源文件水位；应用重启后据此判断是否重新解析。
    pub fn source_file(
        &self,
        client: Client,
        path: &str,
    ) -> Result<Option<SourceFileRow>, UsageError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, client, path, kind, size_bytes, mtime_ms, content_offset, parser_version,
                        last_scan_ms, unparsed_records
                 FROM source_files WHERE client = ?1 AND path = ?2",
                params![client.as_str(), path],
                |row| {
                    Ok(SourceFileRow {
                        id: row.get(0)?,
                        client: row.get(1)?,
                        path: row.get(2)?,
                        kind: row.get(3)?,
                        size_bytes: row.get(4)?,
                        mtime_ms: row.get(5)?,
                        content_offset: row.get(6)?,
                        parser_version: row.get(7)?,
                        last_scan_ms: row.get(8)?,
                        unparsed_records: row.get(9)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// 按过滤条件汇总用量。只统计 `sessions` 表，避免与会话明细重复相加。
    pub fn totals(&self, filter: &TotalsFilter<'_>) -> Result<UsageTotals, UsageError> {
        let client = filter.client.map(|client| client.as_str());
        let totals = self.conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(reasoning_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_write_tokens), 0),
                    COALESCE(SUM(total_tokens), 0)
             FROM sessions
             WHERE (?1 IS NULL OR client = ?1)
               AND (?2 IS NULL OR project_path = ?2)
               AND (?3 IS NULL OR started_at_ms >= ?3)
               AND (?4 IS NULL OR started_at_ms < ?4)",
            params![client, filter.project_path, filter.from_ms, filter.to_ms],
            |row| {
                Ok(UsageTotals {
                    sessions: row.get(0)?,
                    messages: 0,
                    input_tokens: row.get(1)?,
                    output_tokens: row.get(2)?,
                    reasoning_tokens: row.get(3)?,
                    cache_read_tokens: row.get(4)?,
                    cache_write_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                })
            },
        )?;
        let messages: i64 = self.conn.query_row(
            "SELECT COUNT(*)
             FROM messages AS m
             JOIN sessions AS s ON s.id = m.session_id
             WHERE (?1 IS NULL OR s.client = ?1)
               AND (?2 IS NULL OR s.project_path = ?2)
               AND (?3 IS NULL OR s.started_at_ms >= ?3)
               AND (?4 IS NULL OR s.started_at_ms < ?4)",
            params![client, filter.project_path, filter.from_ms, filter.to_ms],
            |row| row.get(0),
        )?;
        Ok(UsageTotals { messages, ..totals })
    }

    /// 某个会话的明细求和（一致性校验：OpenCode / Claude Code 的会话列应等于明细求和）。
    pub fn messages_sum(
        &self,
        client: Client,
        client_session_id: &str,
    ) -> Result<UsageTotals, UsageError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(reasoning_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_write_tokens), 0),
                    COALESCE(SUM(total_tokens), 0)
             FROM messages WHERE client = ?1 AND session_id = ?2",
            params![client.as_str(), session_id(client, client_session_id)],
            |row| {
                Ok(UsageTotals {
                    sessions: 0,
                    messages: row.get(0)?,
                    input_tokens: row.get(1)?,
                    output_tokens: row.get(2)?,
                    reasoning_tokens: row.get(3)?,
                    cache_read_tokens: row.get(4)?,
                    cache_write_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                })
            },
        )?)
    }

    /// 读取某个会话行的关键字段（测试与诊断用）。
    pub fn session_tokens(
        &self,
        client: Client,
        client_session_id: &str,
    ) -> Result<Option<UsageTotals>, UsageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COALESCE(input_tokens, 0), COALESCE(output_tokens, 0), COALESCE(reasoning_tokens, 0),
                        COALESCE(cache_read_tokens, 0), COALESCE(cache_write_tokens, 0), COALESCE(total_tokens, 0)
                 FROM sessions WHERE client = ?1 AND client_session_id = ?2",
                params![client.as_str(), client_session_id],
                |row| {
                    Ok(UsageTotals {
                        sessions: 1,
                        messages: 0,
                        input_tokens: row.get(0)?,
                        output_tokens: row.get(1)?,
                        reasoning_tokens: row.get(2)?,
                        cache_read_tokens: row.get(3)?,
                        cache_write_tokens: row.get(4)?,
                        total_tokens: row.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    /// 读取某个会话的未知项标注。
    pub fn session_unknown_relations(
        &self,
        client: Client,
        client_session_id: &str,
    ) -> Result<Vec<String>, UsageError> {
        let value: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT unknown_relations FROM sessions WHERE client = ?1 AND client_session_id = ?2",
                params![client.as_str(), client_session_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(decode_unknown_relations(value.flatten().as_deref()))
    }
}
