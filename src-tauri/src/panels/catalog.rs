//! 编译期面板注册表：`get_panel_catalog()` 的单一事实来源（SSOT）。
//!
//! 目录同时驱动前端手动构建器与（P2）LLM 的允许集合；`run_panel_query` 的分发路由与
//! 上限校验都必须与本目录保持一致（见 `panels::tests` 的一致性回归）。

use serde::Serialize;

use super::spec::*;

/// 目录结构版本（与面板 `specVersion` 独立）。
pub const CATALOG_VERSION: u32 = 1;

/// `type` 对应的后端领域查询（`usage::analytics` / 既有只读命令）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainQuery {
    OverviewStats,
    RequestAnalysis,
    DailyTokenTrend,
    RequestActivity,
    ClientUsage,
    Rankings,
    SessionSummaries,
}

impl DomainQuery {
    pub fn as_str(self) -> &'static str {
        match self {
            DomainQuery::OverviewStats => "get_overview_stats",
            DomainQuery::RequestAnalysis => "get_request_analysis",
            DomainQuery::DailyTokenTrend => "get_daily_token_trend",
            DomainQuery::RequestActivity => "get_request_activity",
            DomainQuery::ClientUsage => "get_client_usage",
            DomainQuery::Rankings => "get_rankings",
            DomainQuery::SessionSummaries => "get_sessions",
        }
    }
}

/// 目录参数类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Enum,
    Int,
    IntArray,
    Bool,
    String,
}

impl ParamKind {
    fn as_str(self) -> &'static str {
        match self {
            ParamKind::Enum => "enum",
            ParamKind::Int => "int",
            ParamKind::IntArray => "intArray",
            ParamKind::Bool => "bool",
            ParamKind::String => "string",
        }
    }
}

/// 编译期参数描述。
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: ParamKind,
    pub required: bool,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub values: &'static [&'static str],
}

/// 编译期面板类型描述。
pub struct PanelTypeSpec {
    pub panel_type: PanelType,
    pub time_bases: &'static [TimeBasis],
    pub default_time_basis: TimeBasis,
    /// 该类型接受的 `typeOptions` 字段名（其余组合键被拒绝）。
    pub fields: &'static [&'static str],
    pub routes: &'static [(TimeBasis, DomainQuery)],
    pub params: &'static [ParamSpec],
}

/// 客户端允许值；与 `Client::ALL` 的一致性由回归测试保证。
pub const CLIENT_VALUES: &[&str] = &[
    "codex",
    "claude_code",
    "opencode",
    "dsh",
    "kimi_code",
    "mimocode",
    "pi_desktop",
    "workbuddy",
    "copilot",
    "gemini_cli",
    "grok",
    "mcode",
];

const SESSION_REQUEST: &[TimeBasis] = &[TimeBasis::Session, TimeBasis::Request];
const SESSION_ONLY: &[TimeBasis] = &[TimeBasis::Session];
const REQUEST_ONLY: &[TimeBasis] = &[TimeBasis::Request];
const SESSION_REQUEST_BASES: &[&str] = &["session", "request"];
const SESSION_BASES: &[&str] = &["session"];
const REQUEST_BASES: &[&str] = &["request"];

const RANGE_PARAM: ParamSpec = ParamSpec {
    name: "range",
    kind: ParamKind::Enum,
    required: true,
    min: None,
    max: None,
    values: &["followPage", "preset", "custom"],
};
const CLIENT_PARAM: ParamSpec = ParamSpec {
    name: "client",
    kind: ParamKind::Enum,
    required: false,
    min: None,
    max: None,
    values: CLIENT_VALUES,
};
const TIME_BASIS_BOTH: ParamSpec = ParamSpec {
    name: "timeBasis",
    kind: ParamKind::Enum,
    required: false,
    min: None,
    max: None,
    values: SESSION_REQUEST_BASES,
};
const TIME_BASIS_SESSION: ParamSpec = ParamSpec {
    name: "timeBasis",
    kind: ParamKind::Enum,
    required: false,
    min: None,
    max: None,
    values: SESSION_BASES,
};
const TIME_BASIS_REQUEST: ParamSpec = ParamSpec {
    name: "timeBasis",
    kind: ParamKind::Enum,
    required: false,
    min: None,
    max: None,
    values: REQUEST_BASES,
};
const FROM_MS: ParamSpec = ParamSpec {
    name: "fromMs",
    kind: ParamKind::Int,
    required: false,
    min: None,
    max: None,
    values: &[],
};
const TO_MS: ParamSpec = ParamSpec {
    name: "toMs",
    kind: ParamKind::Int,
    required: false,
    min: None,
    max: None,
    values: &[],
};

