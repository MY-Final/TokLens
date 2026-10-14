//! 生成式面板规格（PanelSpec）：纯数据、封闭白名单、严格校验。
//!
//! 规格只描述"看什么"（类型 / 时间口径 / 客户端 / 展示选项 / 布局），字段为封闭枚举与标量，
//! 不含 SQL、字段名、公式、提示词或文件路径；所有数值由 `panels::query` 分发到 `usage::analytics` 求得。
//!
//! 版本策略：`specVersion` 独立于设置文件版本。高于本构建支持版本的规格被拒绝（提示升级），
//! 低于支持版本的规格经 [`migrate_spec`] 迁移后执行。当前仅存在版本 1。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::usage::client::Client;

use super::catalog;

/// 本构建支持的规格版本。
pub const SUPPORTED_SPEC_VERSION: u32 = 1;

/// `id` 最大长度（前端生成的稳定标识）。
pub const ID_MAX_LEN: usize = 64;
/// 标题最大字符数。
pub const TITLE_MAX_LEN: usize = 80;
/// 可选的列宽（主区 12 列网格）。
pub const COL_SPANS: [u32; 4] = [3, 4, 6, 12];
/// 可选的行高。
pub const ROW_SPANS: [u32; 2] = [1, 2];
/// 排序值上限。
pub const ORDER_MAX: u32 = 63;
/// 文本类参数（search / model / projectPath）最大字符数。
pub const STRING_MAX_LEN: usize = 200;
/// 排行数量下限 / 上限 / 默认值。
pub const RANKING_LIMIT_MIN: u32 = 1;
pub const RANKING_LIMIT_MAX: u32 = 50;
pub const RANKING_LIMIT_DEFAULT: u32 = 8;
/// 会话表数量下限 / 上限 / 默认值。
pub const SESSION_LIMIT_MIN: u32 = 1;
pub const SESSION_LIMIT_MAX: u32 = 500;
pub const SESSION_LIMIT_DEFAULT: u32 = 20;
/// 会话表偏移上限。
pub const SESSION_OFFSET_MAX: u32 = 1_000_000;
/// 日趋势桶数下限 / 上限；日历建议不超过 365。
pub const DAY_COUNT_MIN: u32 = 1;
pub const DAY_COUNT_MAX: u32 = 400;
pub const CALENDAR_DAY_COUNT_MAX: u32 = 365;
/// 请求活动日桶数量范围（含端点）与小时桶上限。
pub const DAY_BOUNDARY_MIN: usize = 2;
pub const DAY_BOUNDARY_MAX: usize = 401;
pub const HOURLY_BOUNDARY_MAX: usize = 27;

/// 面板类型（对应前端具体粒度组件，7 类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelType {
    Stat,
    TokenTrend,
    RequestTrend,
    ClientDistribution,
    Ranking,
    SessionTable,
    ActivityCalendar,
}

impl PanelType {
    pub const ALL: [PanelType; 7] = [
        PanelType::Stat,
        PanelType::TokenTrend,
        PanelType::RequestTrend,
        PanelType::ClientDistribution,
        PanelType::Ranking,
        PanelType::SessionTable,
        PanelType::ActivityCalendar,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PanelType::Stat => "stat",
            PanelType::TokenTrend => "token_trend",
            PanelType::RequestTrend => "request_trend",
            PanelType::ClientDistribution => "client_distribution",
            PanelType::Ranking => "ranking",
            PanelType::SessionTable => "session_table",
            PanelType::ActivityCalendar => "activity_calendar",
        }
    }
}

/// 时间口径：会话累计 / 请求时间。两者永不混算。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeBasis {
    Session,
    Request,
}

impl TimeBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            TimeBasis::Session => "session",
            TimeBasis::Request => "request",
        }
    }
}

/// 仅影响前端展示，后端一次性返回完整 DTO。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Tokens,
    Requests,
}

impl Metric {
    pub fn as_str(self) -> &'static str {
        match self {
            Metric::Tokens => "tokens",
            Metric::Requests => "requests",
        }
    }
}

/// 排行维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Model,
    Project,
}

impl Dimension {
    pub fn as_str(self) -> &'static str {
        match self {
            Dimension::Model => "model",
            Dimension::Project => "project",
        }
    }
}

