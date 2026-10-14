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
export const PANEL_MIN_LIMIT = 1;
export const PANEL_MAX_LIMIT = 100;
export const PANEL_MAX_ORDER = 63;
export const PANEL_MAX_TITLE = 80;
export const PANEL_MAX_TEXT = 200;

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

/** 目录 DTO（`get_panel_catalog`，单一事实来源）。 */
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
  params: PanelCatalogParam[];
  fields: string[];
};
export type PanelGlobalLimits = {
  colSpans: number[];
  rowSpans: number[];
  maxTitle: number;
  minLimit: number;
  maxLimit: number;
  maxOrder: number;
};
export type PanelCatalog = {
  catalogVersion: number;
  panelTypes: PanelCatalogType[];
  globalLimits: PanelGlobalLimits;
};

/** `run_panel_query` 派生参数（由前端按 `lib/ranges.ts` 计算，Rust 校验后分发）。 */
export type PanelQueryArgs = {
  fromMs: number | null;
  toMs: number | null;
  dayStartMs?: number;
  dayCount?: number;
  dayBoundaries?: number[];
  hourlyBoundaries?: number[];
  todayFromMs?: number;
  limit?: number;
  offset?: number;
  groupBy?: PanelDimension;
  sort?: PanelSort;
  search?: string | null;
  model?: string | null;
  projectPath?: string | null;
  requestTime?: boolean;
};

export const PANEL_DEFAULT_RANKING_LIMIT = 8;
export const PANEL_DEFAULT_SESSION_LIMIT = 20;

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
  ranking: { dimension: "model", limit: 8, shortenKeys: false },
  session_table: {
    limit: 20,
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

/** 目录不可用时使用的内置白名单，供编辑器渲染合法控件（与 ⑦ 校验一致）。 */
export const FALLBACK_CATALOG: PanelCatalog = {
  catalogVersion: 0,
  panelTypes: [
    {
      type: "stat",
      timeBases: ["session", "request"],
      fields: ["rangeTotal", "todayTotal", "sessions", "messages"],
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
      fields: [],
      params: [
        {
          name: "stack",
          kind: "select",
          required: false,
          min: null,
          max: null,
          values: ["client", "tokens"],
        },
      ],
    },
    {
      type: "request_trend",
      timeBases: ["request"],
      fields: [],
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
    { type: "client_distribution", timeBases: ["session", "request"], fields: [], params: [] },
    {
      type: "ranking",
      timeBases: ["session", "request"],
      fields: [],
      params: [
        {
          name: "dimension",
          kind: "select",
          required: false,
          min: null,
          max: null,
          values: ["model", "project"],
        },
        { name: "limit", kind: "number", required: false, min: 1, max: 100, values: null },
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
      fields: [],
      params: [
        { name: "limit", kind: "number", required: false, min: 1, max: 100, values: null },
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
    { type: "activity_calendar", timeBases: ["session"], fields: [], params: [] },
  ],
  globalLimits: {
    colSpans: [3, 4, 6, 12],
    rowSpans: [1, 2],
    maxTitle: PANEL_MAX_TITLE,
    minLimit: PANEL_MIN_LIMIT,
    maxLimit: PANEL_MAX_LIMIT,
    maxOrder: PANEL_MAX_ORDER,
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
    args.limit = spec.typeOptions.limit ?? PANEL_DEFAULT_RANKING_LIMIT;
    args.groupBy = spec.typeOptions.dimension ?? "model";
  }
  if (spec.type === "session_table") {
    args.limit = spec.typeOptions.limit ?? PANEL_DEFAULT_SESSION_LIMIT;
    args.offset = 0;
    args.sort = spec.typeOptions.sort ?? "newest";
    args.search = spec.typeOptions.search ?? null;
    args.model = spec.typeOptions.model ?? null;
    args.projectPath = spec.typeOptions.projectPath ?? null;
    args.requestTime = spec.typeOptions.requestTime ?? false;
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

function validateTypeOptions(
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
    if (!isInteger(limit) || limit < PANEL_MIN_LIMIT || limit > PANEL_MAX_LIMIT) {
      errors.push({
        path: "typeOptions.limit",
        message: `数量上限需在 ${PANEL_MIN_LIMIT}–${PANEL_MAX_LIMIT} 之间`,
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
  if (typeof id !== "string" || id.length === 0) {
    errors.push({ path: "id", message: "缺少面板标识" });
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
  const typeOptions = validateTypeOptions(value.typeOptions, errors);
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
      timeBasis: info?.timeBases[0] ?? DEFAULT_TIME_BASIS[type],
      client: null,
    },
    typeOptions: { ...DEFAULT_TYPE_OPTIONS[type] },
    layout: { colSpan: DEFAULT_COL_SPAN[type], rowSpan: 1, order },
    source: { origin: "manual" },
  };
}
