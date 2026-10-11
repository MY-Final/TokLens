export type DailyRange = { fromMs: number | null; toMs: number };
export function dailyScope(ms: number, range?: DailyRange) {
  const start = new Date(ms);
  start.setHours(0, 0, 0, 0);
  const end = new Date(start);
  end.setDate(end.getDate() + 1);
  return range &&
    ((range.fromMs !== null && range.fromMs > start.getTime()) || range.toMs < end.getTime())
    ? "所选时段 · 本地日期"
    : "本地日期";
}
