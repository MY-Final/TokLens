/** 展示层格式化：只做单位与显示处理，不改变统计口径。 */
import type { SyncState } from "@/lib/usage-types";
import type { TokenDisplay } from "@/lib/settings-types";

const NUMBER_FORMAT = new Intl.NumberFormat("zh-CN");

/** 精确数字：1,234,567。 */
export function formatNumber(value: number): string {
  return NUMBER_FORMAT.format(value);
}

/** 紧凑数字：12.3k / 1.2M（用于卡片与坐标轴）。 */
export function formatCompactTokens(value: number): string {
  const abs = Math.abs(value);
  if (abs >= 1_000_000_000) {
    return `${(value / 1_000_000_000).toFixed(2)}B`;
  }
  if (abs >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(2)}M`;
  }
  if (abs >= 1_000) {
    return `${(value / 1_000).toFixed(1)}k`;
  }
  return formatNumber(value);
}

export const TOKEN_DISPLAY_OPTIONS: { value: TokenDisplay; label: string }[] = [
  { value: "auto", label: "自动（k / M / B）" },
  { value: "chinese", label: "自动（万 / 亿）" },
  { value: "k", label: "千（k）" },
  { value: "m", label: "百万（M）" },
  { value: "b", label: "十亿（B）" },
  { value: "exact", label: "完整数字（千分位）" },
];

/** Only changes presentation; original Token counts remain available as exact values. */
export function formatTokenDisplay(value: number, display: TokenDisplay): string {
  if (display === "exact") return formatNumber(value);
  if (display === "auto") return formatCompactTokens(value);
  const abs = Math.abs(value);
  if (display === "chinese" && abs < 10_000) return formatNumber(value);
  const [divisor, suffix] =
    display === "chinese"
      ? abs >= 100_000_000
        ? ([100_000_000, "亿"] as const)
        : ([10_000, "万"] as const)
      : display === "k"
        ? ([1_000, "k"] as const)
        : display === "m"
          ? ([1_000_000, "M"] as const)
          : ([1_000_000_000, "B"] as const);
  if (value === 0) return `0 ${suffix}`;
  if (abs / divisor < 0.01) return `${value < 0 ? "−" : ""}<0.01 ${suffix}`;
  return `${(value / divisor).toFixed(2)} ${suffix}`;
}

/** 缺失值统一显示为 "—"（未知，不是 0）。 */
export function formatTokensOrDash(value: number | null): string {
  return value === null ? "—" : formatNumber(value);
}

export function formatCompactOrDash(value: number | null): string {
  return value === null ? "—" : formatCompactTokens(value);
}

/** 百分比：`part / whole`，两位小数以内的整数百分比。 */
export function formatPercent(part: number, whole: number): string {
  if (whole <= 0) {
    return "—";
  }
  const percent = (part / whole) * 100;
  return `${percent >= 10 ? percent.toFixed(0) : percent.toFixed(1)}%`;
}

function pad(value: number): string {
  return value.toString().padStart(2, "0");
}

/** 本地时间 2026-10-11 15:04。 */
export function formatDateTime(ms: number | null): string {
  if (ms === null) {
    return "—";
  }
  const date = new Date(ms);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** 本地日期 10-11（用于趋势坐标轴）。 */
export function formatMonthDay(ms: number): string {
  const date = new Date(ms);
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** 相对时间：刚刚 / N 分钟前 / N 小时前 / N 天前 / 具体日期。 */
export function formatRelative(ms: number | null): string {
  if (ms === null) {
    return "—";
  }
  const diff = Date.now() - ms;
  if (diff < 60_000) {
    return "刚刚";
  }
  if (diff < 3_600_000) {
    return `${Math.floor(diff / 60_000)} 分钟前`;
  }
  if (diff < 86_400_000) {
    return `${Math.floor(diff / 3_600_000)} 小时前`;
  }
  if (diff < 7 * 86_400_000) {
    return `${Math.floor(diff / 86_400_000)} 天前`;
  }
  return formatDateTime(ms);
}

/** 项目路径只显示最后两段，避免长路径撑破表格。 */
export function shortenPath(path: string | null): string {
  if (!path) {
    return "—";
  }
  const normalized = path.replace(/\\/g, "/").replace(/\/+$/, "");
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 2) {
    return normalized;
  }
  return `…/${parts.slice(-2).join("/")}`;
}

/** 同步状态文案（侧边栏与数据源页共用）。 */
export function syncStatusLabel(sync: SyncState | null | undefined): string {
  if (!sync) {
    return "尚未同步";
  }
  if (sync.running) {
    return "正在同步本地数据…";
  }
  if (sync.error) {
    return "上次同步失败";
  }
  if (sync.finishedAtMs) {
    return `上次同步 ${formatRelative(sync.finishedAtMs)}`;
  }
  return "尚未同步";
}
