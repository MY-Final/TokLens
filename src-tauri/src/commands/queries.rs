//! 只读查询命令：全部走 `usage::analytics` 的聚合，命令层不做口径计算。

use tauri::State;

use super::dto::{
    ClientParam, ClientSourceStatusDto, DailyTokenBucketDto, DataSourceStatusDto, OverviewStatsDto,
    RankingEntryDto, RankingsDto, SessionQuery, SessionSummaryDto, client_usage_dto,
};
use super::{AppState, CommandError, SyncStateDto};
use crate::usage::analytics::{AnalyticsFilter, SessionListOptions, SessionSort};
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

/// 每日 Token 趋势：精确会话范围与本地零点显示桶起点独立，缺失日期补 0。
#[tauri::command(rename_all = "camelCase")]
pub fn get_daily_token_trend(
    state: State<'_, AppState>,
    from_ms: i64,
    day_count: u32,
    day_start_ms: Option<i64>,
    to_ms: Option<i64>,
    client: Option<ClientParam>,
) -> Result<Vec<DailyTokenBucketDto>, CommandError> {
    if to_ms.is_some_and(|to| to <= from_ms) {
        return Err(CommandError::new("结束时间必须晚于开始时间。"));
    }
    let day_start_ms = day_start_ms.unwrap_or(from_ms);
    let store = state.store()?;
    let day_count = day_count.clamp(1, 400);
    let buckets = store.daily_token_trend_in_range(
        day_start_ms,
        day_count,
        Some(from_ms),
        to_ms,
        client.map(Client::from),
    )?;
    let mut dtos: Vec<_> = buckets.into_iter().map(DailyTokenBucketDto::from).collect();
    for entry in Client::ALL {
        if client.is_some_and(|filter| Client::from(filter) != entry) {
            continue;
        }
        let daily = store.daily_token_trend_in_range(
            day_start_ms,
            day_count,
            Some(from_ms),
            to_ms,
            Some(entry),
        )?;
        for (dto, bucket) in dtos.iter_mut().zip(daily) {
            dto.clients.push(super::dto::DailyClientTokensDto {
                client: entry.as_str().to_string(),
                sessions: bucket.sessions,
                sessions_without_usage: bucket.sessions_without_usage,
                sessions_partial: bucket.sessions_partial,
                total_tokens: bucket.sums.total_tokens,
            });
        }
    }
    Ok(dtos)
}

/// 客户端用量分布：固定返回十二个客户端，空数据保持为 0，占比由领域查询提供。
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

/// Paginated metadata-only sessions, with search, exact grouping and time-basis drilldown.
#[tauri::command(rename_all = "camelCase")]
pub fn get_sessions(
    state: State<'_, AppState>,
    args: SessionQuery,
) -> Result<Vec<SessionSummaryDto>, CommandError> {
    let SessionQuery {
        from_ms,
        to_ms,
        client,
        limit,
        offset,
        search,
        project_path,
        sort,
        model,
        model_unknown,
        project_unknown,
        request_time,
    } = args;
    if matches!((from_ms, to_ms), (Some(from), Some(to)) if from >= to) {
        return Err(CommandError::new("结束时间必须晚于开始时间。"));
    }
    let store = state.store()?;
    let limit = limit
        .unwrap_or(DEFAULT_SESSION_LIMIT)
        .clamp(1, MAX_SESSION_LIMIT);
    let offset = offset.unwrap_or(0);
    let mut query_filter = filter(from_ms, to_ms, client);
    query_filter.project_path = project_path.as_deref();
    let sort = match sort.as_deref().unwrap_or("newest") {
        "newest" => SessionSort::Newest,
        "oldest" => SessionSort::Oldest,
        "tokens_desc" => SessionSort::TotalTokens,
        "requests_desc" => SessionSort::Requests,
        _ => return Err(CommandError::new("会话排序方式无效。")),
    };
    let options = SessionListOptions {
        search: search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        model: model.as_deref(),
        model_unknown: model_unknown.unwrap_or(false),
        project_unknown: project_unknown.unwrap_or(false),
        request_time: request_time.unwrap_or(false),
        sort,
    };
    let sessions = store.session_summaries(&query_filter, &options, limit, offset)?;
    Ok(sessions.into_iter().map(SessionSummaryDto::from).collect())
}

/// 数据源状态：客户端接入情况、已跟踪文件、最近扫描时间、同步状态与数据时间范围。
#[tauri::command(rename_all = "camelCase")]
pub fn get_data_source_status(
    state: State<'_, AppState>,
) -> Result<DataSourceStatusDto, CommandError> {
    let roots = crate::collectors::CollectorRoots::from_environment();
    // Read sync first: completed status must never describe counts read before completion.
    let sync = state.sync_state();
    let checks = state
        .sync
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .client_checks
        .clone();
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
            .map(|status| {
                let source_path = roots
                    .path(status.client)
                    .map(|path| path.to_string_lossy().into_owned());
                let source_paths = roots
                    .source_locations(status.client)
                    .into_iter()
                    .map(super::dto::SourcePathDto::from)
                    .collect();
                let check = checks.get(&status.client);
                ClientSourceStatusDto {
                    source_path,
                    source_paths,
                    last_checked_ms: check.map(|(at, _)| *at),
                    last_result: check.map(|(_, result)| result.clone().into()),
                    ..status.into()
                }
            })
            .collect(),
        sync,
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

/// Re-scan a single enabled client through the shared background synchronization gate.
#[tauri::command(rename_all = "camelCase")]
pub fn sync_client(
    app: tauri::AppHandle,
    client: ClientParam,
    force: Option<bool>,
) -> Result<SyncStateDto, CommandError> {
    super::sync::request_client_sync(&app, client.into(), force.unwrap_or(true))
}

/// Refresh all implemented collectors. Individual failures are reported separately.
#[tauri::command(rename_all = "camelCase")]
pub fn sync_usage(
    app: tauri::AppHandle,
    force: Option<bool>,
) -> Result<SyncStateDto, CommandError> {
    super::request_sync(&app, force.unwrap_or(false))
}
