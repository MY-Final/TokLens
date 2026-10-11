//! 统计查询：全部基于 `sessions`（总量唯一事实来源）与 `messages`（明细）表。
//!
//! 口径约定（与 `docs/data-model.md` 一致）：
//!
//! - 所有 token 数值来自归一化后的**互不重叠分桶**，求和不会重复计入子集字段；
//! - `cache_write_tokens` 是独立上报值，不计入 `total_tokens`；
//! - 时间范围按会话的 `started_at_ms` 过滤（毫秒，UTC）；
//! - 缺失值保持 `None`/`NULL`，聚合时不被当成 0 参与"总量"，只会体现为"无用量会话数"。

use crate::usage::store::{UsageError, UsageStore};
use crate::usage::tokens::Client;

/// 查询过滤条件。
#[derive(Debug, Clone, Copy, Default)]
pub struct AnalyticsFilter<'a> {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub client: Option<Client>,
    /// 项目路径精确匹配（用于按项目下钻）。
    pub project_path: Option<&'a str>,
}

/// 聚合结果（token 分桶之和）。缺失值不参与求和，单独由 `sessions_without_usage` 统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenSums {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
}

/// 一段区间内的统计快照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RangeTotals {
    pub sessions: i64,
    pub messages: i64,
    pub sums: TokenSums,
    /// 完全没有用量数据的会话数（`total_tokens` 为 NULL）。
    pub sessions_without_usage: i64,
    /// 请求明细不完整的会话数（`message_coverage = 'partial'`）。
    pub sessions_partial: i64,
    /// 客户端自报总量与其分项之和不一致的会话数（已按客户端值计入并标记）。
    pub sessions_total_mismatch: i64,
}

/// 单日聚合。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DailyBucket {
    pub day_start_ms: i64,
    pub sessions: i64,
    pub messages: i64,
    pub sums: TokenSums,
}

/// 按客户端聚合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientUsage {
    pub client: Client,
    pub sessions: i64,
    pub messages: i64,
    pub sums: TokenSums,
}

/// 分组排行（模型 / 项目）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupUsage {
    /// 分组键；`None` 表示客户端未记录该字段（UI 需显示为"未知"）。
    pub key: Option<String>,
    pub sessions: i64,
    pub messages: i64,
    pub total_tokens: i64,
}

/// 会话摘要（不含任何正文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub client: Client,
    pub client_session_id: String,
    pub model: Option<String>,
    pub project_path: Option<String>,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub messages: i64,
    pub coverage: String,
    pub total_basis: String,
    pub unknown_relations: Vec<String>,
}

/// 单个数据源（客户端）的状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSourceStatus {
    pub client: Client,
    pub sessions: i64,
    pub messages: i64,
    pub files_tracked: i64,
    pub unparsed_records: i64,
    pub last_scan_ms: Option<i64>,
}

const SUM_COLUMNS: &str = "COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), \
     COALESCE(SUM(reasoning_tokens), 0), COALESCE(SUM(cache_read_tokens), 0), \
     COALESCE(SUM(cache_write_tokens), 0), COALESCE(SUM(total_tokens), 0)";

