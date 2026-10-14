/**
 * 生成式面板规格（PanelSpec）类型、目录 DTO 与前端镜像校验。
 *
 * 规格为纯数据、封闭白名单，拒绝未知字段；权威校验在 Rust `run_panel_query`，
 * 此处只做 UX 预校验与默认值推导（见 docs/generative-panel-spec.md ⑦）。
 */

import { startOfDay } from "date-fns";

import { CLIENT_KEYS } from "@/lib/client-labels";
import {
  createCustomRange,
  localDayBoundaries,
  localHourBoundaries,
  rangeScope,
  RANGE_PRESETS,
  resolveRange,
  type DateRange,
} from "@/lib/ranges";
import type { ClientKey, SessionFilters } from "@/lib/usage-types";

/** 已知的 spec 版本；更高版本加载后标记为只读、不丢弃。 */
export const PANEL_SPEC_VERSION = 1;
export const PANEL_COL_SPANS = [3, 4, 6, 12] as const;
export const PANEL_ROW_SPANS = [1, 2] as const;
export const PANEL_MAX_ORDER = 63;
export const PANEL_MAX_TITLE = 80;
export const PANEL_MAX_ID = 64;
export const PANEL_MAX_TEXT = 200;
/** limit 边界按类型区分（与 Rust `panels::spec` 常量一致）。 */
export const PANEL_RANKING_LIMIT_MIN = 1;
export const PANEL_RANKING_LIMIT_MAX = 50;
export const PANEL_RANKING_LIMIT_DEFAULT = 8;
export const PANEL_SESSION_LIMIT_MIN = 1;
export const PANEL_SESSION_LIMIT_MAX = 500;
export const PANEL_SESSION_LIMIT_DEFAULT = 20;

export const PANEL_TYPES = [
  "stat",
  "token_trend",
  "request_trend",
  "client_distribution",
  "ranking",
  "session_table",
  "activity_calendar",
] as const;

export type PanelType = (typeof PANEL_TYPES)[number];
export type PanelTimeBasis = "session" | "request";
export type PanelRangeKind = "followPage" | "preset" | "custom";
export type PanelPresetKey = "today" | "7d" | "30d" | "90d" | "all" | "week" | "month";
export type PanelMetric = "tokens" | "requests";
export type PanelDimension = "model" | "project";
export type PanelSort = "newest" | "oldest" | "tokens_desc" | "requests_desc";
export type PanelStack = "client" | "tokens";
export type PanelStatField = "rangeTotal" | "todayTotal" | "sessions" | "messages";
export type PanelSourceOrigin = "manual" | "ai";
export type PanelColSpan = (typeof PANEL_COL_SPANS)[number];
export type PanelRowSpan = (typeof PANEL_ROW_SPANS)[number];

export type PanelRange =
  | { kind: "followPage" }
  | { kind: "preset"; key: PanelPresetKey }
  | { kind: "custom"; fromMs: number; toMs: number };

export type PanelTypeOptions = {
  metric?: PanelMetric;
  dimension?: PanelDimension;
  limit?: number;
  sort?: PanelSort;
  requestTime?: boolean;
  stack?: PanelStack;
  fields?: string[];
  shortenKeys?: boolean;
  search?: string | null;
  model?: string | null;
  projectPath?: string | null;
  field?: PanelStatField;
};

export type PanelQuery = {
  range: PanelRange;
  timeBasis?: PanelTimeBasis;
  client: ClientKey | null;
};

export type PanelLayout = {
  colSpan: PanelColSpan;
  rowSpan: PanelRowSpan;
  order: number;
};

export type PanelSpec = {
  specVersion: number;
  id: string;
  type: PanelType;
  title?: string;
  query: PanelQuery;
  typeOptions: PanelTypeOptions;
  layout: PanelLayout;
  source: { origin: PanelSourceOrigin };
};

/** 高版本 spec 原样保留，仅供只读展示（不渲染、不编辑）。 */
export type UnsupportedPanel = {
  id: string;
  title: string;
  specVersion: number;
};

export type PanelValidationError = { path: string; message: string };
export type PanelValidationResult =
  { ok: true; spec: PanelSpec } | { ok: false; errors: PanelValidationError[] };

/**
 * 目录 DTO（`get_panel_catalog`，单一事实来源，Rust 原始形状）。
 * `params` 同时包含查询级参数与 `typeOptions` 参数；`fields` 列出该类型接受的 `typeOptions` 键。
 */
