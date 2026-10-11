//! Paginated request metadata for a single session; never reads source contents.

use super::{AnalyticsFilter, SessionSummary};
use crate::usage::model::decode_unknown_relations;
use crate::usage::store::{UsageError, UsageStore};
use rusqlite::OptionalExtension;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDetail {
    pub session: SessionSummary,
    pub requests: Vec<RequestDetail>,
    pub request_coverage: RequestCoverageDetail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDetail {
    pub dedup_key: String,
    pub seq: Option<i64>,
    pub created_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub source_kind: String,
    pub source_ref: Option<String>,
    pub is_sidechain: bool,
    pub unknown_relations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestCoverageDetail {
    pub total: i64,
    pub timed: i64,
    pub untimed: i64,
    pub with_usage: i64,
    pub without_usage: i64,
    pub estimated: i64,
    pub partial: bool,
}

impl UsageStore {
    /// Resolve by client identity, ignoring list date/project filters; include all request days.
    pub fn session_detail(
        &self,
        filter: &AnalyticsFilter<'_>,
        client_session_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<SessionDetail, UsageError> {
        let client = filter
            .client
            .ok_or_else(|| UsageError::InvalidInput("session detail requires a client".into()))?;
        let result = self
            .conn
            .query_row(
                "SELECT s.id, s.model, s.project_path, s.started_at_ms, s.ended_at_ms,
                    s.input_tokens, s.output_tokens, s.reasoning_tokens, s.cache_read_tokens,
                    s.cache_write_tokens, s.total_tokens, s.message_coverage,
                    s.total_basis, s.unknown_relations
             FROM sessions s WHERE s.client = ?1 AND s.client_session_id = ?2",
                rusqlite::params![client.as_str(), client_session_id],
                |row| {
                    let unknown: Option<String> = row.get(13)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        SessionSummary {
                            client,
                            client_session_id: client_session_id.into(),
                            model: row.get(1)?,
                            project_path: row.get(2)?,
                            started_at_ms: row.get(3)?,
                            ended_at_ms: row.get(4)?,
                            input_tokens: row.get(5)?,
                            output_tokens: row.get(6)?,
                            reasoning_tokens: row.get(7)?,
                            cache_read_tokens: row.get(8)?,
                            cache_write_tokens: row.get(9)?,
                            total_tokens: row.get(10)?,
                            messages: 0,
                            coverage: row.get(11)?,
                            total_basis: row.get(12)?,
                            unknown_relations: decode_unknown_relations(unknown.as_deref()),
                        },
                    ))
                },
            )
            .optional()?;
        let (session_id, mut session) =
            result.ok_or_else(|| UsageError::InvalidInput("session was not found".into()))?;
        let request_coverage = self.conn.query_row(
            "SELECT COUNT(*), COUNT(created_at_ms), COUNT(total_tokens),
                    COALESCE(SUM(source_kind = 'estimated'), 0)
             FROM messages WHERE session_id = ?1 AND client = ?2",
            rusqlite::params![session_id, client.as_str()],
            |row| {
                let total: i64 = row.get(0)?;
                let timed: i64 = row.get(1)?;
                let with_usage: i64 = row.get(2)?;
                Ok(RequestCoverageDetail {
                    total,
                    timed,
                    untimed: total - timed,
                    with_usage,
                    without_usage: total - with_usage,
                    estimated: row.get(3)?,
                    partial: session.coverage == "partial",
                })
            },
        )?;
        session.messages = request_coverage.total;
        let mut statement = self.conn.prepare(
            "SELECT dedup_key, seq, created_at_ms, completed_at_ms, model, agent,
                    input_tokens, output_tokens, reasoning_tokens, cache_read_tokens,
                    cache_write_tokens, total_tokens, source_kind, source_ref,
                    is_sidechain, unknown_relations
             FROM messages WHERE session_id = ?1 AND client = ?2
             ORDER BY created_at_ms IS NULL, created_at_ms, seq IS NULL, seq, id
             LIMIT ?3 OFFSET ?4",
        )?;
        let rows = statement.query_map(
            rusqlite::params![session_id, client.as_str(), limit.min(200), offset],
            |row| {
                let unknown: Option<String> = row.get(15)?;
                Ok(RequestDetail {
                    dedup_key: row.get(0)?,
                    seq: row.get(1)?,
                    created_at_ms: row.get(2)?,
                    completed_at_ms: row.get(3)?,
                    model: row.get(4)?,
                    agent: row.get(5)?,
                    input_tokens: row.get(6)?,
                    output_tokens: row.get(7)?,
                    reasoning_tokens: row.get(8)?,
                    cache_read_tokens: row.get(9)?,
                    cache_write_tokens: row.get(10)?,
                    total_tokens: row.get(11)?,
                    source_kind: row.get(12)?,
                    source_ref: row.get(13)?,
                    is_sidechain: row.get(14)?,
                    unknown_relations: decode_unknown_relations(unknown.as_deref()),
                })
            },
        )?;
        Ok(SessionDetail {
            session,
            requests: rows.collect::<rusqlite::Result<Vec<_>>>()?,
            request_coverage,
        })
    }
}

#[cfg(test)]
#[path = "session_detail_tests.rs"]
mod tests;
