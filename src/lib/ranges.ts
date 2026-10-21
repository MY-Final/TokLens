/** 日期范围边界按本地日历计算；查询与聚合仍在 Rust 侧完成。 */
import { addDays, differenceInCalendarDays, startOfDay, startOfMonth, startOfWeek } from "date-fns";

/** 持久化设置使用的预设，保持与 AppSettings 契约一致。 */
export type RangeKey = "today" | "7d" | "30d" | "90d" | "all";
export type RangePreset = RangeKey | "week" | "month";
export type DateRange = {
  key: RangePreset | "custom";
  label: string;
  /** 实际查询起点，可以包含时分；null 表示不限起点。 */
  fromMs: number | null;
  /** 实际查询终点，排他。 */
  toMs: number;
  /** 首个展示日桶的本地零点；长区间仅展示最近 400 天。 */
  dayStartMs: number;
  /** 趋势图展示的日桶数量，不超过 400。 */
  dayCount: number;
};
export const RANGE_OPTIONS: ReadonlyArray<{ key: RangeKey; label: string }> = [
  { key: "today", label: "今天" },
  { key: "7d", label: "最近 7 天" },
  { key: "30d", label: "最近 30 天" },
  { key: "90d", label: "最近 90 天" },
  { key: "all", label: "全部" },
];
export const RANGE_PRESETS: ReadonlyArray<{ key: RangePreset; label: string }> = [
  { key: "today", label: "今天" },
  { key: "week", label: "本周" },
  { key: "month", label: "本月" },
  ...RANGE_OPTIONS.slice(1),
];
/** 相对范围：以当前时刻为终点滚动计算。 */
export const RELATIVE_PRESETS: ReadonlyArray<{ key: RangeKey; label: string }> = RANGE_OPTIONS;
/** 自然周期：跟随本地日历的周与月。 */
export const NATURAL_PRESETS: ReadonlyArray<{ key: RangePreset; label: string }> = [
  { key: "week", label: "本周" },
  { key: "month", label: "本月" },
];
const MS_PER_MINUTE = 60_000;
/** User selection identity, excluding clock ticks within rolling presets. */
export function rangeScope(range: DateRange): string {
  return range.key === "custom"
    ? `custom:${range.fromMs}:${range.toMs}`
    : range.key === "all"
      ? "all"
      : `${range.key}:${range.fromMs}`;
}

/** Consecutive local calendar midnights; Rust assigns request timestamps to these buckets. */
export function localDayBoundaries(range: DateRange): number[] {
  return Array.from({ length: range.dayCount + 1 }, (_, index) =>
    addDays(range.dayStartMs, index).getTime(),
  );
}
/** Hour boundaries preserve repeated/skipped local hours across DST; the last bucket ends at the query cutoff. */
export function localHourBoundaries(range: DateRange): number[] {
  if (range.key !== "today" || range.fromMs === null) return [];
  const boundaries = [range.fromMs];
  const nextDay = addDays(startOfDay(range.fromMs), 1).getTime();
  const endMs = Math.min(range.toMs, nextDay);
  let offset = new Date(range.fromMs).getTimezoneOffset();
  // A transition can be a half hour; split at both local hour starts and offset changes.
  for (let at = range.fromMs + MS_PER_MINUTE; at < endMs; at += MS_PER_MINUTE) {
    const date = new Date(at);
    const nextOffset = date.getTimezoneOffset();
    if (date.getMinutes() === 0 || nextOffset !== offset) boundaries.push(at);
    offset = nextOffset;
  }
  if (endMs > range.fromMs) boundaries.push(endMs);
  return boundaries;
}
function displayDays(fromMs: number, toMs: number): Pick<DateRange, "dayStartMs" | "dayCount"> {
  const firstDay = startOfDay(fromMs);
  const lastDay = startOfDay(toMs - 1);
  const cappedFirstDay = addDays(lastDay, -399);
  const dayStartMs = Math.max(firstDay.getTime(), cappedFirstDay.getTime());
  return { dayStartMs, dayCount: Math.max(differenceInCalendarDays(lastDay, dayStartMs) + 1, 1) };
}
/** 预设包含当前显示的整分钟，结束边界为下一分钟（排他）。 */
export function resolveRange(
  key: RangePreset,
  firstEventMs: number | null,
  now: Date = new Date(),
): DateRange {
  const todayStart = startOfDay(now).getTime();
  const toMs = Math.floor(now.getTime() / MS_PER_MINUTE) * MS_PER_MINUTE + MS_PER_MINUTE;
  let fromMs = todayStart;
  switch (key) {
    case "7d":
      fromMs = addDays(todayStart, -6).getTime();
      break;
    case "30d":
      fromMs = addDays(todayStart, -29).getTime();
      break;
    case "90d":
      fromMs = addDays(todayStart, -89).getTime();
      break;
    case "week":
      fromMs = startOfWeek(now, { weekStartsOn: 1 }).getTime();
      break;
    case "month":
      fromMs = startOfMonth(now).getTime();
      break;
    case "all":
      fromMs = firstEventMs === null ? todayStart : startOfDay(firstEventMs).getTime();
      break;
    case "today":
      break;
  }
  return {
    key,
    label: RANGE_PRESETS.find((preset) => preset.key === key)?.label ?? key,
    fromMs,
    toMs,
    ...displayDays(fromMs, toMs),
  };
}
/**
 * 触发器摘要：同一天显示时刻区间，跨天显示日期区间，跨年补上年份。
 * 只做展示格式化，不改变范围本身的起止边界。
 */