/// 趋势堆叠方式（前端 `TokenTrendChart` 固定用 `client` / `tokens`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stack {
    Tokens,
    Client,
}

impl Stack {
    pub fn as_str(self) -> &'static str {
        match self {
            Stack::Tokens => "tokens",
            Stack::Client => "client",
        }
    }
}

/// `stat` 面板选中的指标字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatField {
    RangeTotal,
    TodayTotal,
    Sessions,
    Messages,
}

impl StatField {
    pub fn as_str(self) -> &'static str {
        match self {
            StatField::RangeTotal => "rangeTotal",
            StatField::TodayTotal => "todayTotal",
            StatField::Sessions => "sessions",
            StatField::Messages => "messages",
        }
    }
}

/// 会话表排序方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSort {
    Newest,
    Oldest,
    TokensDesc,
    RequestsDesc,
}

impl SessionSort {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionSort::Newest => "newest",
            SessionSort::Oldest => "oldest",
            SessionSort::TokensDesc => "tokens_desc",
            SessionSort::RequestsDesc => "requests_desc",
        }
    }
}

/// 预置时间范围键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RangePreset {
    Today,
    #[serde(rename = "7d")]
    Days7,
    #[serde(rename = "30d")]
    Days30,
    #[serde(rename = "90d")]
    Days90,
    All,
    Week,
    Month,
}

impl RangePreset {
    pub fn as_str(self) -> &'static str {
        match self {
            RangePreset::Today => "today",
            RangePreset::Days7 => "7d",
            RangePreset::Days30 => "30d",
            RangePreset::Days90 => "90d",
            RangePreset::All => "all",
            RangePreset::Week => "week",
            RangePreset::Month => "month",
        }
    }
}

/// 范围种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RangeKind {
    FollowPage,
    Preset,
    Custom,
}

impl RangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RangeKind::FollowPage => "followPage",
            RangeKind::Preset => "preset",
            RangeKind::Custom => "custom",
        }
    }
}

/// 面板来源（仅用于 UI 标注，不参与取值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Manual,
    Ai,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Manual => "manual",
            Origin::Ai => "ai",
        }
    }
}

fn default_col_span() -> u32 {
    12
}

fn default_row_span() -> u32 {
    1
}

fn default_origin() -> Origin {
    Origin::Manual
}

/// 时间范围声明（语义由前端 `lib/ranges.ts` 解析为具体时间戳）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelRange {
    pub kind: RangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<RangePreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_ms: Option<i64>,
}

/// 查询声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelQuery {
    pub range: PanelRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_basis: Option<TimeBasis>,
    /// 客户端标识（`ClientKey`），`null` 表示全部。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

/// 布局：主区 12 列网格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelLayout {
    #[serde(default = "default_col_span")]
    pub col_span: u32,
    #[serde(default = "default_row_span")]
    pub row_span: u32,
    #[serde(default)]
    pub order: u32,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            col_span: default_col_span(),
            row_span: default_row_span(),
            order: 0,
        }
    }
}

/// 来源标注。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelSource {
    #[serde(default = "default_origin")]
    pub origin: Origin,
}

impl Default for PanelSource {
    fn default() -> Self {
        Self {
            origin: default_origin(),
        }
    }
}

/// 类型专属选项：全部有枚举或数值边界，未知键由 `deny_unknown_fields` 拒绝。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct PanelTypeOptions {
    /// `stat` 选中的指标字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<StatField>,
    /// 保留字段（规格 ② 声明）；P1 未使用，仅供白名单兼容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<Metric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimension: Option<Dimension>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<SessionSort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_time: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<Stack>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shorten_keys: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
}

/// 面板规格（纯数据；未知字段一律拒绝）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelSpec {
    pub spec_version: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub panel_type: PanelType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub query: PanelQuery,
    #[serde(default)]
    pub type_options: PanelTypeOptions,
    #[serde(default)]
    pub layout: PanelLayout,
    #[serde(default)]
    pub source: PanelSource,
}

/// 规格版本相对本构建的分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecVersionClass {
    /// 与支持版本一致。
    Current,
    /// 低于支持版本，需迁移。
    Migrate(u32),
    /// 高于支持版本；保留原始数据并标记只读。
    Unsupported(u32),
}