export type PanelParamKindDto = "enum" | "int" | "intArray" | "bool" | "string";
export type PanelCatalogParamDto = {
  name: string;
  kind: string;
  required: boolean;
  min: number | null;
  max: number | null;
  values: string[];
};
export type PanelCatalogTypeDto = {
  type: PanelType;
  timeBases: PanelTimeBasis[];
  defaultTimeBasis: PanelTimeBasis;
  params: PanelCatalogParamDto[];
  fields: string[];
  query: string;
};
export type PanelGlobalLimitsDto = {
  titleMaxLen: number;
  idMaxLen: number;
  colSpans: number[];
  rowSpans: number[];
  orderMax: number;
  stringMaxLen: number;
  dayCountMin: number;
  dayCountMax: number;
  calendarDayCountMax: number;
  dayBoundaryMin: number;
  dayBoundaryMax: number;
  hourlyBoundaryMax: number;
  rankingLimitMin: number;
  rankingLimitMax: number;
  rankingLimitDefault: number;
  sessionLimitMin: number;
  sessionLimitMax: number;
  sessionLimitDefault: number;
  sessionOffsetMax: number;
};
export type PanelCatalogDto = {
  catalogVersion: number;
  panelTypes: PanelCatalogTypeDto[];
  globalLimits: PanelGlobalLimitsDto;
};

/** 编辑器使用的目录形状：`params` 已收敛为可渲染的 `typeOptions` 控件。 */
export type PanelParamKind = "number" | "select" | "boolean" | "string";
export type PanelCatalogParam = {
  name: string;
  kind: PanelParamKind;
  required: boolean;
  min: number | null;
  max: number | null;
  values: string[] | null;
};
export type PanelCatalogType = {
  type: PanelType;
  timeBases: PanelTimeBasis[];
  defaultTimeBasis: PanelTimeBasis;
  params: PanelCatalogParam[];
  fields: string[];
};
export type PanelGlobalLimits = PanelGlobalLimitsDto;
export type PanelCatalog = {
  catalogVersion: number;
  panelTypes: PanelCatalogType[];
  globalLimits: PanelGlobalLimits;
};

const PARAM_KIND_MAP: Record<string, PanelParamKind | null> = {
  enum: "select",
  int: "number",
  bool: "boolean",
  string: "string",
  intArray: null,
};

function adaptParam(dto: PanelCatalogParamDto): PanelCatalogParam | null {
  const kind = PARAM_KIND_MAP[dto.kind] ?? null;
  if (kind === null) return null;
  return {
    name: dto.name,
    kind,
    required: dto.required === true,
    min: typeof dto.min === "number" ? dto.min : null,
    max: typeof dto.max === "number" ? dto.max : null,
    values: Array.isArray(dto.values) ? [...dto.values] : null,
  };
}

function adaptCatalogType(dto: PanelCatalogTypeDto): PanelCatalogType {
  const fields = Array.isArray(dto.fields) ? [...dto.fields] : [];
  const params =
    Array.isArray(dto.params) && fields.length > 0
      ? dto.params
          .filter((param) => fields.includes(param.name))
          .map(adaptParam)
          .filter((param): param is PanelCatalogParam => param !== null)
      : [];
  return {
    type: dto.type,
    timeBases: Array.isArray(dto.timeBases) ? [...dto.timeBases] : [],
    defaultTimeBasis: dto.defaultTimeBasis,
    params,
    fields,
  };
}

/**
 * 目录适配层：把 Rust 原始 DTO 收敛为编辑器可用的形状。
 * `enum→select`、`int→number`、`bool→boolean`、`intArray→跳过`；只保留属于 `fields`
 * （即 `typeOptions`）的参数，避免把查询级参数写入规格而保存失败。
 */
export function adaptPanelCatalog(value: unknown): PanelCatalog {
  if (typeof value !== "object" || value === null) return FALLBACK_CATALOG;
  const dto = value as Partial<PanelCatalogDto>;
  if (!Array.isArray(dto.panelTypes) || typeof dto.globalLimits !== "object") {
    return FALLBACK_CATALOG;
  }
  return {
    catalogVersion: typeof dto.catalogVersion === "number" ? dto.catalogVersion : 0,
    panelTypes: dto.panelTypes.map(adaptCatalogType),
    globalLimits: { ...FALLBACK_CATALOG.globalLimits, ...dto.globalLimits },
  };
}