export function rangeSpanLabel(range: DateRange): string {
  const from = range.fromMs ?? range.dayStartMs;
  const to = range.toMs - 1;
  const fromDate = new Date(from);
  const toDate = new Date(to);
  const sameDay = startOfDay(from).getTime() === startOfDay(to).getTime();
  if (sameDay) {
    return `${pad2(fromDate.getMonth() + 1)}-${pad2(fromDate.getDate())} ${pad2(fromDate.getHours())}:${pad2(fromDate.getMinutes())}–${pad2(toDate.getHours())}:${pad2(toDate.getMinutes())}`;
  }
  if (fromDate.getFullYear() === toDate.getFullYear()) {
    return `${pad2(fromDate.getMonth() + 1)}-${pad2(fromDate.getDate())}—${pad2(toDate.getMonth() + 1)}-${pad2(toDate.getDate())}`;
  }
  return `${fromDate.getFullYear()}-${pad2(fromDate.getMonth() + 1)}-${pad2(fromDate.getDate())}—${toDate.getFullYear()}-${pad2(toDate.getMonth() + 1)}-${pad2(toDate.getDate())}`;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

/**
 * 创建固定自定义区间；toMs 必须已是排他边界。
 * 无效日期或起点不早于终点时抛出 Error，调用方应展示错误并保留当前范围。
 */
export function createCustomRange(fromMs: number, toMs: number): DateRange {
  if (
    !Number.isFinite(fromMs) ||
    !Number.isFinite(toMs) ||
    Number.isNaN(new Date(fromMs).getTime()) ||
    Number.isNaN(new Date(toMs).getTime())
  ) {
    throw new Error("请输入有效的开始和结束日期时间");
  }
  if (fromMs >= toMs) {
    throw new Error("开始时间必须早于结束时间");
  }
  return {
    key: "custom",
    label: `${formatDateTimeInput(fromMs).replace("T", " ")} – ${formatDateTimeInput(toMs - 1).replace("T", " ")}`,
    fromMs,
    toMs,
    ...displayDays(fromMs, toMs),
  };
}
/** 本地时间的 datetime-local 输入值，不进行 UTC 转换。 */
export function formatDateTimeInput(ms: number): string {
  const date = new Date(ms);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${String(date.getFullYear()).padStart(4, "0")}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
/** 严格解析本地分钟值；拒绝日期溢出及夏令时跳过的不存在时间。 */
export function parseDateTimeInput(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/.test(value)) {
    return null;
  }
  const date = new Date(value);
  const ms = date.getTime();
  return Number.isFinite(ms) && formatDateTimeInput(ms) === value ? ms : null;
}
