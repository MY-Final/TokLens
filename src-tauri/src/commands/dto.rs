//! Tauri Command 的 DTO：字段名统一 camelCase，与前端 `src/lib/usage-types.ts` 的类型逐字对应。
//!
//! - 缺失值一律用 `null`（`Option`），不用 0 代替；
//! - 只输出元数据与统计值，不含会话正文、提示词或凭据；
//! - 费用字段当前不提供（本机数据没有可靠成本来源，见 `docs/data-metrics.md`）。

use serde::{Deserialize, Serialize};

use crate::collectors::{ClientSyncResult, SyncBatch, SyncReport};
use crate::usage::analytics::{
    ClientSourceStatus, ClientUsage, DailyBucket, GroupUsage, RangeTotals, SessionSummary,
    TokenSums,
};
use crate::usage::store::UsageError;
use crate::usage::tokens::Client;

use super::SyncState;

/// 同步状态 DTO（也是事件负载）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStateDto {
    pub running: bool,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub report: Option<SyncReportDto>,
}

impl From<&SyncState> for SyncStateDto {
    fn from(state: &SyncState) -> Self {
        Self {
            running: state.running,
            started_at_ms: state.started_at_ms,
            finished_at_ms: state.finished_at_ms,
            duration_ms: state.duration_ms,
            error: state.error.clone(),
            report: state.report.clone().map(SyncReportDto::from),
        }
    }
}

/// 前端传入的客户端筛选参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientParam {
    Codex,
    ClaudeCode,
    #[serde(rename = "opencode")]
    OpenCode,
    Dsh,
    KimiCode,
}

impl From<ClientParam> for Client {
    fn from(param: ClientParam) -> Self {
        match param {
            ClientParam::Codex => Client::Codex,
            ClientParam::ClaudeCode => Client::ClaudeCode,
            ClientParam::OpenCode => Client::OpenCode,
            ClientParam::Dsh => Client::Dsh,
            ClientParam::KimiCode => Client::KimiCode,
        }
    }
}

/// 已实现本地采集器的客户端；实际发现状态由逐客户端同步结果提供。
pub fn is_supported(client: Client) -> bool {
    matches!(
        client,
        Client::Codex | Client::ClaudeCode | Client::OpenCode | Client::Dsh | Client::KimiCode
    )
}

/// token 分桶（互不重叠，可直接相加得到规范总量）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTotalsDto {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub counted_cache_write_tokens: i64,
    pub total_tokens: i64,
}

impl From<TokenSums> for TokenTotalsDto {
    fn from(sums: TokenSums) -> Self {
        Self {
            input_tokens: sums.input_tokens,
            output_tokens: sums.output_tokens,
            reasoning_tokens: sums.reasoning_tokens,
            cache_read_tokens: sums.cache_read_tokens,
            cache_write_tokens: sums.cache_write_tokens,
            counted_cache_write_tokens: sums.counted_cache_write_tokens,
            total_tokens: sums.total_tokens,
        }
    }
}

/// 一段区间（或某一天）的统计快照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsBucketDto {
    pub sessions: i64,
    pub messages: i64,
    /// 完全没有用量数据的会话数（总量的"未知"部分，不参与求和）。
    pub sessions_without_usage: i64,
    /// 请求明细不完整的会话数（源文件被分页/续接）。
    pub sessions_partial: i64,
    /// 客户端自报总量与其分项之和不一致的会话数（按客户端值计入并标记）。
    pub sessions_total_mismatch: i64,
    pub totals: TokenTotalsDto,
}

impl From<RangeTotals> for StatsBucketDto {
    fn from(totals: RangeTotals) -> Self {
        Self {
            sessions: totals.sessions,
            messages: totals.messages,
            sessions_without_usage: totals.sessions_without_usage,
            sessions_partial: totals.sessions_partial,
            sessions_total_mismatch: totals.sessions_total_mismatch,
            totals: totals.sums.into(),
        }
    }
}

/// 总览统计：所选区间 + 今日（边界由前端按本地日历给出）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewStatsDto {
    pub range: StatsBucketDto,
    pub today: StatsBucketDto,
    pub first_event_ms: Option<i64>,
    pub last_event_ms: Option<i64>,
}

/// 每日趋势的一个桶（`day_start_ms` 为该日本地零点）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyTokenBucketDto {
    pub clients: Vec<DailyClientTokensDto>,
    pub day_start_ms: i64,
    pub sessions: i64,
    pub messages: i64,
    pub totals: TokenTotalsDto,
}

impl From<DailyBucket> for DailyTokenBucketDto {
    fn from(bucket: DailyBucket) -> Self {
        Self {
            clients: Vec::new(),
            day_start_ms: bucket.day_start_ms,
            sessions: bucket.sessions,
            messages: bucket.messages,
            totals: bucket.sums.into(),
        }
    }
}