/** `run_panel_query` 派生参数（与 Rust `panels::query::PanelQueryArgs` 对应）。 */
export type PanelQueryArgs = {
  fromMs: number | null;
  toMs: number | null;
  todayFromMs?: number;
  dayStartMs?: number;
  dayCount?: number;
  dayBoundaries?: number[];
  hourlyBoundaries?: number[];
  limit?: number;
  offset?: number;
};

/** 查询数据的运行时输入（页面范围 + 当前时钟）。 */
export type PanelRangeInput = {
  range: DateRange;
  firstEventMs: number | null;
  nowMs: number;
};

export type PanelRenderContext = PanelRangeInput & {
  refreshToken: number;
  onOpenSessions: (filters?: Partial<SessionFilters>, range?: DateRange) => void;
};

const PRESET_KEYS: ReadonlySet<string> = new Set(RANGE_PRESETS.map((preset) => preset.key));
const CLIENT_KEY_SET: ReadonlySet<string> = new Set(CLIENT_KEYS);
const SET_BY_KIND: Record<PanelRangeKind, ReadonlySet<string>> = {
  followPage: new Set(["kind"]),
  preset: new Set(["kind", "key"]),
  custom: new Set(["kind", "fromMs", "toMs"]),
};
const RANGE_KINDS = Object.keys(SET_BY_KIND) as PanelRangeKind[];
const METRICS: ReadonlySet<string> = new Set(["tokens", "requests"]);
const DIMENSIONS: ReadonlySet<string> = new Set(["model", "project"]);
const SORTS: ReadonlySet<string> = new Set(["newest", "oldest", "tokens_desc", "requests_desc"]);
const STACKS: ReadonlySet<string> = new Set(["client", "tokens"]);
const STAT_FIELDS: ReadonlySet<string> = new Set([
  "rangeTotal",
  "todayTotal",
  "sessions",
  "messages",
]);
const ORIGINS: ReadonlySet<string> = new Set(["manual", "ai"]);
const TIME_BASES: ReadonlySet<string> = new Set(["session", "request"]);
const TYPE_SET: ReadonlySet<string> = new Set(PANEL_TYPES);

const SPEC_KEYS = new Set([
  "specVersion",
  "id",
  "type",
  "title",
  "query",
  "typeOptions",
  "layout",
  "source",
]);
const QUERY_KEYS = new Set(["range", "timeBasis", "client"]);
const LAYOUT_KEYS = new Set(["colSpan", "rowSpan", "order"]);
const SOURCE_KEYS = new Set(["origin"]);
const TYPE_OPTION_KEYS = new Set([
  "metric",
  "dimension",
  "limit",
  "sort",
  "requestTime",
  "stack",
  "fields",
  "shortenKeys",
  "search",
  "model",
  "projectPath",
  "field",
]);
/** 每个类型接受的 typeOptions 键（与 Rust `panels::spec::validate_options` 一致）。 */
const TYPE_OPTION_FIELDS: Record<PanelType, ReadonlySet<string>> = {
  stat: new Set(["field"]),
  token_trend: new Set(["stack"]),
  request_trend: new Set(["metric"]),
  client_distribution: new Set(),
  ranking: new Set(["dimension", "limit", "shortenKeys"]),
  session_table: new Set(["limit", "sort", "requestTime", "search", "model", "projectPath"]),
  activity_calendar: new Set(),
};
/** 保留字段：任意类型均可携带（规格 ② 声明，P1 未使用）。 */
const RESERVED_OPTION_KEYS = new Set(["fields"]);

export const DEFAULT_PANEL_TITLES: Record<PanelType, string> = {
  stat: "指标",
  token_trend: "每日 Token 趋势",
  request_trend: "请求活动",
  client_distribution: "客户端用量分布",
  ranking: "排行",
  session_table: "会话列表",
  activity_calendar: "活动日历",
};

const DEFAULT_TIME_BASIS: Record<PanelType, PanelTimeBasis> = {
  stat: "session",
  token_trend: "session",
  request_trend: "request",
  client_distribution: "session",
  ranking: "session",
  session_table: "session",
  activity_calendar: "session",
};

const DEFAULT_COL_SPAN: Record<PanelType, PanelColSpan> = {
  stat: 3,
  token_trend: 6,
  request_trend: 12,
  client_distribution: 6,
  ranking: 6,
  session_table: 12,
  activity_calendar: 6,
};

