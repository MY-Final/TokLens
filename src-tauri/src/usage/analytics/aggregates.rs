//! Range, daily and client aggregates.

use super::sql::{SUM_COLUMNS, read_sums};
use super::{AnalyticsFilter, ClientUsage, DailyBucket, RangeTotals};
use crate::usage::client::Client;
use crate::usage::store::{UsageError, UsageStore};

impl UsageStore {
    /// 区间统计：会话数、明细数、token 分桶之和，以及"无用量/明细不完整"的会话数。
    pub fn range_totals(&self, filter: &AnalyticsFilter<'_>) -> Result<RangeTotals, UsageError> {
        let client = filter.client.map(Client::as_str);
        let sql = format!(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN total_tokens IS NULL THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN message_coverage = 'partial' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN client_total_mismatch = 1 THEN 1 ELSE 0 END), 0),
                    {SUM_COLUMNS}
             FROM sessions
             WHERE (?1 IS NULL OR started_at_ms >= ?1)
               AND (?2 IS NULL OR started_at_ms < ?2)
               AND (?3 IS NULL OR client = ?3)
               AND (?4 IS NULL OR project_path = ?4)"
        );
        let mut totals = self.conn.query_row(
            &sql,
            rusqlite::params![filter.from_ms, filter.to_ms, client, filter.project_path],
            |row| {
                Ok(RangeTotals {
                    sessions: row.get(0)?,
                    sessions_without_usage: row.get(1)?,
                    sessions_partial: row.get(2)?,
                    sessions_total_mismatch: row.get(3)?,
                    sums: read_sums(row, 4)?,
                    messages: 0,
                })
            },
        )?;
        totals.messages = self.count_messages(filter)?;
        Ok(totals)
    }

    fn count_messages(&self, filter: &AnalyticsFilter<'_>) -> Result<i64, UsageError> {
        let client = filter.client.map(Client::as_str);
        Ok(self.conn.query_row(
            "SELECT COUNT(*)
             FROM messages AS m
             JOIN sessions AS s ON s.id = m.session_id
             WHERE (?1 IS NULL OR s.started_at_ms >= ?1)
               AND (?2 IS NULL OR s.started_at_ms < ?2)
               AND (?3 IS NULL OR s.client = ?3)
               AND (?4 IS NULL OR s.project_path = ?4)",
            rusqlite::params![filter.from_ms, filter.to_ms, client, filter.project_path],
            |row| row.get(0),
        )?)
    }

    /// 每日趋势：按 `day_start_ms` 起、每 24 小时一个桶，缺失的日期补 0。
    ///
    /// `day_start_ms` 由调用方按本地日历给出（本地零点），因此桶边界与用户看到的日期一致
    /// （不处理夏令时切换；`docs/data-model.md` 已记录该限制）。
    pub fn daily_token_trend(
        &self,
        day_start_ms: i64,
        day_count: u32,
        client: Option<Client>,
    ) -> Result<Vec<DailyBucket>, UsageError> {
        const DAY_MS: i64 = 86_400_000;
        let mut buckets: Vec<DailyBucket> = (0..day_count as i64)
            .map(|index| DailyBucket {
                day_start_ms: day_start_ms + index * DAY_MS,
                ..DailyBucket::default()
            })
            .collect();
        if day_count == 0 {
            return Ok(buckets);
        }
        let end_ms = day_start_ms + i64::from(day_count) * DAY_MS;
        let sql = format!(
            "SELECT CAST((started_at_ms - ?1) / {DAY_MS} AS INTEGER) AS day_index,
                    COUNT(*),
                    SUM(CASE WHEN total_tokens IS NULL THEN 1 ELSE 0 END),
                    SUM(CASE WHEN message_coverage = 'partial' THEN 1 ELSE 0 END),
                    {SUM_COLUMNS}
             FROM sessions AS s
             WHERE started_at_ms IS NOT NULL
               AND started_at_ms >= ?1
               AND started_at_ms < ?2
               AND (?3 IS NULL OR client = ?3)
             GROUP BY day_index
             ORDER BY day_index"
        );
        let client = client.map(Client::as_str);
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params![day_start_ms, end_ms, client], |row| {
            let index: i64 = row.get(0)?;
            Ok((
                index,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                read_sums(row, 4)?,
            ))
        })?;
        let mut message_counts = std::collections::HashMap::new();
        for row in rows {
            let (index, sessions, sessions_without_usage, sessions_partial, sums) = row?;
            if let Some(bucket) = buckets.get_mut(index.max(0) as usize) {
                bucket.sessions = sessions;
                bucket.sessions_without_usage = sessions_without_usage;
                bucket.sessions_partial = sessions_partial;
                bucket.sums = sums;
            }
        }
        // 明细数按会话所属日期归类。
        let mut statement = self.conn.prepare(
            "SELECT CAST((s.started_at_ms - ?1) / 86400000 AS INTEGER) AS day_index, COUNT(*)
             FROM messages AS m
             JOIN sessions AS s ON s.id = m.session_id
             WHERE s.started_at_ms IS NOT NULL AND s.started_at_ms >= ?1 AND s.started_at_ms < ?2
               AND (?3 IS NULL OR s.client = ?3)
             GROUP BY day_index",
        )?;
        let rows = statement.query_map(rusqlite::params![day_start_ms, end_ms, client], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (index, count) = row?;
            message_counts.insert(index, count);
        }
        for (index, count) in message_counts {
            if let Some(bucket) = buckets.get_mut(index.max(0) as usize) {
                bucket.messages = count;
            }
        }
        Ok(buckets)
    }

    /// 按客户端聚合（只返回库中真实存在的客户端记录）。
    pub fn client_usage(
        &self,
        filter: &AnalyticsFilter<'_>,
    ) -> Result<Vec<ClientUsage>, UsageError> {
        let sql = format!(
            "SELECT client, COUNT(*), {SUM_COLUMNS}
             FROM sessions
             WHERE (?1 IS NULL OR started_at_ms >= ?1)
               AND (?2 IS NULL OR started_at_ms < ?2)
             GROUP BY client
             ORDER BY client"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params![filter.from_ms, filter.to_ms], |row| {
            let client: String = row.get(0)?;
            Ok((client, row.get::<_, i64>(1)?, read_sums(row, 2)?))
        })?;
        let mut usage = Vec::new();
        for row in rows {
            let (client, sessions, sums) = row?;
            let Some(parsed) = Client::parse(&client) else {
                continue;
            };
            usage.push(ClientUsage {
                share_bps: 0,
                client: parsed,
                sessions,
                messages: self.count_messages(&AnalyticsFilter {
                    client: Some(parsed),
                    ..*filter
                })?,
                sums,
            });
        }
        let total: i128 = usage
            .iter()
            .map(|entry| i128::from(entry.sums.total_tokens))
            .sum();
        if total > 0 {
            for entry in &mut usage {
                entry.share_bps = (i128::from(entry.sums.total_tokens) * 10_000 / total) as i64;
            }
        }
        Ok(usage)
    }
}
