//! `run_panel_query` 单一分发器：按 `type` / `timeBasis` 白名单分发到既有领域查询。
//!
//! 只读、无状态；不落库、不写文件、不发网络。分发器内不得拼接 SQL——所有取值都经
//! `usage::analytics` 的既有函数，输入仅限受限枚举与标量。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use super::catalog::DomainQuery;
use super::spec::{
    CALENDAR_DAY_COUNT_MAX, DAY_BOUNDARY_MAX, DAY_BOUNDARY_MIN, DAY_COUNT_MAX, DAY_COUNT_MIN,
    HOURLY_BOUNDARY_MAX, PanelRange, PanelSpec, PanelType, RANKING_LIMIT_DEFAULT,
    RANKING_LIMIT_MAX, RANKING_LIMIT_MIN, RangeKind, SESSION_LIMIT_DEFAULT, SESSION_LIMIT_MAX,
    SESSION_LIMIT_MIN, TimeBasis,
};
use crate::commands::dto::{
    ClientUsageDto, DailyClientTokensDto, DailyTokenBucketDto, OverviewStatsDto, RankingEntryDto,
    RankingsDto, SessionSummaryDto, client_usage_dto,
};
use crate::commands::request_dto::{RequestActivityDto, RequestAnalysisDto};
use crate::commands::{AppState, CommandError};
use crate::settings::SettingsStore;
use crate::usage::analytics::{
    AnalyticsFilter, SessionListOptions, SessionSort as DomainSessionSort,
};
use crate::usage::client::Client;
use crate::usage::store::UsageStore;

/// 前端提供的、由 `lib/ranges.ts` 解析后的具体时间参数。
///
/// 规格里的 `range` 是声明（`followPage` / `preset` / `custom`）；预设与页面范围由前端按本地日历
/// 解析为具体时间戳与桶边界后传入（不经后端推算，口径复用现有前端时间语义）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct PanelQueryArgs {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    /// `stat`(session) 的"今日"起点（本地零点）。
    pub today_from_ms: Option<i64>,
    pub day_start_ms: Option<i64>,
    pub day_count: Option<u32>,
    pub day_boundaries: Vec<i64>,
    pub hourly_boundaries: Vec<i64>,
}

/// 面板查询返回的数据：与现有前端组件的 props 逐字对应。
///
/// 类型无损直通 `src/lib/usage-types.ts` / `api.ts` 的既有 DTO：`token_trend` 与
/// `activity_calendar` 返回 `DailyTokenBucket[]`，`request_trend` 返回 `RequestActivity`，
/// `ranking`(session) 返回 `Rankings`，`client_distribution`(session) 返回 `ClientUsage[]`，
/// `session_table` 返回 `SessionSummary[]`，`stat` 返回 `OverviewStats`（session）或
/// `RequestAnalysis`（request，与 `ranking`/`client_distribution` 的 request 口径共用）。
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum PanelDataDto {
    Overview(OverviewStatsDto),
    RequestAnalysis(RequestAnalysisDto),
    DailyTokenTrend(Vec<DailyTokenBucketDto>),
    RequestActivity(RequestActivityDto),
    ClientUsage(Vec<ClientUsageDto>),
    Rankings(RankingsDto),
    SessionTable(Vec<SessionSummaryDto>),
}

/// 面板查询结果封装（`data` 为上面其一，`type` / `timeBasis` 供前端分派适配器）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelQueryResultDto {
    pub spec_id: String,
    #[serde(rename = "type")]
    pub panel_type: String,
    pub time_basis: String,
    pub data: PanelDataDto,
}

/// 纯路由映射（无 I/O）：`type` + `timeBasis` → 领域查询。与 `panels::catalog` 必须一致。
pub fn route(panel_type: PanelType, time_basis: TimeBasis) -> Option<DomainQuery> {
    match (panel_type, time_basis) {
        (PanelType::Stat, TimeBasis::Session) => Some(DomainQuery::OverviewStats),
        (PanelType::Stat, TimeBasis::Request) => Some(DomainQuery::RequestAnalysis),
        (PanelType::TokenTrend, TimeBasis::Session) => Some(DomainQuery::DailyTokenTrend),
        (PanelType::RequestTrend, TimeBasis::Request) => Some(DomainQuery::RequestActivity),
        (PanelType::ClientDistribution, TimeBasis::Session) => Some(DomainQuery::ClientUsage),
        (PanelType::ClientDistribution, TimeBasis::Request) => Some(DomainQuery::RequestAnalysis),
        (PanelType::Ranking, TimeBasis::Session) => Some(DomainQuery::Rankings),
        (PanelType::Ranking, TimeBasis::Request) => Some(DomainQuery::RequestAnalysis),
        (PanelType::SessionTable, TimeBasis::Session | TimeBasis::Request) => {
            Some(DomainQuery::SessionSummaries)
        }
        (PanelType::ActivityCalendar, TimeBasis::Session) => Some(DomainQuery::DailyTokenTrend),
        _ => None,
    }
}