const STAT_PARAMS: &[ParamSpec] = &[
    RANGE_PARAM,
    TIME_BASIS_BOTH,
    CLIENT_PARAM,
    FROM_MS,
    TO_MS,
    ParamSpec {
        name: "todayFromMs",
        kind: ParamKind::Int,
        required: false,
        min: None,
        max: None,
        values: &[],
    },
    ParamSpec {
        name: "field",
        kind: ParamKind::Enum,
        required: false,
        min: None,
        max: None,
        values: &["rangeTotal", "todayTotal", "sessions", "messages"],
    },
];

const TOKEN_TREND_PARAMS: &[ParamSpec] = &[
    RANGE_PARAM,
    TIME_BASIS_SESSION,
    CLIENT_PARAM,
    ParamSpec {
        name: "fromMs",
        kind: ParamKind::Int,
        required: true,
        min: None,
        max: None,
        values: &[],
    },
    ParamSpec {
        name: "dayCount",
        kind: ParamKind::Int,
        required: true,
        min: Some(DAY_COUNT_MIN as i64),
        max: Some(DAY_COUNT_MAX as i64),
        values: &[],
    },
    ParamSpec {
        name: "dayStartMs",
        kind: ParamKind::Int,
        required: false,
        min: None,
        max: None,
        values: &[],
    },
    TO_MS,
    ParamSpec {
        name: "stack",
        kind: ParamKind::Enum,
        required: false,
        min: None,
        max: None,
        values: &["total", "client"],
    },
];

const REQUEST_TREND_PARAMS: &[ParamSpec] = &[
    RANGE_PARAM,
    TIME_BASIS_REQUEST,
    CLIENT_PARAM,
    FROM_MS,
    TO_MS,
    ParamSpec {
        name: "dayBoundaries",
        kind: ParamKind::IntArray,
        required: true,
        min: Some(DAY_BOUNDARY_MIN as i64),
        max: Some(DAY_BOUNDARY_MAX as i64),
        values: &[],
    },
    ParamSpec {
        name: "hourlyBoundaries",
        kind: ParamKind::IntArray,
        required: false,
        min: None,
        max: Some(HOURLY_BOUNDARY_MAX as i64),
        values: &[],
    },
    ParamSpec {
        name: "metric",
        kind: ParamKind::Enum,
        required: false,
        min: None,
        max: None,
        values: &["tokens", "requests"],
    },
];

const CLIENT_DISTRIBUTION_PARAMS: &[ParamSpec] =
    &[RANGE_PARAM, TIME_BASIS_BOTH, CLIENT_PARAM, FROM_MS, TO_MS];

const RANKING_PARAMS: &[ParamSpec] = &[
    RANGE_PARAM,
    TIME_BASIS_BOTH,
    CLIENT_PARAM,
    FROM_MS,
    TO_MS,
    ParamSpec {
        name: "dimension",
        kind: ParamKind::Enum,
        required: false,
        min: None,
        max: None,
        values: &["model", "project"],
    },
    ParamSpec {
        name: "limit",
        kind: ParamKind::Int,
        required: false,
        min: Some(RANKING_LIMIT_MIN as i64),
        max: Some(RANKING_LIMIT_MAX as i64),
        values: &[],
    },
    ParamSpec {
        name: "shortenKeys",
        kind: ParamKind::Bool,
        required: false,
        min: None,
        max: None,
        values: &[],
    },
];

