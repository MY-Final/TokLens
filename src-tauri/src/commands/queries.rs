//! 只读查询命令：全部走 `usage::analytics` 的聚合，命令层不做口径计算。

use tauri::State;

use super::dto::{
    ClientParam, ClientSourceStatusDto, DailyTokenBucketDto, DataSourceStatusDto, OverviewStatsDto,
    RankingEntryDto, RankingsDto, SessionSummaryDto, client_usage_dto,
};
use super::{AppState, CommandError, SyncStateDto};
use crate::usage::analytics::AnalyticsFilter;
use crate::usage::tokens::Client;

const DEFAULT_RANKING_LIMIT: u32 = 8;
const MAX_RANKING_LIMIT: u32 = 50;
const DEFAULT_SESSION_LIMIT: u32 = 20;
const MAX_SESSION_LIMIT: u32 = 500;

fn filter(
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    client: Option<ClientParam>,
) -> AnalyticsFilter<'static> {
    AnalyticsFilter {
        from_ms,
        to_ms,
        client: client.map(Client::from),
        project_path: None,
    }
}

/// 总览统计：所选区间 + 今日（`today_from_ms` 为前端按本地日历给出的今日零点）。
#[tauri::command(rename_all = "camelCase")]
pub fn get_overview_stats(
    state: State<'_, AppState>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    today_from_ms: Option<i64>,
    client: Option<ClientParam>,
) -> Result<OverviewStatsDto, CommandError> {
    let store = state.store()?;
    let range = store.range_totals(&filter(from_ms, to_ms, client))?;
    let today = match today_from_ms {
        Some(from) => store.range_totals(&filter(Some(from), None, client))?,
        None => Default::default(),
    };
    let (first_event_ms, last_event_ms) = store.data_bounds()?;
    Ok(OverviewStatsDto {
        range: range.into(),
        today: today.into(),
        first_event_ms,
        last_event_ms,
    })
}

/// 每日 Token 趋势：`day_count` 天，从 `from_ms`（本地零点）开始，缺失日期补 0。
#[tauri::command(rename_all = "camelCase")]
pub fn get_daily_token_trend(
    state: State<'_, AppState>,
    from_ms: i64,
    day_count: u32,
    client: Option<ClientParam>,
) -> Result<Vec<DailyTokenBucketDto>, CommandError> {
    let store = state.store()?;
    let day_count = day_count.clamp(1, 400);
    let buckets = store.daily_token_trend(from_ms, day_count, client.map(Client::from))?;
    let mut dtos: Vec<_> = buckets.into_iter().map(DailyTokenBucketDto::from).collect();
    for entry in Client::ALL {
        if client.is_some_and(|filter| Client::from(filter) != entry) {
            continue;
        }
        let daily = store.daily_token_trend(from_ms, day_count, Some(entry))?;
        for (dto, bucket) in dtos.iter_mut().zip(daily) {
            dto.clients.push(super::dto::DailyClientTokensDto {
                client: entry.as_str().to_string(),
                total_tokens: bucket.sums.total_tokens,
            });
        }
    }
    Ok(dtos)
}

/// 客户端用量分布：固定返回七个客户端，空数据保持为 0，占比由领域查询提供。
#[tauri::command(rename_all = "camelCase")]
pub fn get_client_usage(
    state: State<'_, AppState>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
) -> Result<Vec<super::dto::ClientUsageDto>, CommandError> {
    let store = state.store()?;
    let usage = store.client_usage(&filter(from_ms, to_ms, None))?;
    Ok(client_usage_dto(&usage))
}

/// 模型与项目排行。
#[tauri::command(rename_all = "camelCase")]
pub fn get_rankings(
    state: State<'_, AppState>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    client: Option<ClientParam>,
    limit: Option<u32>,
) -> Result<RankingsDto, CommandError> {
    let store = state.store()?;
    let limit = limit
        .unwrap_or(DEFAULT_RANKING_LIMIT)
        .clamp(1, MAX_RANKING_LIMIT);
    let filter = filter(from_ms, to_ms, client);
    Ok(RankingsDto {
        models: store
            .model_ranking(&filter, limit)?
            .into_iter()
            .map(RankingEntryDto::from)
            .collect(),
        projects: store
            .project_ranking(&filter, limit)?
            .into_iter()
            .map(RankingEntryDto::from)
            .collect(),
    })
}

/// 会话列表（最近优先）。只返回元数据与用量，不含正文。
#[tauri::command(rename_all = "camelCase")]
pub fn get_sessions(
    state: State<'_, AppState>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    client: Option<ClientParam>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<SessionSummaryDto>, CommandError> {
    let store = state.store()?;
    let limit = limit
        .unwrap_or(DEFAULT_SESSION_LIMIT)
        .clamp(1, MAX_SESSION_LIMIT);
    let offset = offset.unwrap_or(0);
    let sessions = store.session_summaries(&filter(from_ms, to_ms, client), limit, offset)?;
    Ok(sessions.into_iter().map(SessionSummaryDto::from).collect())
}

/// 数据源状态：客户端接入情况、已跟踪文件、最近扫描时间、同步状态与数据时间范围。
#[tauri::command(rename_all = "camelCase")]
pub fn get_data_source_status(
    state: State<'_, AppState>,
) -> Result<DataSourceStatusDto, CommandError> {
    let status = {
        let store = state.store()?;
        store.client_source_status()?
    };
    let (first_event_ms, last_event_ms) = {
        let store = state.store()?;
        store.data_bounds()?
    };
    Ok(DataSourceStatusDto {
        clients: status
            .into_iter()
            .map(ClientSourceStatusDto::from)
            .collect(),
        sync: state.sync_state(),
        database_path: state.database_path().to_string_lossy().to_string(),
        first_event_ms,
        last_event_ms,
        // 费用没有可靠来源：不展示任何金额。
        cost_available: false,
    })
}

/// 手动触发 Codex 同步（后台执行，完成后广播 `usage-sync` 事件）。
#[tauri::command(rename_all = "camelCase")]
pub fn sync_codex(
    app: tauri::AppHandle,
    force: Option<bool>,
) -> Result<SyncStateDto, CommandError> {
    super::sync::request_codex_sync(&app, force.unwrap_or(false))
}

/// Refresh all implemented collectors. Individual failures are reported separately.
#[tauri::command(rename_all = "camelCase")]
pub fn sync_usage(
    app: tauri::AppHandle,
    force: Option<bool>,
) -> Result<SyncStateDto, CommandError> {
    super::request_sync(&app, force.unwrap_or(false))
}
