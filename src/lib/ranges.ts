/** 日期范围预设：边界由前端按本地日历计算，查询与聚合仍在 Rust 侧完成。 */

import { addDays, differenceInCalendarDays, startOfDay } from "date-fns";

export type RangeKey = "today" | "7d" | "30d" | "90d" | "all";

export type DateRange = {
  key: RangeKey;
  label: string;
  /** 区间起点（本地零点），null 表示不限起点。 */
  fromMs: number | null;
  /** 区间终点（本地次日零点，排他）。 */
  toMs: number;
  /** 趋势图需要的天数。 */
  dayCount: number;
};

export const RANGE_OPTIONS: ReadonlyArray<{ key: RangeKey; label: string }> = [
  { key: "today", label: "今天" },
  { key: "7d", label: "最近 7 天" },
  { key: "30d", label: "最近 30 天" },
  { key: "90d", label: "最近 90 天" },
  { key: "all", label: "全部" },
];

const MS_PER_DAY = 86_400_000;

/** 把预设解析成具体区间；`firstEventMs` 用于"全部"的起点（没有数据时回落到今天）。 */
export function resolveRange(
  key: RangeKey,
  firstEventMs: number | null,
  now: Date = new Date(),
): DateRange {
  const todayStart = startOfDay(now).getTime();
  const tomorrowStart = addDays(startOfDay(now), 1).getTime();
  switch (key) {
    case "today":
      return { key, label: "今天", fromMs: todayStart, toMs: tomorrowStart, dayCount: 1 };
    case "7d":
      return {
        key,
        label: "最近 7 天",
        fromMs: addDays(startOfDay(now), -6).getTime(),
        toMs: tomorrowStart,
        dayCount: 7,
      };
    case "30d":
    case "90d":
      return {
        key,
        label: key === "90d" ? "最近 90 天" : "最近 30 天",
        fromMs: addDays(startOfDay(now), key === "90d" ? -89 : -29).getTime(),
        toMs: tomorrowStart,
        dayCount: key === "90d" ? 90 : 30,
      };
    case "all": {
      const start =
        firstEventMs === null ? todayStart : startOfDay(new Date(firstEventMs)).getTime();
      const days = Math.min(
        Math.max(
          differenceInCalendarDays(new Date(tomorrowStart - MS_PER_DAY), new Date(start)) + 1,
          1,
        ),
        400,
      );
      return { key, label: "全部", fromMs: start, toMs: tomorrowStart, dayCount: days };
    }
  }
}