/// 执行一次面板查询（校验 + 分发 + 组装 DTO），不触碰 Tauri 上下文，便于合成数据单测。
pub(crate) fn execute(
    store: &UsageStore,
    enabled: &[Client],
    spec: PanelSpec,
    args: PanelQueryArgs,
) -> Result<PanelQueryResultDto, CommandError> {
    let validated = super::spec::validate_spec(spec, enabled).map_err(CommandError::new)?;
    let query = route(validated.spec.panel_type, validated.time_basis)
        .ok_or_else(|| CommandError::new("该面板类型不支持所选时间口径"))?;
    let data = run_query(store, &validated, &args, query)?;
    Ok(PanelQueryResultDto {
        spec_id: validated.spec.id.clone(),
        panel_type: validated.spec.panel_type.as_str().to_string(),
        time_basis: validated.time_basis.as_str().to_string(),
        data,
    })
}

fn run_query(
    store: &UsageStore,
    validated: &super::spec::ValidatedSpec,
    args: &PanelQueryArgs,
    query: DomainQuery,
) -> Result<PanelDataDto, CommandError> {
    let (from_ms, to_ms) = resolve_range(args, &validated.spec.query.range);
    let filter = AnalyticsFilter {
        from_ms,
        to_ms,
        client: validated.client,
        project_path: None,
    };
    match query {
        DomainQuery::OverviewStats => {
            let range = store.range_totals(&filter)?;
            let today = match args.today_from_ms {
                Some(today_from) => store.range_totals(&AnalyticsFilter {
                    from_ms: Some(today_from),
                    to_ms: None,
                    client: validated.client,
                    project_path: None,
                })?,
                None => Default::default(),
            };
            let (first_event_ms, last_event_ms) = store.data_bounds()?;
            Ok(PanelDataDto::Overview(OverviewStatsDto {
                range: range.into(),
                today: today.into(),
                first_event_ms,
                last_event_ms,
            }))
        }
        DomainQuery::RequestAnalysis => Ok(PanelDataDto::RequestAnalysis(
            store.request_analysis(&filter)?.into(),
        )),
        DomainQuery::DailyTokenTrend => Ok(PanelDataDto::DailyTokenTrend(daily_buckets(
            store, validated, args, from_ms, to_ms,
        )?)),
        DomainQuery::RequestActivity => {
            validate_activity_buckets(args, from_ms, to_ms)?;
            let activity = store.request_activity_with_hourly(
                &filter,
                &args.day_boundaries,
                &args.hourly_boundaries,
            )?;
            Ok(PanelDataDto::RequestActivity(activity.into()))
        }
        DomainQuery::ClientUsage => Ok(PanelDataDto::ClientUsage(client_usage_dto(
            &store.client_usage(&filter)?,
        ))),
        DomainQuery::Rankings => {
            let limit = ranking_limit(validated.spec.type_options.limit);
            Ok(PanelDataDto::Rankings(RankingsDto {
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
            }))
        }
        DomainQuery::SessionSummaries => Ok(PanelDataDto::SessionTable(session_summaries(
            store, validated, &filter,
        )?)),
    }
}

fn daily_buckets(
    store: &UsageStore,
    validated: &super::spec::ValidatedSpec,
    args: &PanelQueryArgs,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
) -> Result<Vec<DailyTokenBucketDto>, CommandError> {
    let from_ms = from_ms.ok_or_else(|| CommandError::new("每日趋势需要 fromMs"))?;
    let max = if validated.spec.panel_type == PanelType::ActivityCalendar {
        CALENDAR_DAY_COUNT_MAX
    } else {
        DAY_COUNT_MAX
    };
    let day_count = args
        .day_count
        .ok_or_else(|| CommandError::new("每日趋势需要 dayCount"))?;
    if !(DAY_COUNT_MIN..=max).contains(&day_count) {
        return Err(CommandError::new(format!(
            "dayCount 必须在 {DAY_COUNT_MIN}..={max} 之间"
        )));
    }
    if to_ms.is_some_and(|to| to <= from_ms) {
        return Err(CommandError::new("结束时间必须晚于开始时间。"));
    }
    let day_start_ms = args.day_start_ms.unwrap_or(from_ms);
    let mut buckets: Vec<DailyTokenBucketDto> = store
        .daily_token_trend_in_range(
            day_start_ms,
            day_count,
            Some(from_ms),
            to_ms,
            validated.client,
        )?
        .into_iter()
        .map(DailyTokenBucketDto::from)
        .collect();
    for entry in Client::ALL {
        if validated.client.is_some_and(|client| client != entry) {
            continue;
        }
        let daily = store.daily_token_trend_in_range(
            day_start_ms,
            day_count,
            Some(from_ms),
            to_ms,
            Some(entry),
        )?;
        for (dto, bucket) in buckets.iter_mut().zip(daily) {
            dto.clients.push(DailyClientTokensDto {
                client: entry.as_str().to_string(),
                sessions: bucket.sessions,
                sessions_without_usage: bucket.sessions_without_usage,
                sessions_partial: bucket.sessions_partial,
                total_tokens: bucket.sums.total_tokens,
            });
        }
    }
    Ok(buckets)
}