const SESSION_TABLE_PARAMS: &[ParamSpec] = &[
    RANGE_PARAM,
    TIME_BASIS_BOTH,
    CLIENT_PARAM,
    FROM_MS,
    TO_MS,
    ParamSpec {
        name: "limit",
        kind: ParamKind::Int,
        required: false,
        min: Some(SESSION_LIMIT_MIN as i64),
        max: Some(SESSION_LIMIT_MAX as i64),
        values: &[],
    },
    ParamSpec {
        name: "offset",
        kind: ParamKind::Int,
        required: false,
        min: Some(0),
        max: Some(SESSION_OFFSET_MAX as i64),
        values: &[],
    },
    ParamSpec {
        name: "sort",
        kind: ParamKind::Enum,
        required: false,
        min: None,
        max: None,
        values: &["newest", "oldest", "tokens_desc", "requests_desc"],
    },
    ParamSpec {
        name: "requestTime",
        kind: ParamKind::Bool,
        required: false,
        min: None,
        max: None,
        values: &[],
    },
    ParamSpec {
        name: "search",
        kind: ParamKind::String,
        required: false,
        min: None,
        max: Some(STRING_MAX_LEN as i64),
        values: &[],
    },
    ParamSpec {
        name: "model",
        kind: ParamKind::String,
        required: false,
        min: None,
        max: Some(STRING_MAX_LEN as i64),
        values: &[],
    },
    ParamSpec {
        name: "projectPath",
        kind: ParamKind::String,
        required: false,
        min: None,
        max: Some(STRING_MAX_LEN as i64),
        values: &[],
    },
];

const ACTIVITY_CALENDAR_PARAMS: &[ParamSpec] = &[
    RANGE_PARAM,
    TIME_BASIS_SESSION,
    CLIENT_PARAM,
    ParamSpec {
        name: "fromMs",
        kind: ParamKind::Int,
        required: true,
        min: None,
        max: None,
        values: &[],
    },
    ParamSpec {
        name: "dayCount",
        kind: ParamKind::Int,
        required: true,
        min: Some(DAY_COUNT_MIN as i64),
        max: Some(CALENDAR_DAY_COUNT_MAX as i64),
        values: &[],
    },
    ParamSpec {
        name: "dayStartMs",
        kind: ParamKind::Int,
        required: false,
        min: None,
        max: None,
        values: &[],
    },
    TO_MS,
];

/// 面板目录：7 类类型逐一映射到既有领域查询。
pub const CATALOG: &[PanelTypeSpec] = &[
    PanelTypeSpec {
        panel_type: PanelType::Stat,
        time_bases: SESSION_REQUEST,
        default_time_basis: TimeBasis::Session,
        fields: &["field"],
        routes: &[
            (TimeBasis::Session, DomainQuery::OverviewStats),
            (TimeBasis::Request, DomainQuery::RequestAnalysis),
        ],
        params: STAT_PARAMS,
    },
    PanelTypeSpec {
        panel_type: PanelType::TokenTrend,
        time_bases: SESSION_ONLY,
        default_time_basis: TimeBasis::Session,
        fields: &["stack"],
        routes: &[(TimeBasis::Session, DomainQuery::DailyTokenTrend)],
        params: TOKEN_TREND_PARAMS,
    },
    PanelTypeSpec {
        panel_type: PanelType::RequestTrend,
        time_bases: REQUEST_ONLY,
        default_time_basis: TimeBasis::Request,
        fields: &["metric"],
        routes: &[(TimeBasis::Request, DomainQuery::RequestActivity)],
        params: REQUEST_TREND_PARAMS,
    },
    PanelTypeSpec {
        panel_type: PanelType::ClientDistribution,
        time_bases: SESSION_REQUEST,
        default_time_basis: TimeBasis::Session,
        fields: &[],
        routes: &[
            (TimeBasis::Session, DomainQuery::ClientUsage),
            (TimeBasis::Request, DomainQuery::RequestAnalysis),
        ],
        params: CLIENT_DISTRIBUTION_PARAMS,
    },
    PanelTypeSpec {
        panel_type: PanelType::Ranking,
        time_bases: SESSION_REQUEST,
        default_time_basis: TimeBasis::Session,
        fields: &["dimension", "limit", "shortenKeys"],
        routes: &[
            (TimeBasis::Session, DomainQuery::Rankings),
            (TimeBasis::Request, DomainQuery::RequestAnalysis),
        ],
        params: RANKING_PARAMS,
    },
    PanelTypeSpec {
        panel_type: PanelType::SessionTable,
        time_bases: SESSION_REQUEST,
        default_time_basis: TimeBasis::Session,
        fields: &[
            "limit",
            "sort",
            "requestTime",
            "search",
            "model",
            "projectPath",
        ],
        routes: &[
            (TimeBasis::Session, DomainQuery::SessionSummaries),
            (TimeBasis::Request, DomainQuery::SessionSummaries),
        ],
        params: SESSION_TABLE_PARAMS,
    },
    PanelTypeSpec {
        panel_type: PanelType::ActivityCalendar,
        time_bases: SESSION_ONLY,
        default_time_basis: TimeBasis::Session,
        fields: &[],
        routes: &[(TimeBasis::Session, DomainQuery::DailyTokenTrend)],
        params: ACTIVITY_CALENDAR_PARAMS,
    },
];