fn read_sums(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<TokenSums> {
    Ok(TokenSums {
        input_tokens: row.get(offset)?,
        output_tokens: row.get(offset + 1)?,
        reasoning_tokens: row.get(offset + 2)?,
        cache_read_tokens: row.get(offset + 3)?,
        cache_write_tokens: row.get(offset + 4)?,
        total_tokens: row.get(offset + 5)?,
    })
}

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
            Ok((index, row.get::<_, i64>(1)?, read_sums(row, 2)?))
        })?;
        let mut message_counts = std::collections::HashMap::new();
        for row in rows {
            let (index, sessions, sums) = row?;
            if let Some(bucket) = buckets.get_mut(index.max(0) as usize) {
                bucket.sessions = sessions;
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
                client: parsed,
                sessions,
                messages: self.count_messages(&AnalyticsFilter {
                    client: Some(parsed),
                    ..*filter
                })?,
                sums,
            });
        }
        Ok(usage)
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::model::{MessageRecord, SessionRecord, parse_iso8601_utc_ms};
    use crate::usage::normalize::{
        ClaudeSessionRollup, CodexThreadSnapshot, CodexUsageRecord, claude_code_session,
        codex_message, codex_session,
    };
    use crate::usage::store::{SourceFile, SourceFileKind};
    use crate::usage::tokens::RawTokenCounts;

    const D0: &str = "2026-10-05T00:00:00.000Z";
    const D1: &str = "2026-10-06T09:00:00.000Z";
    const D2: &str = "2026-10-07T20:00:00.000Z";
    const PROJECT_ALPHA: &str = "D:\\Repos\\alpha";
    const PROJECT_BETA: &str = "D:\\Repos\\beta";

    fn ms(value: &str) -> i64 {
        parse_iso8601_utc_ms(value).expect("test timestamp")
    }

    fn counts(
        input: Option<u64>,
        output: Option<u64>,
        cache_read: Option<u64>,
        cache_write: Option<u64>,
        reasoning: Option<u64>,
        client_total: Option<u64>,
    ) -> RawTokenCounts {
        RawTokenCounts {
            input,
            output,
            cache_read,
            cache_write,
            reasoning,
            client_total,
        }
    }

    fn codex_record(
        thread: &str,
        started: &str,
        ended: &str,
        model: Option<&str>,
        project: Option<&str>,
        cumulative: RawTokenCounts,
    ) -> SessionRecord {
        codex_session(&CodexThreadSnapshot {
            thread_id: thread,
            parent_thread_id: None,
            cwd: project,
            model,
            started_at: Some(started),
            ended_at: Some(ended),
            first_ordinal: Some(0),
            message_count: 1,
            cumulative,
        })
    }

    fn claude_record(started: &str, project: &str, model: &str) -> SessionRecord {
        claude_code_session(&ClaudeSessionRollup {
            session_id: "session-d",
            cwd: Some(project),
            model: Some(model),
            started_at: Some(started),
            ended_at: Some(started),
            message_count: 1,
            usage: counts(Some(500), Some(50), Some(100), Some(0), None, None),
        })
    }

    fn message(thread: &str, response: &str, ordinal: i64, at: &str) -> MessageRecord {
        codex_message(&CodexUsageRecord {
            thread_id: thread,
            response_id: Some(response),
            ordinal,
            timestamp: Some(at),
            usage: counts(Some(10), Some(2), Some(0), Some(0), Some(0), Some(12)),
        })
    }

    fn source(client: Client, path: &str) -> SourceFile {
        SourceFile {
            client,
            path: path.to_string(),
            kind: SourceFileKind::Jsonl,
            size_bytes: Some(1_024),
            mtime_ms: Some(1_791_000_000_000),
            content_offset: 1_024,
            parser_version: 1,
            last_scan_ms: Some(1_791_000_000_500),
            unparsed_records: 0,
        }
    }

    /// 4 个会话：3 个 Codex（跨 3 天，其中一个完全没有用量数据）+ 1 个 Claude Code。
    fn seeded_store() -> UsageStore {
        let store = UsageStore::open_in_memory().expect("store");
        store
            .import_session(
                &source(Client::Codex, "C:\\sessions\\a.jsonl"),
                &codex_record(
                    "thread-a",
                    D0,
                    D0,
                    Some("gpt-5.6-luna"),
                    Some(PROJECT_ALPHA),
                    counts(
                        Some(1_000),
                        Some(100),
                        Some(400),
                        Some(5),
                        Some(20),
                        Some(1_100),
                    ),
                ),
                &[
                    message("thread-a", "resp-a1", 1, D0),
                    message("thread-a", "resp-a2", 2, D0),
                ],
            )
            .expect("import a");
        store
            .import_session(
                &source(Client::Codex, "C:\\sessions\\b.jsonl"),
                &codex_record(
                    "thread-b",
                    D1,
                    D1,
                    Some("gpt-6-astra"),
                    Some(PROJECT_BETA),
                    counts(
                        Some(2_000),
                        Some(300),
                        Some(0),
                        Some(0),
                        Some(0),
                        Some(2_300),
                    ),
                ),
                &[message("thread-b", "resp-b1", 1, D1)],
            )
            .expect("import b");
        store
            .import_session(
                &source(Client::Codex, "C:\\sessions\\c.jsonl"),
                &codex_record("thread-c", D2, D2, None, None, RawTokenCounts::default()),
                &[],
            )
            .expect("import c");
        store
            .import_session(
                &source(Client::ClaudeCode, "C:\\claude\\d.jsonl"),
                &claude_record(D1, PROJECT_ALPHA, "claude-sonnet-5"),
                &[],
            )
            .expect("import d");
        store
    }

    #[test]
    fn range_totals_respect_range_and_client_filters() {
        let store = seeded_store();
        let all = store
            .range_totals(&AnalyticsFilter::default())
            .expect("all totals");
        assert_eq!(all.sessions, 4);
        assert_eq!(all.messages, 3);
        assert_eq!(
            all.sessions_total_mismatch, 0,
            "测试数据的分项之和与客户端总量一致"
        );
        assert_eq!(all.sessions_without_usage, 1, "没有用量数据的会话单独计数");
        assert_eq!(all.sums.input_tokens, 600 + 2_000 + 500);
        assert_eq!(all.sums.output_tokens, 80 + 300 + 50);
        assert_eq!(all.sums.reasoning_tokens, 20);
        assert_eq!(all.sums.cache_read_tokens, 500);
        assert_eq!(all.sums.cache_write_tokens, 5);
        assert_eq!(all.sums.total_tokens, 1_100 + 2_300 + 650);

        let codex_only = store
            .range_totals(&AnalyticsFilter {
                client: Some(Client::Codex),
                ..AnalyticsFilter::default()
            })
            .expect("codex totals");
        assert_eq!(codex_only.sessions, 3);
        assert_eq!(codex_only.messages, 3);
        assert_eq!(codex_only.sums.total_tokens, 1_100 + 2_300);

        let last_day = store
            .range_totals(&AnalyticsFilter {
                from_ms: Some(ms("2026-10-07T00:00:00.000Z")),
                ..AnalyticsFilter::default()
            })
            .expect("range totals");
        assert_eq!(last_day.sessions, 1);
        assert_eq!(last_day.sums.total_tokens, 0);
        assert_eq!(last_day.sessions_without_usage, 1);
    }

    #[test]
    fn daily_trend_buckets_by_session_start_and_fills_gaps() {
        let store = seeded_store();
        let buckets = store
            .daily_token_trend(ms("2026-10-05T00:00:00.000Z"), 4, Some(Client::Codex))
            .expect("trend");
        assert_eq!(buckets.len(), 4, "缺失日期补 0");
        assert_eq!(buckets[0].day_start_ms, ms("2026-10-05T00:00:00.000Z"));
        assert_eq!(buckets[0].sessions, 1);
        assert_eq!(buckets[0].messages, 2);
        assert_eq!(buckets[0].sums.total_tokens, 1_100);
        assert_eq!(buckets[1].sessions, 1);
        assert_eq!(buckets[1].sums.total_tokens, 2_300);
        assert_eq!(buckets[2].sessions, 1);
        assert_eq!(buckets[2].sums.total_tokens, 0);
        assert_eq!(buckets[3].sessions, 0);
        assert_eq!(buckets[3].sums, TokenSums::default());
    }

    #[test]
    fn client_usage_groups_by_client() {
        let store = seeded_store();
        let usage = store
            .client_usage(&AnalyticsFilter::default())
            .expect("usage");
        assert_eq!(usage.len(), 2, "只返回库中真实存在的客户端");
        let codex = usage
            .iter()
            .find(|entry| entry.client == Client::Codex)
            .expect("codex usage");
        assert_eq!(codex.sessions, 3);
        assert_eq!(codex.messages, 3);
        assert_eq!(codex.sums.total_tokens, 3_400);
        let claude = usage
            .iter()
            .find(|entry| entry.client == Client::ClaudeCode)
            .expect("claude usage");
        assert_eq!(claude.sums.total_tokens, 650);
    }

    #[test]
    fn rankings_keep_unknown_groups_visible() {
        let store = seeded_store();
        let models = store
            .model_ranking(&AnalyticsFilter::default(), 10)
            .expect("models");
        assert_eq!(models[0].key.as_deref(), Some("gpt-6-astra"));
        assert_eq!(models[0].total_tokens, 2_300);
        assert_eq!(models[1].key.as_deref(), Some("gpt-5.6-luna"));
        let unknown = models
            .iter()
            .find(|entry| entry.key.is_none())
            .expect("unknown model stays visible");
        assert_eq!(unknown.sessions, 1);

        let projects = store
            .project_ranking(&AnalyticsFilter::default(), 10)
            .expect("projects");
        assert_eq!(projects[0].key.as_deref(), Some(PROJECT_BETA));
        assert_eq!(projects[0].total_tokens, 2_300);
        let alpha = projects
            .iter()
            .find(|entry| entry.key.as_deref() == Some(PROJECT_ALPHA))
            .expect("alpha");
        assert_eq!(alpha.sessions, 2, "跨客户端按项目聚合");
        assert_eq!(alpha.messages, 2);
    }

    #[test]
    fn session_summaries_are_newest_first_with_coverage_metadata() {
        let store = seeded_store();
        let sessions = store
            .session_summaries(&AnalyticsFilter::default(), 2, 0)
            .expect("sessions");
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].client_session_id, "thread-c");
        assert_eq!(sessions[0].total_tokens, None, "无用量会话保持 NULL");
        assert_eq!(sessions[0].messages, 0);
        assert_eq!(sessions[1].client_session_id, "thread-b");

        let page_two = store
            .session_summaries(&AnalyticsFilter::default(), 2, 2)
            .expect("sessions page 2");
        assert_eq!(page_two.len(), 2);
        let codex_a = page_two
            .iter()
            .find(|session| session.client_session_id == "thread-a")
            .expect("thread-a");
        assert_eq!(codex_a.coverage, "complete");
        assert_eq!(codex_a.total_basis, "client_cumulative");
        assert_eq!(codex_a.input_tokens, Some(600), "分桶已扣除缓存子集");
        assert_eq!(codex_a.cache_write_tokens, Some(5));
        assert_eq!(
            codex_a.unknown_relations,
            vec!["cache_write.relation_to_input".to_string()]
        );
    }

    #[test]
    fn totals_flag_sessions_whose_client_total_disagrees_with_its_buckets() {
        let store = UsageStore::open_in_memory().expect("store");
        // 客户端自报总量 (2_000) 与 input + output (1_100) 不一致：保留客户端值并计数。
        store
            .import_session(
                &source(Client::Codex, "C:\\sessions\\m.jsonl"),
                &codex_record(
                    "thread-m",
                    D0,
                    D0,
                    Some("gpt-5.6-luna"),
                    Some(PROJECT_ALPHA),
                    counts(
                        Some(1_000),
                        Some(100),
                        Some(0),
                        Some(0),
                        Some(0),
                        Some(2_000),
                    ),
                ),
                &[],
            )
            .expect("import mismatch");
        let totals = store
            .range_totals(&AnalyticsFilter::default())
            .expect("totals");
        assert_eq!(totals.sessions_total_mismatch, 1);
        assert_eq!(totals.sums.total_tokens, 2_000, "总量取客户端值");
        assert_eq!(
            totals.sums.input_tokens + totals.sums.output_tokens,
            1_100,
            "分桶按客户端分项计算"
        );
    }
    #[test]
    fn source_status_and_bounds_reflect_imported_files() {
        let store = seeded_store();
        let status = store.client_source_status().expect("status");
        let codex = status
            .iter()
            .find(|entry| entry.client == Client::Codex)
            .expect("codex");
        assert_eq!(codex.sessions, 3);
        assert_eq!(codex.files_tracked, 3);
        assert_eq!(codex.unparsed_records, 0);
        assert_eq!(codex.last_scan_ms, Some(1_791_000_000_500));
        let opencode = status
            .iter()
            .find(|entry| entry.client == Client::OpenCode)
            .expect("opencode");
        assert_eq!(opencode.sessions, 0);
        assert_eq!(opencode.files_tracked, 0);

        let (first, last) = store.data_bounds().expect("bounds");
        assert_eq!(first, Some(ms(D0)));
        assert_eq!(last, Some(ms(D2)));
    }
}