const DEFAULT_TYPE_OPTIONS: Record<PanelType, PanelTypeOptions> = {
  stat: { field: "rangeTotal" },
  token_trend: { stack: "client" },
  request_trend: { metric: "tokens" },
  client_distribution: {},
  ranking: { dimension: "model", limit: PANEL_RANKING_LIMIT_DEFAULT, shortenKeys: false },
  session_table: {
    limit: PANEL_SESSION_LIMIT_DEFAULT,
    sort: "newest",
    requestTime: false,
    search: null,
    model: null,
    projectPath: null,
  },
  activity_calendar: {},
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && Number.isInteger(value);
}

function collectUnknownKeys(
  value: Record<string, unknown>,
  allowed: ReadonlySet<string>,
): string[] {
  return Object.keys(value).filter((key) => !allowed.has(key));
}

/** 从任意输入中读取 `specVersion`；缺失或非正整数返回 null。 */
export function readPanelSpecVersion(value: unknown): number | null {
  if (!isRecord(value)) return null;
  const version = value.specVersion;
  return isInteger(version) && version > 0 ? version : null;
}

/** 面板类型是否在当前已知目录中。 */
export function isPanelType(value: unknown): value is PanelType {
  return typeof value === "string" && TYPE_SET.has(value);
}

/**
 * AI 面板生成是否已就绪：Base URL、模型与密钥三者齐备才算已配置；
 * 未配置时生成入口保持禁用并引导去设置页（默认离线，不配置不发请求）。
 */
export function isPanelAiConfigured(
  config: { baseUrl: string | null; model: string | null; hasKey: boolean } | null,
): boolean {
  return (
    config !== null &&
    typeof config.baseUrl === "string" &&
    config.baseUrl.length > 0 &&
    typeof config.model === "string" &&
    config.model.length > 0 &&
    config.hasKey
  );
}

export function panelTypeLabel(type: PanelType): string {
  return DEFAULT_PANEL_TITLES[type];
}

export function panelTitle(spec: PanelSpec): string {
  const title = spec.title?.trim();
  return title && title.length > 0 ? title : DEFAULT_PANEL_TITLES[spec.type];
}

export function defaultTimeBasis(type: PanelType): PanelTimeBasis {
  return DEFAULT_TIME_BASIS[type];
}

export function resolveTimeBasis(spec: PanelSpec): PanelTimeBasis {
  return spec.query.timeBasis ?? DEFAULT_TIME_BASIS[spec.type];
}

export function defaultColSpan(type: PanelType): PanelColSpan {
  return DEFAULT_COL_SPAN[type];
}

export function defaultTypeOptions(type: PanelType): PanelTypeOptions {
  return { ...DEFAULT_TYPE_OPTIONS[type] };
}