/// 分类 `specVersion`（纯函数）。
pub fn classify_spec_version(version: u32) -> SpecVersionClass {
    if version == SUPPORTED_SPEC_VERSION {
        SpecVersionClass::Current
    } else if version < SUPPORTED_SPEC_VERSION {
        SpecVersionClass::Migrate(version)
    } else {
        SpecVersionClass::Unsupported(version)
    }
}

/// 将低版本规格前向迁移到 [`SUPPORTED_SPEC_VERSION`]（纯函数，可单测）。
///
/// 当前支持版本为 1，尚无更早版本，因此非当前版本会显式失败；未来新增版本时在此追加迁移步骤。
pub fn migrate_spec(value: Value) -> Result<Value, String> {
    let version = read_spec_version(&value).unwrap_or(0);
    match classify_spec_version(version) {
        SpecVersionClass::Current => Ok(value),
        SpecVersionClass::Unsupported(v) => {
            Err(format!("面板规格版本 {v} 暂不支持，请升级 TokLens"))
        }
        SpecVersionClass::Migrate(v) => Err(format!("无法从面板规格版本 {v} 迁移")),
    }
}

/// 读取规格中的 `specVersion`（缺失返回 `None`）。
pub fn read_spec_version(value: &Value) -> Option<u32> {
    value
        .get("specVersion")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
}

/// 解析原始 JSON 规格：校验版本、迁移低版本、反序列化为 [`PanelSpec`]（严格模式）。
pub fn parse_spec(value: Value) -> Result<PanelSpec, String> {
    let version =
        read_spec_version(&value).ok_or_else(|| "面板规格缺少 specVersion".to_string())?;
    match classify_spec_version(version) {
        SpecVersionClass::Unsupported(v) => {
            Err(format!("面板规格版本 {v} 暂不支持，请升级 TokLens"))
        }
        _ => {
            let migrated = migrate_spec(value)?;
            serde_json::from_value::<PanelSpec>(migrated)
                .map_err(|error| format!("面板规格无效：{error}"))
        }
    }
}

/// 通过权威校验的规格：保留了原始规格，并解析出时间口径与客户端。
#[derive(Debug, Clone)]
pub struct ValidatedSpec {
    pub spec: PanelSpec,
    pub time_basis: TimeBasis,
    pub client: Option<Client>,
}

/// 权威校验：枚举 / 长度 / 边界，`client` 必须已解析且属于已启用集合，越界一律拒绝（不改写）。
pub fn validate_spec(spec: PanelSpec, enabled: &[Client]) -> Result<ValidatedSpec, String> {
    if spec.spec_version != SUPPORTED_SPEC_VERSION {
        return Err(format!(
            "面板规格版本 {} 暂不支持，请升级 TokLens",
            spec.spec_version
        ));
    }
    if spec.id.is_empty() || spec.id.chars().count() > ID_MAX_LEN {
        return Err("面板标识无效".into());
    }
    if let Some(title) = &spec.title
        && title.chars().count() > TITLE_MAX_LEN
    {
        return Err(format!("面板标题不能超过 {TITLE_MAX_LEN} 个字符"));
    }

    let entry = catalog::entry(spec.panel_type).ok_or_else(|| "未知的面板类型".to_string())?;
    let time_basis = spec.query.time_basis.unwrap_or(entry.default_time_basis);
    if !entry.time_bases.contains(&time_basis) {
        return Err("该面板类型不支持所选时间口径".into());
    }

    let client = match spec.query.client.as_deref() {
        None => None,
        Some(value) => {
            let client = Client::parse(value).ok_or_else(|| format!("未知的客户端：{value}"))?;
            if !enabled.contains(&client) {
                return Err("该客户端未在数据源中启用".into());
            }
            Some(client)
        }
    };

    validate_range(&spec.query.range)?;
    validate_options(spec.panel_type, &spec.type_options)?;
    validate_layout(&spec.layout)?;

    Ok(ValidatedSpec {
        spec,
        time_basis,
        client,
    })
}

