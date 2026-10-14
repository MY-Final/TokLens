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
    SESSION_LIMIT_MIN, SESSION_OFFSET_MAX, TimeBasis,
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
///
/// 该结构**刻意宽容**（不 `deny_unknown_fields`）：前端可附带 `groupBy` / `sessionQuery` 等
/// 展示层字段而不报错。展示类选项以规格 `typeOptions` 为权威；`limit` / `offset` 在规格缺省时
/// 作为回退（`limit` 仍按规格 ④ 上限校验，越界拒绝）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PanelQueryArgs {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    /// `stat`(session) 的"今日"起点（本地零点）。
    pub today_from_ms: Option<i64>,
    pub day_start_ms: Option<i64>,
    pub day_count: Option<u32>,
    pub day_boundaries: Vec<i64>,
    pub hourly_boundaries: Vec<i64>,
    /// 回退：规格 `typeOptions.limit` 缺省时使用；规格存在时以规格为准。
    pub limit: Option<u32>,
    /// 会话表偏移（规格 ② 无此字段，仅经 args 传入）。
    pub offset: Option<u32>,
}

/// 面板查询返回的数据：与对应既有命令的 DTO **逐字相同**，前端按 `spec.type` / `timeBasis`
/// 从 `unknown` 收窄后直接喂给现有组件 props。
///
/// - `stat`(session) → `OverviewStats`（同 `get_overview_stats`）
/// - `stat` / `ranking` / `client_distribution`(request) → `RequestAnalysis`（同 `get_request_analysis`）
/// - `token_trend`、`activity_calendar` → `DailyTokenBucket[]`（同 `get_daily_token_trend`）
/// - `request_trend` → `RequestActivity`（同 `get_request_activity`）
/// - `client_distribution`(session) → `ClientUsage[]`（同 `get_client_usage`）
/// - `ranking`(session) → `Rankings`（同 `get_rankings`）
/// - `session_table` → `SessionSummary[]`（同 `get_sessions`）
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
) -> Result<PanelDataDto, CommandError> {
    let validated = super::spec::validate_spec(spec, enabled).map_err(CommandError::new)?;
    let query = route(validated.spec.panel_type, validated.time_basis)
        .ok_or_else(|| CommandError::new("该面板类型不支持所选时间口径"))?;
    run_query(store, &validated, &args, query)
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
            let limit = resolve_limit(
                validated.spec.type_options.limit.or(args.limit),
                RANKING_LIMIT_MIN,
                RANKING_LIMIT_MAX,
                RANKING_LIMIT_DEFAULT,
            )?;
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
            store, validated, args, &filter,
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
    args: &PanelQueryArgs,
    filter: &AnalyticsFilter<'_>,
) -> Result<Vec<SessionSummaryDto>, CommandError> {
    let options = &validated.spec.type_options;
    let limit = resolve_limit(
        options.limit.or(args.limit),
        SESSION_LIMIT_MIN,
        SESSION_LIMIT_MAX,
        SESSION_LIMIT_DEFAULT,
    )?;
    let offset = args.offset.unwrap_or(0);
    if offset > SESSION_OFFSET_MAX {
        return Err(CommandError::new(format!(
            "偏移量不能超过 {SESSION_OFFSET_MAX}"
        )));
    }
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
        .session_summaries(&query_filter, &list_options, limit, offset)?
        .into_iter()
        .map(SessionSummaryDto::from)
        .collect())
}

/// 解析数量上限：`None` 用默认值；越界一律拒绝（不改写）。
fn resolve_limit(raw: Option<u32>, min: u32, max: u32, default: u32) -> Result<u32, CommandError> {
    match raw {
        None => Ok(default),
        Some(limit) if (min..=max).contains(&limit) => Ok(limit),
        Some(_) => Err(CommandError::new(format!("数量必须在 {min}..={max} 之间"))),
    }
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

/// 面板运行查询：单一咽喉，权威校验后分发到既有领域查询；返回对应组件的 DTO。
#[tauri::command(rename_all = "camelCase")]
pub fn run_panel_query(
    app: AppHandle,
    state: State<'_, AppState>,
    spec: Value,
    args: PanelQueryArgs,
) -> Result<PanelDataDto, CommandError> {
    let enabled = enabled_clients(&app)?;
    let spec = super::spec::parse_spec(spec).map_err(CommandError::new)?;
    let store = state.store()?;
    execute(&store, &enabled, spec, args)
}