/** 目录不可用时使用的内置白名单（与 Rust 目录同形），供编辑器渲染合法控件。 */
export const FALLBACK_CATALOG: PanelCatalog = {
  catalogVersion: 0,
  panelTypes: [
    {
      type: "stat",
      timeBases: ["session", "request"],
      defaultTimeBasis: "session",
      fields: ["field"],
      params: [
        {
          name: "field",
          kind: "select",
          required: false,
          min: null,
          max: null,
          values: ["rangeTotal", "todayTotal", "sessions", "messages"],
        },
      ],
    },
    {
      type: "token_trend",
      timeBases: ["session"],
      defaultTimeBasis: "session",
      fields: ["stack"],
      params: [
        {
          name: "stack",
          kind: "select",
          required: false,
          min: null,
          max: null,
          values: ["tokens", "client"],
        },
      ],
    },
    {
      type: "request_trend",
      timeBases: ["request"],
      defaultTimeBasis: "request",
      fields: ["metric"],
      params: [
        {
          name: "metric",
          kind: "select",
          required: false,
          min: null,
          max: null,
          values: ["tokens", "requests"],
        },
      ],
    },
    {
      type: "client_distribution",
      timeBases: ["session", "request"],
      defaultTimeBasis: "session",
      fields: [],
      params: [],
    },
    {
      type: "ranking",
      timeBases: ["session", "request"],
      defaultTimeBasis: "session",
      fields: ["dimension", "limit", "shortenKeys"],
      params: [
        {
          name: "dimension",
          kind: "select",
          required: false,
          min: null,
          max: null,
          values: ["model", "project"],
        },
        {
          name: "limit",
          kind: "number",
          required: false,
          min: PANEL_RANKING_LIMIT_MIN,
          max: PANEL_RANKING_LIMIT_MAX,
          values: null,
        },
        {
          name: "shortenKeys",
          kind: "boolean",
          required: false,
          min: null,
          max: null,
          values: null,
        },
      ],
    },
    {
      type: "session_table",
      timeBases: ["session", "request"],
      defaultTimeBasis: "session",
      fields: ["limit", "sort", "requestTime", "search", "model", "projectPath"],
      params: [
        {
          name: "limit",
          kind: "number",
          required: false,
          min: PANEL_SESSION_LIMIT_MIN,
          max: PANEL_SESSION_LIMIT_MAX,
          values: null,
        },
        {
          name: "sort",
          kind: "select",
          required: false,
          min: null,
          max: null,
          values: ["newest", "oldest", "tokens_desc", "requests_desc"],
        },
        {
          name: "requestTime",
          kind: "boolean",
          required: false,
          min: null,
          max: null,
          values: null,
        },
        { name: "search", kind: "string", required: false, min: null, max: 200, values: null },
        { name: "model", kind: "string", required: false, min: null, max: 200, values: null },
        { name: "projectPath", kind: "string", required: false, min: null, max: 200, values: null },
      ],
    },
    {
      type: "activity_calendar",
      timeBases: ["session"],
      defaultTimeBasis: "session",
      fields: [],
      params: [],
    },
  ],
  globalLimits: {
    titleMaxLen: PANEL_MAX_TITLE,
    idMaxLen: PANEL_MAX_ID,
    colSpans: [3, 4, 6, 12],
    rowSpans: [1, 2],
    orderMax: PANEL_MAX_ORDER,
    stringMaxLen: PANEL_MAX_TEXT,
    dayCountMin: 1,
    dayCountMax: 400,
    calendarDayCountMax: 365,
    dayBoundaryMin: 2,
    dayBoundaryMax: 401,
    hourlyBoundaryMax: 27,
    rankingLimitMin: PANEL_RANKING_LIMIT_MIN,
    rankingLimitMax: PANEL_RANKING_LIMIT_MAX,
    rankingLimitDefault: PANEL_RANKING_LIMIT_DEFAULT,
    sessionLimitMin: PANEL_SESSION_LIMIT_MIN,
    sessionLimitMax: PANEL_SESSION_LIMIT_MAX,
    sessionLimitDefault: PANEL_SESSION_LIMIT_DEFAULT,
    sessionOffsetMax: 1_000_000,
  },
};

export function createPanelId(): string {
  const cryptoApi = globalThis.crypto;
  if (cryptoApi && typeof cryptoApi.randomUUID === "function") {
    return `p_${cryptoApi.randomUUID().replace(/-/g, "").slice(0, 8)}`;
  }
  return `p_${Math.random().toString(36).slice(2, 10)}`;
}

