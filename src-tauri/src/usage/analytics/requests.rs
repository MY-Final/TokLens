//! Request-time analytics, independent of session cumulative usage.

use super::sql::read_sums;
use super::{AnalyticsFilter, TokenSums};
use crate::usage::client::Client;
use crate::usage::store::{UsageError, UsageStore};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequestTotals {
    pub sessions: i64,
    pub requests: i64,
    pub requests_with_usage: i64,
    pub requests_without_usage: i64,
    pub measured_requests: i64,
    pub derived_requests: i64,
    pub sums: TokenSums,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDailyClient {
    pub client: Client,
    pub totals: RequestTotals,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDailyBucket {
    pub day_start_ms: i64,
    pub totals: RequestTotals,
    pub clients: Vec<RequestDailyClient>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequestCoverage {
    pub recorded_requests: i64,
    pub timed_requests: i64,
    pub untimed_requests: i64,
    pub estimated_requests: i64,
    pub sessions_partial: i64,
    pub sessions_without_requests: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestActivity {
    pub range: RequestTotals,
    pub clients: Vec<RequestAnalysisGroup>,
    pub daily: Vec<RequestDailyBucket>,
    pub coverage: RequestCoverage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestAnalysisMetrics {
    pub totals: RequestTotals,
    pub average_tokens: Option<f64>,
    pub share_bps: Option<i64>,
    pub cache_hit_bps: Option<i64>,
    pub cache_known_requests: i64,
    pub known_token_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestAnalysisGroup {
    pub key: Option<String>,
    pub metrics: RequestAnalysisMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestAnalysis {
    pub summary: RequestAnalysisMetrics,
    pub clients: Vec<RequestAnalysisGroup>,
    pub models: Vec<RequestAnalysisGroup>,
    pub projects: Vec<RequestAnalysisGroup>,
    pub coverage: RequestCoverage,
}

const ANALYSIS_COLUMNS: &str = "COALESCE(SUM(m.input_tokens IS NOT NULL AND m.cache_read_tokens IS NOT NULL),0),
    COALESCE(SUM(CASE WHEN m.input_tokens IS NOT NULL AND m.cache_read_tokens IS NOT NULL THEN m.input_tokens END),0),
    COALESCE(SUM(CASE WHEN m.input_tokens IS NOT NULL AND m.cache_read_tokens IS NOT NULL THEN m.cache_read_tokens END),0),
    COUNT(m.input_tokens), COUNT(m.output_tokens), COUNT(m.reasoning_tokens), COUNT(m.cache_read_tokens), COUNT(m.cache_write_tokens)";

fn ratio_bps(numerator: i128, denominator: i128) -> Option<i64> {
    (denominator > 0).then(|| ((numerator * 10_000) / denominator) as i64)
}

fn read_analysis(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<RequestAnalysisMetrics> {
    let totals = read_totals(row, offset)?;
    let cache_input: i64 = row.get(offset + 14)?;
    let cache_read: i64 = row.get(offset + 15)?;
    Ok(RequestAnalysisMetrics {
        average_tokens: (totals.requests_with_usage > 0)
            .then(|| totals.sums.total_tokens as f64 / totals.requests_with_usage as f64),
        totals,
        share_bps: None,
        cache_hit_bps: ratio_bps(
            cache_read.into(),
            i128::from(cache_input) + i128::from(cache_read),
        ),
        cache_known_requests: row.get(offset + 13)?,
        known_token_fields: [
            "inputTokens",
            "outputTokens",
            "reasoningTokens",
            "cacheReadTokens",
            "cacheWriteTokens",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, field)| Ok((field, row.get::<_, i64>(offset + 16 + index)?)))
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(field, _)| field.into())
        .collect(),
    })
}

const REQUEST_COLUMNS: &str = "COUNT(DISTINCT m.session_id), COUNT(*),
    COALESCE(SUM(m.total_tokens IS NOT NULL), 0),
    COALESCE(SUM(m.total_tokens IS NULL), 0),
    COALESCE(SUM(m.source_kind = 'measured'), 0),
    COALESCE(SUM(m.source_kind = 'derived'), 0),
    COALESCE(SUM(m.input_tokens), 0), COALESCE(SUM(m.output_tokens), 0),
    COALESCE(SUM(m.reasoning_tokens), 0), COALESCE(SUM(m.cache_read_tokens), 0),
    COALESCE(SUM(m.cache_write_tokens), 0), COALESCE(SUM(m.total_tokens), 0),
    COALESCE(SUM(CASE WHEN s.client IN ('claude_code', 'dsh', 'kimi_code', 'mimocode', 'pi_desktop', 'copilot')
                     THEN m.cache_write_tokens ELSE 0 END), 0)";

const REQUEST_FROM: &str = "FROM messages m JOIN sessions s ON s.id = m.session_id";
const REQUEST_WHERE: &str = "WHERE m.source_kind IN ('measured', 'derived')
    AND m.created_at_ms IS NOT NULL
    AND (?1 IS NULL OR m.created_at_ms >= ?1)
    AND (?2 IS NULL OR m.created_at_ms < ?2)
    AND (?3 IS NULL OR s.client = ?3)
    AND (?4 IS NULL OR s.project_path = ?4)";

fn read_totals(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<RequestTotals> {
    Ok(RequestTotals {
        sessions: row.get(offset)?,
        requests: row.get(offset + 1)?,
        requests_with_usage: row.get(offset + 2)?,
        requests_without_usage: row.get(offset + 3)?,
        measured_requests: row.get(offset + 4)?,
        derived_requests: row.get(offset + 5)?,
        sums: read_sums(row, offset + 6)?,
    })
}

impl UsageStore {
    /// Full-range comparisons and drill-downs use one read snapshot, never truncated daily buckets.
    pub fn request_analysis(
        &self,
        filter: &AnalyticsFilter<'_>,
    ) -> Result<RequestAnalysis, UsageError> {
        if matches!((filter.from_ms, filter.to_ms), (Some(from), Some(to)) if from >= to) {
            return Err(UsageError::InvalidInput(
                "request range must be increasing".into(),
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let comparison_filter = AnalyticsFilter {
            client: None,
            ..*filter
        };
        let summary = self.request_analysis_summary(filter)?;
        let comparison = self.request_analysis_summary(&comparison_filter)?;
        let clients = self.request_analysis_groups(
            "s.client",
            &comparison_filter,
            comparison.totals.sums.total_tokens,
            None,
        )?;
        let models = self.request_analysis_groups(
            "s.model",
            filter,
            summary.totals.sums.total_tokens,
            Some(8),
        )?;
        let projects = self.request_analysis_groups(
            "s.project_path",
            filter,
            summary.totals.sums.total_tokens,
            Some(8),
        )?;
        let coverage = self.request_coverage(filter)?;
        transaction.commit()?;
        Ok(RequestAnalysis {
            summary,
            clients,
            models,
            projects,
            coverage,
        })
    }

    fn request_analysis_summary(
        &self,
        filter: &AnalyticsFilter<'_>,
    ) -> Result<RequestAnalysisMetrics, UsageError> {
        Ok(self.conn.query_row(
            &format!("SELECT {REQUEST_COLUMNS}, {ANALYSIS_COLUMNS} {REQUEST_FROM} {REQUEST_WHERE}"),
            rusqlite::params![
                filter.from_ms,
                filter.to_ms,
                filter.client.map(Client::as_str),
                filter.project_path
            ],
            |row| read_analysis(row, 0),
        )?)
    }

    fn request_analysis_groups(
        &self,
        column: &str,
        filter: &AnalyticsFilter<'_>,
        total: i64,
        limit: Option<u32>,
    ) -> Result<Vec<RequestAnalysisGroup>, UsageError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {column}, {REQUEST_COLUMNS}, {ANALYSIS_COLUMNS} {REQUEST_FROM} {REQUEST_WHERE}
             GROUP BY {column} ORDER BY COUNT(m.total_tokens) = 0, SUM(m.total_tokens) DESC, COUNT(*) DESC, {column}
             LIMIT ?5"
        ))?;
        let rows = statement.query_map(
            rusqlite::params![
                filter.from_ms,
                filter.to_ms,
                filter.client.map(Client::as_str),
                filter.project_path,
                limit.map(i64::from).unwrap_or(-1)
            ],
            |row| {
                let mut metrics = read_analysis(row, 1)?;
                if metrics.totals.requests_with_usage > 0 {
                    metrics.share_bps =
                        ratio_bps(metrics.totals.sums.total_tokens.into(), total.into());
                }
                Ok(RequestAnalysisGroup {
                    key: row.get(0)?,
                    metrics,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Only recorded request timestamps locate usage in the half-open range.
    /// Coverage ignores dates, while the supplied display boundaries may be truncated.
    pub fn request_activity(
        &self,
        filter: &AnalyticsFilter<'_>,
        day_boundaries: &[i64],
    ) -> Result<RequestActivity, UsageError> {
        if day_boundaries.len() == 1
            || day_boundaries.len() > 401
            || day_boundaries.windows(2).any(|pair| pair[0] >= pair[1])
            || matches!((filter.from_ms, filter.to_ms), (Some(from), Some(to)) if from >= to)
        {
            return Err(UsageError::InvalidInput(
                "request range and daily boundaries must be increasing; at most 400 days".into(),
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let client = filter.client.map(Client::as_str);
        let params = rusqlite::params![filter.from_ms, filter.to_ms, client, filter.project_path];
        let range = self.conn.query_row(
            &format!("SELECT {REQUEST_COLUMNS} {REQUEST_FROM} {REQUEST_WHERE}"),
            params,
            |row| read_totals(row, 0),
        )?;
        let coverage = self.request_coverage(filter)?;
        let clients =
            self.request_analysis_groups("s.client", filter, range.sums.total_tokens, None)?;
        let mut daily: Vec<_> = day_boundaries
            .windows(2)
            .map(|pair| RequestDailyBucket {
                day_start_ms: pair[0],
                totals: RequestTotals::default(),
                clients: Vec::new(),
            })
            .collect();
        if !daily.is_empty() {
            // Integers only: no raw user strings enter SQL. Local-day lengths are caller supplied.
            let values = day_boundaries
                .windows(2)
                .enumerate()
                .map(|(index, pair)| format!("({index},{},{})", pair[0], pair[1]))
                .collect::<Vec<_>>()
                .join(",");
            let prefix = format!("WITH days(idx,start_ms,end_ms) AS (VALUES {values})");
            let join =
                "JOIN days b ON m.created_at_ms >= b.start_ms AND m.created_at_ms < b.end_ms";
            let sql = format!(
                "{prefix} SELECT b.idx, {REQUEST_COLUMNS} {REQUEST_FROM} {join} {REQUEST_WHERE} GROUP BY b.idx ORDER BY b.idx"
            );
            let mut statement = self.conn.prepare(&sql)?;
            let rows = statement.query_map(params, |row| {
                Ok((row.get::<_, u32>(0)? as usize, read_totals(row, 1)?))
            })?;
            for row in rows {
                let (index, totals) = row?;
                daily[index].totals = totals;
            }
            let sql = format!(
                "{prefix} SELECT b.idx, s.client, {REQUEST_COLUMNS} {REQUEST_FROM} {join} {REQUEST_WHERE} GROUP BY b.idx,s.client ORDER BY b.idx,s.client"
            );
            let mut statement = self.conn.prepare(&sql)?;
            let rows = statement.query_map(params, |row| {
                Ok((
                    row.get::<_, u32>(0)? as usize,
                    row.get::<_, String>(1)?,
                    read_totals(row, 2)?,
                ))
            })?;
            for row in rows {
                let (index, client, totals) = row?;
                if let Some(client) = Client::parse(&client) {
                    daily[index]
                        .clients
                        .push(RequestDailyClient { client, totals });
                }
            }
        }
        transaction.commit()?;
        Ok(RequestActivity {
            range,
            clients,
            daily,
            coverage,
        })
    }

    fn request_coverage(
        &self,
        filter: &AnalyticsFilter<'_>,
    ) -> Result<RequestCoverage, UsageError> {
        let client = filter.client.map(Client::as_str);
        let mut coverage = self.conn.query_row(
            "SELECT COUNT(*),
             COALESCE(SUM(m.source_kind IN ('measured','derived') AND m.created_at_ms IS NOT NULL),0),
             COALESCE(SUM(m.source_kind IN ('measured','derived') AND m.created_at_ms IS NULL),0),
             COALESCE(SUM(m.source_kind = 'estimated'),0)
             FROM messages m JOIN sessions s ON s.id = m.session_id
             WHERE (?1 IS NULL OR s.client = ?1) AND (?2 IS NULL OR s.project_path = ?2)",
            rusqlite::params![client, filter.project_path],
            |row| Ok(RequestCoverage {
                recorded_requests: row.get(0)?, timed_requests: row.get(1)?,
                untimed_requests: row.get(2)?, estimated_requests: row.get(3)?,
                ..RequestCoverage::default()
            }),
        )?;
        let (partial, no_requests) = self.conn.query_row(
            "SELECT COALESCE(SUM(s.message_coverage = 'partial'),0),
             COALESCE(SUM(NOT EXISTS (SELECT 1 FROM messages m WHERE m.session_id = s.id)),0)
             FROM sessions s
             WHERE (?1 IS NULL OR s.client = ?1) AND (?2 IS NULL OR s.project_path = ?2)",
            rusqlite::params![client, filter.project_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        coverage.sessions_partial = partial;
        coverage.sessions_without_requests = no_requests;
        Ok(coverage)
    }
}

#[cfg(test)]
#[path = "request-tests.rs"]
mod tests;
