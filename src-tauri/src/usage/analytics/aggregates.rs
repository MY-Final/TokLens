//! Range, daily and client aggregates.

use super::sql::{SUM_COLUMNS, read_sums};
use super::{AnalyticsFilter, ClientUsage, DailyBucket, RangeTotals};
use crate::usage::client::Client;
use crate::usage::store::{UsageError, UsageStore};

impl UsageStore {
    /// 区间统计：会话数、明细数、token 分桶之和，以及"无用量/明细不完整"的会话数。
    pub fn range_totals(&self, filter: &AnalyticsFilter<'_>) -> Result<RangeTotals, UsageError> {
        let client = filter.client.map(Client::as_str);
        // 归集口径见 `attribution.rs`：有请求明细按明细时间归集，否则按会话开始时间归集。
        let cte = super::attribution::attributed_cte(&super::attribution::AttributionParams {
            from: 1,
            to: 2,
            client: Some(3),
            project_path: Some(4),
        });
        let sql = format!(
            "{cte}
             SELECT COUNT(DISTINCT session_id),
                    COUNT(DISTINCT CASE WHEN session_total_tokens IS NULL THEN session_id END),
                    COUNT(DISTINCT CASE WHEN message_coverage = 'partial' THEN session_id END),
                    COUNT(DISTINCT CASE WHEN client_total_mismatch = 1 THEN session_id END),
                    {SUM_COLUMNS},
                    COALESCE(SUM(CASE WHEN from_request = 1 THEN 1 ELSE 0 END), 0)
             FROM attributed"
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
                    messages: row.get(11)?,
                    cache_hit_bps: None,
                    cache_eligible_sessions: 0,
                })
            },
        )?;
        let (eligible, input, cache): (i64, f64, f64) = self.conn.query_row(
            &format!(
                "{cte}
                 SELECT COUNT(DISTINCT session_id), COALESCE(TOTAL(input_tokens), 0), COALESCE(TOTAL(cache_read_tokens), 0)
                 FROM attributed
                 WHERE input_tokens IS NOT NULL AND cache_read_tokens IS NOT NULL"
            ),
            rusqlite::params![filter.from_ms, filter.to_ms, client, filter.project_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        totals.cache_eligible_sessions = eligible;
        totals.cache_hit_bps =
            (input + cache > 0.0).then(|| (cache * 10_000.0 / (input + cache)) as i64);
        Ok(totals)
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
        self.daily_token_trend_in_range(day_start_ms, day_count, None, None, client)
    }

    /// 桶按归集时间划分；精确区间过滤与桶起点相互独立。
    pub fn daily_token_trend_in_range(
        &self,
        day_start_ms: i64,
        day_count: u32,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
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
        let cte = super::attribution::attributed_cte(&super::attribution::AttributionParams {
            from: 4,
            to: 5,
            client: Some(3),
            project_path: None,
        });
        let sql = format!(
            "{cte}
             SELECT CAST((at_ms - ?1) / {DAY_MS} AS INTEGER) AS day_index,
                    COUNT(DISTINCT session_id),
                    COUNT(DISTINCT CASE WHEN session_total_tokens IS NULL THEN session_id END),
                    COUNT(DISTINCT CASE WHEN message_coverage = 'partial' THEN session_id END),
                    {SUM_COLUMNS},
                    COALESCE(SUM(CASE WHEN from_request = 1 THEN 1 ELSE 0 END), 0)
             FROM attributed
             WHERE at_ms >= ?1 AND at_ms < ?2
             GROUP BY day_index
             ORDER BY day_index"
        );
        let client = client.map(Client::as_str);
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params![day_start_ms, end_ms, client, from_ms, to_ms],
            |row| {
                let index: i64 = row.get(0)?;
                Ok((
                    index,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    read_sums(row, 4)?,
                    row.get::<_, i64>(11)?,
                ))
            },
        )?;
        for row in rows {
            let (index, sessions, sessions_without_usage, sessions_partial, sums, messages) = row?;
            if let Some(bucket) = buckets.get_mut(index.max(0) as usize) {
                bucket.sessions = sessions;
                bucket.sessions_without_usage = sessions_without_usage;
                bucket.sessions_partial = sessions_partial;
                bucket.sums = sums;
                bucket.messages = messages;
            }
        }
        Ok(buckets)
    }

    /// 按客户端聚合（只返回库中真实存在的客户端记录）。
    pub fn client_usage(
        &self,
        filter: &AnalyticsFilter<'_>,
    ) -> Result<Vec<ClientUsage>, UsageError> {
        // 归集口径见 `attribution.rs`；会话数与明细数都来自同一个归集结果，不再另起子查询。
        let cte = super::attribution::attributed_cte(&super::attribution::AttributionParams {
            from: 1,
            to: 2,
            client: None,
            project_path: Some(3),
        });
        let sql = format!(
            "{cte}
             SELECT client,
                    COUNT(DISTINCT session_id),
                    COUNT(DISTINCT CASE WHEN session_total_tokens IS NULL THEN session_id END),
                    {SUM_COLUMNS},
                    COALESCE(SUM(CASE WHEN from_request = 1 THEN 1 ELSE 0 END), 0)
             FROM attributed
             GROUP BY client
             ORDER BY client"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params![filter.from_ms, filter.to_ms, filter.project_path],
            |row| {
                let client: String = row.get(0)?;
                Ok((
                    client,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    read_sums(row, 3)?,
                    row.get::<_, i64>(10)?,
                ))
            },
        )?;
        let mut usage = Vec::new();
        for row in rows {
            let (client, sessions, sessions_without_usage, sums, messages) = row?;
            let Some(parsed) = Client::parse(&client) else {
                continue;
            };
            usage.push(ClientUsage {
                sessions_without_usage,
                share_bps: 0,
                client: parsed,
                sessions,
                messages,
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