/// 客户端用量（未接入的客户端返回 0 与 `supported: false`，不混入模拟数据）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientUsageDto {
    pub share_bps: i64,
    pub client: String,
    pub supported: bool,
    pub sessions: i64,
    pub messages: i64,
    pub totals: TokenTotalsDto,
}

/// 按固定顺序（Codex / Claude Code / OpenCode / DSH / Kimi Code）输出五个客户端，缺失数据补 0。
pub fn client_usage_dto(usage: &[ClientUsage]) -> Vec<ClientUsageDto> {
    Client::ALL
        .into_iter()
        .map(|client| {
            let entry = usage.iter().find(|entry| entry.client == client);
            ClientUsageDto {
                share_bps: entry.map_or(0, |entry| entry.share_bps),
                client: client.as_str().to_string(),
                supported: is_supported(client),
                sessions: entry.map_or(0, |entry| entry.sessions),
                messages: entry.map_or(0, |entry| entry.messages),
                totals: entry.map_or_else(TokenTotalsDto::default, |entry| entry.sums.into()),
            }
        })
        .collect()
}

/// 排行条目。`key` 为 `null` 表示客户端未记录该字段（前端显示"未知"）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankingEntryDto {
    pub key: Option<String>,
    pub sessions: i64,
    pub messages: i64,
    pub total_tokens: i64,
}

impl From<GroupUsage> for RankingEntryDto {
    fn from(group: GroupUsage) -> Self {
        Self {
            key: group.key,
            sessions: group.sessions,
            messages: group.messages,
            total_tokens: group.total_tokens,
        }
    }
}

/// 模型与项目排行。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankingsDto {
    pub models: Vec<RankingEntryDto>,
    pub projects: Vec<RankingEntryDto>,
}

/// 会话摘要（无正文）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummaryDto {
    pub client: String,
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

impl From<SessionSummary> for SessionSummaryDto {
    fn from(summary: SessionSummary) -> Self {
        Self {
            client: summary.client.as_str().to_string(),
            client_session_id: summary.client_session_id,
            model: summary.model,
            project_path: summary.project_path,
            started_at_ms: summary.started_at_ms,
            ended_at_ms: summary.ended_at_ms,
            input_tokens: summary.input_tokens,
            output_tokens: summary.output_tokens,
            reasoning_tokens: summary.reasoning_tokens,
            cache_read_tokens: summary.cache_read_tokens,
            cache_write_tokens: summary.cache_write_tokens,
            total_tokens: summary.total_tokens,
            messages: summary.messages,
            coverage: summary.coverage,
            total_basis: summary.total_basis,
            unknown_relations: summary.unknown_relations,
        }
    }
}

/// 单个数据源状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSourceStatusDto {
    pub client: String,
    pub supported: bool,
    pub sessions: i64,
    pub messages: i64,
    pub files_tracked: i64,
    pub unparsed_records: i64,
    pub last_scan_ms: Option<i64>,
}

impl From<ClientSourceStatus> for ClientSourceStatusDto {
    fn from(status: ClientSourceStatus) -> Self {
        Self {
            client: status.client.as_str().to_string(),
            supported: is_supported(status.client),
            sessions: status.sessions,
            messages: status.messages,
            files_tracked: status.files_tracked,
            unparsed_records: status.unparsed_records,
            last_scan_ms: status.last_scan_ms,
        }
    }
}

/// 一次同步的结果摘要。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReportDto {
    pub clients: Vec<ClientSyncResultDto>,
    pub root_missing: bool,
    pub files_seen: i64,
    pub files_parsed: i64,
    pub files_skipped_unchanged: i64,
    pub files_without_identity: i64,
    pub files_failed: i64,
    pub unparsed_lines: i64,
    pub sessions_imported: i64,
    pub sessions_skipped_stale: i64,
    pub messages_inserted: i64,
    pub messages_updated: i64,
    pub messages_pruned: i64,
}

impl From<SyncReport> for SyncReportDto {
    fn from(report: SyncReport) -> Self {
        Self {
            clients: Vec::new(),
            root_missing: report.root_missing,
            files_seen: report.files_seen as i64,
            files_parsed: report.files_parsed as i64,
            files_skipped_unchanged: report.files_skipped_unchanged as i64,
            files_without_identity: report.files_without_identity as i64,
            files_failed: report.files_failed as i64,
            unparsed_lines: report.unparsed_lines as i64,
            sessions_imported: report.sessions_imported as i64,
            sessions_skipped_stale: report.sessions_skipped_stale as i64,
            messages_inserted: report.messages_inserted as i64,
            messages_updated: report.messages_updated as i64,
            messages_pruned: report.messages_pruned as i64,
        }
    }
}