fn validate_activity_buckets(
    args: &PanelQueryArgs,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
) -> Result<(), CommandError> {
    let days = &args.day_boundaries;
    if days.len() < DAY_BOUNDARY_MIN
        || days.len() > DAY_BOUNDARY_MAX
        || days.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(CommandError::new("日期范围无效，最多显示 400 个连续日桶。"));
    }
    let hours = &args.hourly_boundaries;
    if hours.len() > HOURLY_BOUNDARY_MAX {
        return Err(CommandError::new("小时桶数量超出上限。"));
    }
    if !hours.is_empty() {
        if hours.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(CommandError::new("小时桶必须严格递增。"));
        }
        if let (Some(from), Some(to)) = (from_ms, to_ms)
            && (hours.first() != Some(&from) || hours.last() != Some(&to))
        {
            return Err(CommandError::new("小时桶端点必须等于所选区间。"));
        }
    }
    Ok(())
}

fn session_summaries(
    store: &UsageStore,
    validated: &super::spec::ValidatedSpec,
    filter: &AnalyticsFilter<'_>,
) -> Result<Vec<SessionSummaryDto>, CommandError> {
    let options = &validated.spec.type_options;
    let limit = options
        .limit
        .unwrap_or(SESSION_LIMIT_DEFAULT)
        .clamp(SESSION_LIMIT_MIN, SESSION_LIMIT_MAX);
    let mut query_filter = *filter;
    query_filter.project_path = options.project_path.as_deref();
    let search = options
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let sort = match options.sort {
        None | Some(super::spec::SessionSort::Newest) => DomainSessionSort::Newest,
        Some(super::spec::SessionSort::Oldest) => DomainSessionSort::Oldest,
        Some(super::spec::SessionSort::TokensDesc) => DomainSessionSort::TotalTokens,
        Some(super::spec::SessionSort::RequestsDesc) => DomainSessionSort::Requests,
    };
    let list_options = SessionListOptions {
        search,
        model: options.model.as_deref(),
        model_unknown: false,
        project_unknown: false,
        request_time: options.request_time.unwrap_or(false),
        sort,
    };
    Ok(store
        .session_summaries(&query_filter, &list_options, limit, 0)?
        .into_iter()
        .map(SessionSummaryDto::from)
        .collect())
}

fn ranking_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(RANKING_LIMIT_DEFAULT)
        .clamp(RANKING_LIMIT_MIN, RANKING_LIMIT_MAX)
}

/// custom 范围可直接给出时间戳；否则用前端解析后的参数。
fn resolve_range(args: &PanelQueryArgs, range: &PanelRange) -> (Option<i64>, Option<i64>) {
    match range.kind {
        RangeKind::Custom => (args.from_ms.or(range.from_ms), args.to_ms.or(range.to_ms)),
        _ => (args.from_ms, args.to_ms),
    }
}

/// 读取已启用客户端（校验 `client` 是否在数据源中启用）。
pub(crate) fn enabled_clients(app: &AppHandle) -> Result<Vec<Client>, CommandError> {
    app.state::<SettingsStore>()
        .snapshot()
        .map(|snapshot| snapshot.settings.clients())
        .map_err(CommandError::new)
}

/// 面板运行查询：单一咽喉，权威校验后分发到既有领域查询。
#[tauri::command(rename_all = "camelCase")]
pub fn run_panel_query(
    app: AppHandle,
    state: State<'_, AppState>,
    spec: Value,
    args: PanelQueryArgs,
) -> Result<PanelQueryResultDto, CommandError> {
    let enabled = enabled_clients(&app)?;
    let spec = super::spec::parse_spec(spec).map_err(CommandError::new)?;
    let store = state.store()?;
    execute(&store, &enabled, spec, args)
}