/// 按类型查找目录条目。
pub fn entry(panel_type: PanelType) -> Option<&'static PanelTypeSpec> {
    CATALOG.iter().find(|spec| spec.panel_type == panel_type)
}

/// 目录导出的参数描述 DTO。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelParam {
    pub name: String,
    pub kind: String,
    pub required: bool,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub values: Vec<String>,
}

/// 目录导出的类型描述 DTO。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelTypeDescriptor {
    #[serde(rename = "type")]
    pub panel_type: String,
    pub time_bases: Vec<String>,
    pub default_time_basis: String,
    pub params: Vec<PanelParam>,
    pub fields: Vec<String>,
    pub query: String,
}

/// 目录导出的全局边界。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalLimits {
    pub title_max_len: u32,
    pub id_max_len: u32,
    pub col_spans: Vec<u32>,
    pub row_spans: Vec<u32>,
    pub order_max: u32,
    pub string_max_len: u32,
    pub day_count_min: u32,
    pub day_count_max: u32,
    pub calendar_day_count_max: u32,
    pub day_boundary_min: u32,
    pub day_boundary_max: u32,
    pub hourly_boundary_max: u32,
    pub ranking_limit_min: u32,
    pub ranking_limit_max: u32,
    pub ranking_limit_default: u32,
    pub session_limit_min: u32,
    pub session_limit_max: u32,
    pub session_limit_default: u32,
    pub session_offset_max: u32,
}

/// 白名单目录（单一事实来源）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelCatalog {
    pub catalog_version: u32,
    pub panel_types: Vec<PanelTypeDescriptor>,
    pub global_limits: GlobalLimits,
}

impl GlobalLimits {
    fn current() -> Self {
        Self {
            title_max_len: TITLE_MAX_LEN as u32,
            id_max_len: ID_MAX_LEN as u32,
            col_spans: COL_SPANS.to_vec(),
            row_spans: ROW_SPANS.to_vec(),
            order_max: ORDER_MAX,
            string_max_len: STRING_MAX_LEN as u32,
            day_count_min: DAY_COUNT_MIN,
            day_count_max: DAY_COUNT_MAX,
            calendar_day_count_max: CALENDAR_DAY_COUNT_MAX,
            day_boundary_min: DAY_BOUNDARY_MIN as u32,
            day_boundary_max: DAY_BOUNDARY_MAX as u32,
            hourly_boundary_max: HOURLY_BOUNDARY_MAX as u32,
            ranking_limit_min: RANKING_LIMIT_MIN,
            ranking_limit_max: RANKING_LIMIT_MAX,
            ranking_limit_default: RANKING_LIMIT_DEFAULT,
            session_limit_min: SESSION_LIMIT_MIN,
            session_limit_max: SESSION_LIMIT_MAX,
            session_limit_default: SESSION_LIMIT_DEFAULT,
            session_offset_max: SESSION_OFFSET_MAX,
        }
    }
}

fn descriptor(spec: &PanelTypeSpec) -> PanelTypeDescriptor {
    PanelTypeDescriptor {
        panel_type: spec.panel_type.as_str().to_string(),
        time_bases: spec
            .time_bases
            .iter()
            .map(|basis| basis.as_str().to_string())
            .collect(),
        default_time_basis: spec.default_time_basis.as_str().to_string(),
        params: spec
            .params
            .iter()
            .map(|param| PanelParam {
                name: param.name.to_string(),
                kind: param.kind.as_str().to_string(),
                required: param.required,
                min: param.min,
                max: param.max,
                values: param.values.iter().map(|value| value.to_string()).collect(),
            })
            .collect(),
        fields: spec.fields.iter().map(|field| field.to_string()).collect(),
        query: spec
            .routes
            .iter()
            .map(|(_, query)| query.as_str())
            .collect::<Vec<_>>()
            .join(" | "),
    }
}

/// 导出白名单目录；前端手动构建器与 P2 的允许集合均以此为单一事实来源。
#[tauri::command]
pub fn get_panel_catalog() -> PanelCatalog {
    PanelCatalog {
        catalog_version: CATALOG_VERSION,
        panel_types: CATALOG.iter().map(descriptor).collect(),
        global_limits: GlobalLimits::current(),
    }
}