/// 数据源状态总览。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceStatusDto {
    pub clients: Vec<ClientSourceStatusDto>,
    pub sync: SyncStateDto,
    pub database_path: String,
    pub first_event_ms: Option<i64>,
    pub last_event_ms: Option<i64>,
    /// 是否存在可靠的成本来源。当前恒为 false：不展示虚构金额。
    pub cost_available: bool,
}

/// 把存储层错误翻译成前端可读的消息。
pub fn usage_error_message(error: &UsageError) -> String {
    match error {
        UsageError::Database(_) => "本地数据库读写失败，请查看日志或重启应用".to_string(),
        UsageError::Io(_) => "本地数据库文件不可写，请检查应用数据目录权限".to_string(),
        UsageError::InvalidInput(message) => format!("请求参数无效：{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_filter_accepts_the_same_ids_returned_to_the_frontend() {
        for client in Client::ALL {
            let json = serde_json::to_string(client.as_str()).expect("client id");
            let param: ClientParam = serde_json::from_str(&json).expect("filter parameter");
            assert_eq!(Client::from(param), client);
        }
    }
    use crate::usage::analytics::{ClientUsage, TokenSums};

    #[test]
    fn client_usage_dto_keeps_fixed_order_and_marks_supported() {
        let rows = vec![
            ClientUsage {
                share_bps: 0,
                client: Client::Codex,
                sessions: 3,
                messages: 9,
                sums: TokenSums {
                    input_tokens: 100,
                    output_tokens: 10,
                    reasoning_tokens: 2,
                    cache_read_tokens: 1_000,
                    cache_write_tokens: 5,
                    counted_cache_write_tokens: 0,
                    total_tokens: 1_112,
                },
            },
            ClientUsage {
                share_bps: 0,
                client: Client::OpenCode,
                sessions: 1,
                messages: 1,
                sums: TokenSums::default(),
            },
        ];
        let dto = client_usage_dto(&rows);
        assert_eq!(dto.len(), 5);
        assert_eq!(dto[0].client, "codex");
        assert!(dto[0].supported);
        assert_eq!(dto[0].totals.total_tokens, 1_112);
        assert_eq!(dto[1].client, "claude_code");
        assert!(dto[1].supported, "采集器支持不代表已有数据");
        assert_eq!(dto[1].sessions, 0, "没有数据就是 0，不编造");
        assert_eq!(dto[2].client, "opencode");
        assert!(dto[2].supported);
    }

    #[test]
    fn sync_report_dto_maps_counters() {
        let report = SyncReport {
            files_seen: 353,
            files_parsed: 2,
            files_skipped_unchanged: 351,
            sessions_imported: 2,
            messages_inserted: 5,
            ..SyncReport::default()
        };
        let dto = SyncReportDto::from(report);
        assert_eq!(dto.files_seen, 353);
        assert_eq!(dto.files_skipped_unchanged, 351);
        assert_eq!(dto.messages_inserted, 5);
        assert_eq!(dto.unparsed_lines, 0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSyncResultDto {
    pub client: String,
    pub error: Option<String>,
    pub report: Option<SyncReportDto>,
}

impl From<ClientSyncResult> for ClientSyncResultDto {
    fn from(result: ClientSyncResult) -> Self {
        Self {
            client: result.client.as_str().to_string(),
            error: result.error,
            report: result.report.map(SyncReportDto::from),
        }
    }
}

impl From<SyncBatch> for SyncReportDto {
    fn from(batch: SyncBatch) -> Self {
        let mut summary = Self {
            root_missing: !batch.clients.is_empty(),
            ..Default::default()
        };
        for result in &batch.clients {
            if let Some(report) = &result.report {
                summary.root_missing &= report.root_missing;
                summary.files_seen += report.files_seen as i64;
                summary.files_parsed += report.files_parsed as i64;
                summary.files_skipped_unchanged += report.files_skipped_unchanged as i64;
                summary.files_without_identity += report.files_without_identity as i64;
                summary.files_failed += report.files_failed as i64;
                summary.unparsed_lines += report.unparsed_lines as i64;
                summary.sessions_imported += report.sessions_imported as i64;
                summary.sessions_skipped_stale += report.sessions_skipped_stale as i64;
                summary.messages_inserted += report.messages_inserted as i64;
                summary.messages_updated += report.messages_updated as i64;
                summary.messages_pruned += report.messages_pruned as i64;
            } else {
                summary.root_missing = false;
            }
        }
        summary.clients = batch
            .clients
            .into_iter()
            .map(ClientSyncResultDto::from)
            .collect();
        summary
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyClientTokensDto {
    pub client: String,
    pub total_tokens: i64,
}