/** 稳定序列化（键排序、忽略 undefined），用于查询 key/scope 签名。 */
export function stableStringify(value: unknown): string {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value) ?? "null";
  }
  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(",")}]`;
  }
  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([, entry]) => entry !== undefined)
    .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0));
  return `{${entries
    .map(([key, entry]) => `${JSON.stringify(key)}:${stableStringify(entry)}`)
    .join(",")}}`;
}

/** 解析面板使用的日期范围；`followPage` 直接复用页面范围。 */
export function resolvePanelRange(spec: PanelSpec, input: PanelRangeInput): DateRange {
  const range = spec.query.range;
  if (range.kind === "followPage") return input.range;
  if (range.kind === "preset") {
    return resolveRange(range.key, input.firstEventMs, new Date(input.nowMs));
  }
  try {
    return createCustomRange(range.fromMs, range.toMs);
  } catch {
    return input.range;
  }
}

/** 仅口径（类型 / 范围 / 时间基准 / 客户端）变化时才隐藏旧数据；typeOptions 变化走后台刷新。 */
export function panelQueryScope(spec: PanelSpec, input: PanelRangeInput): string {
  const range = resolvePanelRange(spec, input);
  return stableStringify({
    type: spec.type,
    range: rangeScope(range),
    timeBasis: resolveTimeBasis(spec),
    client: spec.query.client,
  });
}

/** 由解析后的范围推导 `run_panel_query` 的派生参数。 */
export function buildPanelQueryArgs(spec: PanelSpec, input: PanelRangeInput): PanelQueryArgs {
  const range = resolvePanelRange(spec, input);
  const args: PanelQueryArgs = { fromMs: range.fromMs, toMs: range.toMs };
  if (spec.type === "token_trend" || spec.type === "activity_calendar") {
    args.dayStartMs = range.dayStartMs;
    args.dayCount =
      spec.type === "activity_calendar" ? Math.min(range.dayCount, 365) : range.dayCount;
  }
  if (spec.type === "request_trend") {
    args.dayBoundaries = localDayBoundaries(range);
    args.hourlyBoundaries = localHourBoundaries(range);
  }
  if (spec.type === "stat") {
    args.todayFromMs = startOfDay(new Date(input.nowMs)).getTime();
  }
  if (spec.type === "ranking") {
    args.limit = spec.typeOptions.limit ?? PANEL_RANKING_LIMIT_DEFAULT;
  }
  if (spec.type === "session_table") {
    args.limit = spec.typeOptions.limit ?? PANEL_SESSION_LIMIT_DEFAULT;
    args.offset = 0;
  }
  return args;
}

function fail(path: string, message: string): PanelValidationResult {
  return { ok: false, errors: [{ path, message }] };
}

function validateRange(value: unknown, errors: PanelValidationError[]): PanelRange | null {
  if (!isRecord(value)) {
    errors.push({ path: "query.range", message: "缺少范围定义" });
    return null;
  }
  const kind = value.kind;
  if (typeof kind !== "string" || !RANGE_KINDS.includes(kind as PanelRangeKind)) {
    errors.push({ path: "query.range.kind", message: "未知的范围类型" });
    return null;
  }
  const typedKind = kind as PanelRangeKind;
  for (const key of collectUnknownKeys(value, SET_BY_KIND[typedKind])) {
    errors.push({ path: `query.range.${key}`, message: "不支持的字段" });
  }
  if (typedKind === "followPage") return { kind: "followPage" };
  if (typedKind === "preset") {
    const key = value.key;
    if (typeof key !== "string" || !PRESET_KEYS.has(key)) {
      errors.push({ path: "query.range.key", message: "未知的预设范围" });
      return null;
    }
    return { kind: "preset", key: key as PanelPresetKey };
  }
  const fromMs = value.fromMs;
  const toMs = value.toMs;
  if (!isInteger(fromMs) || !isInteger(toMs)) {
    errors.push({ path: "query.range", message: "自定义范围需要有效的起止时间" });
    return null;
  }
  if (fromMs >= toMs) {
    errors.push({ path: "query.range", message: "起始时间必须早于结束时间" });
    return null;
  }
  return { kind: "custom", fromMs, toMs };
}

function limitBounds(type: PanelType): { min: number; max: number } | null {
  if (type === "ranking") return { min: PANEL_RANKING_LIMIT_MIN, max: PANEL_RANKING_LIMIT_MAX };
  if (type === "session_table")
    return { min: PANEL_SESSION_LIMIT_MIN, max: PANEL_SESSION_LIMIT_MAX };
  return null;
}

function validateTypeOptions(
  type: PanelType,
  value: unknown,
  errors: PanelValidationError[],
): PanelTypeOptions | null {
  if (value === undefined) return {};
  if (!isRecord(value)) {
    errors.push({ path: "typeOptions", message: "类型参数必须是对象" });
    return null;
  }
  for (const key of collectUnknownKeys(value, TYPE_OPTION_KEYS)) {
    errors.push({ path: `typeOptions.${key}`, message: "不支持的字段" });
  }
  const allowed = TYPE_OPTION_FIELDS[type];
  for (const key of Object.keys(value)) {
    if (TYPE_OPTION_KEYS.has(key) && !allowed.has(key) && !RESERVED_OPTION_KEYS.has(key)) {
      errors.push({ path: `typeOptions.${key}`, message: "该面板类型不支持此参数" });
    }
  }
  const options: PanelTypeOptions = {};
  const metric = value.metric;
  if (metric !== undefined) {
    if (typeof metric !== "string" || !METRICS.has(metric)) {
      errors.push({ path: "typeOptions.metric", message: "未知的指标" });
    } else {
      options.metric = metric as PanelMetric;
    }
  }
  const dimension = value.dimension;
  if (dimension !== undefined) {
    if (typeof dimension !== "string" || !DIMENSIONS.has(dimension)) {
      errors.push({ path: "typeOptions.dimension", message: "未知的维度" });
    } else {
      options.dimension = dimension as PanelDimension;
    }
  }
  const limit = value.limit;
  if (limit !== undefined) {
    const bounds = limitBounds(type) ?? { min: 1, max: PANEL_SESSION_LIMIT_MAX };
    if (!isInteger(limit) || limit < bounds.min || limit > bounds.max) {
      errors.push({
        path: "typeOptions.limit",
        message: `数量上限需在 ${bounds.min}–${bounds.max} 之间`,
      });
    } else {
      options.limit = limit;
    }
  }
  const sort = value.sort;
  if (sort !== undefined) {
    if (typeof sort !== "string" || !SORTS.has(sort)) {
      errors.push({ path: "typeOptions.sort", message: "未知的排序方式" });
    } else {
      options.sort = sort as PanelSort;
    }
  }
  const stack = value.stack;
  if (stack !== undefined) {
    if (typeof stack !== "string" || !STACKS.has(stack)) {
      errors.push({ path: "typeOptions.stack", message: "未知的趋势模式" });
    } else {
      options.stack = stack as PanelStack;
    }
  }
  const field = value.field;
  if (field !== undefined) {
    if (typeof field !== "string" || !STAT_FIELDS.has(field)) {
      errors.push({ path: "typeOptions.field", message: "未知的指标字段" });
    } else {
      options.field = field as PanelStatField;
    }
  }
  for (const flag of ["requestTime", "shortenKeys"] as const) {
    const entry = value[flag];
    if (entry !== undefined) {
      if (typeof entry !== "boolean") {
        errors.push({ path: `typeOptions.${flag}`, message: "必须是布尔值" });
      } else {
        options[flag] = entry;
      }
    }
  }
  for (const text of ["search", "model", "projectPath"] as const) {
    const entry = value[text];
    if (entry !== undefined && entry !== null) {
      if (typeof entry !== "string" || entry.length > PANEL_MAX_TEXT) {
        errors.push({
          path: `typeOptions.${text}`,
          message: `文本长度需在 ${PANEL_MAX_TEXT} 以内`,
        });
      } else {
        options[text] = entry;
      }
    } else if (entry === null) {
      options[text] = null;
    }
  }
  const fields = value.fields;
  if (fields !== undefined) {
    if (!Array.isArray(fields) || fields.some((entry) => typeof entry !== "string")) {
      errors.push({ path: "typeOptions.fields", message: "字段列表必须是字符串数组" });
    } else {
      options.fields = fields as string[];
    }
  }
  return options;
}

/**
 * 前端镜像校验：白名单 + 枚举 + 数值边界，默认拒绝未知字段。
 * 只接受 `PANEL_SPEC_VERSION`；高版本由调用方识别为只读。
 */
export function validatePanelSpec(value: unknown): PanelValidationResult {
  if (!isRecord(value)) return fail("", "面板定义必须是对象");
  const errors: PanelValidationError[] = [];
  for (const key of collectUnknownKeys(value, SPEC_KEYS)) {
    errors.push({ path: key, message: "不支持的字段" });
  }
  const specVersion = value.specVersion;
  if (specVersion !== PANEL_SPEC_VERSION) {
    errors.push({ path: "specVersion", message: `仅支持版本 ${PANEL_SPEC_VERSION}` });
  }
  const id = value.id;
  if (typeof id !== "string" || id.length === 0 || id.length > PANEL_MAX_ID) {
    errors.push({ path: "id", message: "面板标识无效" });
  }
  const type = value.type;
  if (!isPanelType(type)) {
    errors.push({ path: "type", message: "未知的面板类型" });
  }
  const title = value.title;
  if (title !== undefined) {
    if (typeof title !== "string" || title.length > PANEL_MAX_TITLE) {
      errors.push({ path: "title", message: `标题长度需在 ${PANEL_MAX_TITLE} 以内` });
    }
  }
  const queryRaw = value.query;
  if (!isRecord(queryRaw)) {
    errors.push({ path: "query", message: "缺少查询定义" });
  } else {
    for (const key of collectUnknownKeys(queryRaw, QUERY_KEYS)) {
      errors.push({ path: `query.${key}`, message: "不支持的字段" });
    }
  }
  const range = isRecord(queryRaw) ? validateRange(queryRaw.range, errors) : null;
  let timeBasis: PanelTimeBasis | undefined;
  if (isRecord(queryRaw) && queryRaw.timeBasis !== undefined) {
    if (typeof queryRaw.timeBasis !== "string" || !TIME_BASES.has(queryRaw.timeBasis)) {
      errors.push({ path: "query.timeBasis", message: "未知的时间口径" });
    } else {
      timeBasis = queryRaw.timeBasis as PanelTimeBasis;
    }
  }
  let client: ClientKey | null = null;
  if (isRecord(queryRaw) && queryRaw.client !== undefined && queryRaw.client !== null) {
    if (typeof queryRaw.client !== "string" || !CLIENT_KEY_SET.has(queryRaw.client)) {
      errors.push({ path: "query.client", message: "未知的客户端" });
    } else {
      client = queryRaw.client as ClientKey;
    }
  }
  const typeOptions = validateTypeOptions(
    isPanelType(type) ? type : "stat",
    value.typeOptions,
    errors,
  );
  const layoutRaw = value.layout;
  let colSpan: PanelColSpan | null = null;
  let rowSpan: PanelRowSpan | null = null;
  let order = 0;
  if (!isRecord(layoutRaw)) {
    errors.push({ path: "layout", message: "缺少布局定义" });
  } else {
    for (const key of collectUnknownKeys(layoutRaw, LAYOUT_KEYS)) {
      errors.push({ path: `layout.${key}`, message: "不支持的字段" });
    }
    if (!isInteger(layoutRaw.colSpan) || !PANEL_COL_SPANS.includes(layoutRaw.colSpan as 3)) {
      errors.push({ path: "layout.colSpan", message: "宽度只能是 3 / 4 / 6 / 12" });
    } else {
      colSpan = layoutRaw.colSpan as PanelColSpan;
    }
    if (!isInteger(layoutRaw.rowSpan) || !PANEL_ROW_SPANS.includes(layoutRaw.rowSpan as 1)) {
      errors.push({ path: "layout.rowSpan", message: "高度只能是 1 / 2" });
    } else {
      rowSpan = layoutRaw.rowSpan as PanelRowSpan;
    }
    if (!isInteger(layoutRaw.order) || layoutRaw.order < 0 || layoutRaw.order > PANEL_MAX_ORDER) {
      errors.push({ path: "layout.order", message: `顺序需在 0–${PANEL_MAX_ORDER} 之间` });
    } else {
      order = layoutRaw.order;
    }
  }
  const sourceRaw = value.source;
  let origin: PanelSourceOrigin = "manual";
  if (!isRecord(sourceRaw)) {
    errors.push({ path: "source", message: "缺少来源定义" });
  } else {
    for (const key of collectUnknownKeys(sourceRaw, SOURCE_KEYS)) {
      errors.push({ path: `source.${key}`, message: "不支持的字段" });
    }
    if (typeof sourceRaw.origin !== "string" || !ORIGINS.has(sourceRaw.origin)) {
      errors.push({ path: "source.origin", message: "未知的来源" });
    } else {
      origin = sourceRaw.origin as PanelSourceOrigin;
    }
  }

  if (
    errors.length > 0 ||
    !range ||
    !typeOptions ||
    colSpan === null ||
    rowSpan === null ||
    !isPanelType(type) ||
    typeof id !== "string"
  ) {
    return { ok: false, errors };
  }

  const spec: PanelSpec = {
    specVersion: PANEL_SPEC_VERSION,
    id,
    type,
    query: { range, client, ...(timeBasis ? { timeBasis } : {}) },
    typeOptions,
    layout: { colSpan, rowSpan, order },
    source: { origin },
  };
  if (typeof title === "string" && title.length > 0) spec.title = title;
  return { ok: true, spec };
}

/** 用目录与默认值构造一个新面板；`order` 由调用方按现有数量给出。 */
export function createDefaultPanelSpec(
  catalog: PanelCatalog | null,
  type: PanelType,
  order: number,
): PanelSpec {
  const info = catalog?.panelTypes.find((entry) => entry.type === type);
  return {
    specVersion: PANEL_SPEC_VERSION,
    id: createPanelId(),
    type,
    query: {
      range: { kind: "followPage" },
      timeBasis: info?.defaultTimeBasis ?? DEFAULT_TIME_BASIS[type],
      client: null,
    },
    typeOptions: { ...DEFAULT_TYPE_OPTIONS[type] },
    layout: { colSpan: DEFAULT_COL_SPAN[type], rowSpan: 1, order },
    source: { origin: "manual" },
  };
}