fn validate_range(range: &PanelRange) -> Result<(), String> {
    match range.kind {
        RangeKind::FollowPage => {
            if range.key.is_some() || range.from_ms.is_some() || range.to_ms.is_some() {
                return Err("followPage 范围不接受 key/fromMs/toMs".into());
            }
        }
        RangeKind::Preset => {
            if range.key.is_none() {
                return Err("preset 范围必须提供 key".into());
            }
            if range.from_ms.is_some() || range.to_ms.is_some() {
                return Err("preset 范围不接受 fromMs/toMs".into());
            }
        }
        RangeKind::Custom => {
            if range.key.is_some() {
                return Err("custom 范围不接受 key".into());
            }
            let (Some(from), Some(to)) = (range.from_ms, range.to_ms) else {
                return Err("custom 范围必须提供 fromMs 与 toMs".into());
            };
            if from >= to {
                return Err("结束时间必须晚于开始时间".into());
            }
        }
    }
    Ok(())
}

/// 可被任意类型接受的保留字段（规格 ② 声明但 P1 未使用）。
const RESERVED_OPTIONS: &[&str] = &["fields"];

fn validate_options(panel_type: PanelType, options: &PanelTypeOptions) -> Result<(), String> {
    if let Some(fields) = &options.fields
        && fields
            .iter()
            .any(|field| field.chars().count() > STRING_MAX_LEN)
    {
        return Err("字段选择过长".into());
    }
    let allowed: &[&str] = match panel_type {
        PanelType::Stat => &["field"],
        PanelType::TokenTrend => &["stack"],
        PanelType::RequestTrend => &["metric"],
        PanelType::ClientDistribution => &[],
        PanelType::Ranking => &["dimension", "limit", "shortenKeys"],
        PanelType::SessionTable => &[
            "limit",
            "sort",
            "requestTime",
            "search",
            "model",
            "projectPath",
        ],
        PanelType::ActivityCalendar => &[],
    };
    reject_unknown_options(options, allowed)?;
    validate_limits(panel_type, options)?;
    validate_strings(options)?;
    Ok(())
}

fn reject_unknown_options(options: &PanelTypeOptions, allowed: &[&str]) -> Result<(), String> {
    let present = [
        ("field", options.field.is_some()),
        ("fields", options.fields.is_some()),
        ("metric", options.metric.is_some()),
        ("dimension", options.dimension.is_some()),
        ("limit", options.limit.is_some()),
        ("sort", options.sort.is_some()),
        ("requestTime", options.request_time.is_some()),
        ("stack", options.stack.is_some()),
        ("shortenKeys", options.shorten_keys.is_some()),
        ("search", options.search.is_some()),
        ("model", options.model.is_some()),
        ("projectPath", options.project_path.is_some()),
    ];
    for (name, is_present) in present {
        if is_present && !allowed.contains(&name) && !RESERVED_OPTIONS.contains(&name) {
            return Err(format!("面板类型不支持参数 {name}"));
        }
    }
    Ok(())
}

fn validate_limits(panel_type: PanelType, options: &PanelTypeOptions) -> Result<(), String> {
    if let Some(limit) = options.limit {
        let (min, max) = match panel_type {
            PanelType::Ranking => (RANKING_LIMIT_MIN, RANKING_LIMIT_MAX),
            PanelType::SessionTable => (SESSION_LIMIT_MIN, SESSION_LIMIT_MAX),
            // 其余类型不接受 limit：`reject_unknown_options` 已拒绝。
            _ => return Ok(()),
        };
        if !(min..=max).contains(&limit) {
            return Err(format!("数量必须在 {min}..={max} 之间"));
        }
    }
    Ok(())
}

fn validate_strings(options: &PanelTypeOptions) -> Result<(), String> {
    for value in [&options.search, &options.model, &options.project_path]
        .into_iter()
        .flatten()
    {
        if value.chars().count() > STRING_MAX_LEN {
            return Err(format!("文本参数不能超过 {STRING_MAX_LEN} 个字符"));
        }
    }
    Ok(())
}

fn validate_layout(layout: &PanelLayout) -> Result<(), String> {
    if !COL_SPANS.contains(&layout.col_span) {
        return Err("列宽必须为 3/4/6/12".into());
    }
    if !ROW_SPANS.contains(&layout.row_span) {
        return Err("行高必须为 1 或 2".into());
    }
    if layout.order > ORDER_MAX {
        return Err("排序值超出范围".into());
    }
    Ok(())
}
