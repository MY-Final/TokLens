import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { DataExplanation } from "@/components/DataExplanation";
import { UsageTooltip } from "@/components/UsageTooltip";
import { dailyScope, type DailyRange } from "@/lib/daily-tooltip";
import { DashboardCard } from "@/components/DashboardCard";
import { HeatmapCell, HeatmapLegend } from "@/components/HeatmapCell";
import { EmptyBlock, ErrorBlock, HeatmapSkeleton } from "@/components/StateBlocks";
import { useSyncing } from "@/hooks/use-sync-status";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { dayTotalLabel, formatNumber } from "@/lib/format";
import type { DailyTokenBucket } from "@/lib/usage-types";

type Metric = "tokens" | "sessions" | "messages";
const METRICS = {
  tokens: { label: "Token", unit: "token" },
  sessions: { label: "会话", unit: "个会话" },
  messages: { label: "请求明细", unit: "条请求明细" },
} as const;
const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];

function dateLabel(ms: number): string {
  const date = new Date(ms);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function metricValue(bucket: DailyTokenBucket | undefined, metric: Metric): number | null {
  if (!bucket) return null;
  if (metric === "sessions") return bucket.sessions;
  if (metric === "messages") return bucket.messages;
  return bucket.sessions > 0 && bucket.sessionsWithoutUsage === bucket.sessions
    ? null
    : bucket.totals.totalTokens;
}

/** Calendar alignment only: backend daily buckets remain the source of every metric. */
function calendarWeeks(buckets: DailyTokenBucket[]) {
  const sorted = [...buckets].sort((left, right) => left.dayStartMs - right.dayStartMs);
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  if (!first || !last) return [];
  const byDate = new Map(sorted.map((bucket) => [dateLabel(bucket.dayStartMs), bucket]));
  const cursor = new Date(first.dayStartMs);
  cursor.setHours(0, 0, 0, 0);
  cursor.setDate(cursor.getDate() - ((cursor.getDay() + 6) % 7));
  const end = new Date(last.dayStartMs);
  end.setHours(0, 0, 0, 0);
  end.setDate(end.getDate() + (6 - ((end.getDay() + 6) % 7)));
  const firstDate = dateLabel(first.dayStartMs);
  const lastDate = dateLabel(last.dayStartMs);
  const weeks: { ms: number; inRange: boolean; bucket: DailyTokenBucket | undefined }[][] = [];
  while (cursor <= end) {
    const week = [];
    for (let day = 0; day < 7; day += 1) {
      const key = dateLabel(cursor.getTime());
      week.push({
        ms: cursor.getTime(),
        inRange: key >= firstDate && key <= lastDate,
        bucket: byDate.get(key),
      });
      cursor.setDate(cursor.getDate() + 1);
    }
    weeks.push(week);
  }
  return weeks;
}

export function CalendarHeatmap({
  buckets,
  loading,
  error,
  onRetry,
  rangeLabel,
  controls,
  selectedRange,
  onSelectDay,
}: {
  buckets: DailyTokenBucket[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  rangeLabel: string;
  controls?: ReactNode;
  selectedRange?: DailyRange;
  onSelectDay?: (dayStartMs: number) => void;
}) {
  const [metric, setMetric] = useState<Metric>("tokens");
  const syncing = useSyncing();
  const titleId = useId();
  const scrollRef = useRef<HTMLDivElement>(null);
  const weeks = calendarWeeks(buckets ?? []);
  const dates = weeks.flat().filter((day) => day.inRange);
  const first = dates[0];
  const last = dates[dates.length - 1];
  const maxValue = Math.max(
    0,
    ...(buckets ?? []).map((bucket) => metricValue(bucket, metric) ?? 0),
  );
  const hasData = buckets?.some((bucket) => bucket.sessions > 0) ?? false;
  const { label, unit } = METRICS[metric];

  useEffect(() => {
    const viewport = scrollRef.current;
    if (viewport) viewport.scrollLeft = viewport.scrollWidth;
  }, [buckets, loading, error, metric]);

  return (
    <DashboardCard className="min-w-0 overflow-hidden">
      <Tabs
        value={metric}
        onValueChange={(value) => {
          if (value === "tokens" || value === "sessions" || value === "messages") setMetric(value);
        }}
      >
        <CardHeader className="gap-3 px-5">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="space-y-1">
              <CardTitle id={titleId} className="text-sm">
                活动日历
              </CardTitle>
              <CardDescription className="text-xs">
                {rangeLabel} · 本地时区 · {label}（{unit}）
              </CardDescription>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {controls}
              <TabsList aria-label="活动日历指标" className="h-8">
                {Object.entries(METRICS).map(([key, item]) => (
                  <TabsTrigger key={key} value={key} className="text-xs">
                    {item.label}
                  </TabsTrigger>
                ))}
              </TabsList>
            </div>
          </div>
        </CardHeader>
        <CardContent className="min-w-0 px-5 pt-4">
          <TabsContent value={metric} className="space-y-4">
            {error ? (
              <ErrorBlock message={error} onRetry={onRetry} />
            ) : !hasData && (loading || syncing) ? (
              <HeatmapSkeleton />
            ) : !hasData ? (
              <EmptyBlock
                title="此时间范围暂无会话"
                description="使用已接入的 AI 客户端后同步数据，或调整时间范围。"
              />
            ) : (
              <>
                <div
                  ref={scrollRef}
                  className="max-w-full overflow-x-auto pb-2"
                  role="region"
                  aria-labelledby={titleId}
                  tabIndex={0}
                >
                  <div className="flex w-max min-w-full gap-2">
                    <div
                      className="grid shrink-0 grid-rows-[1.5rem_repeat(7,1rem)] gap-1 text-xs text-muted-foreground"
                      aria-hidden="true"
                    >
                      <span />
                      {WEEKDAYS.map((day) => (
                        <span key={day} className="flex items-center">
                          {day}
                        </span>
                      ))}
                    </div>
                    <div className="flex gap-1">
                      {weeks.map((week, index) => {
                        const monthDay =
                          week.find((day) => day.inRange && new Date(day.ms).getDate() === 1) ??
                          (index === 0 &&
                          !weeks[1]?.some(
                            (next) => next.inRange && new Date(next.ms).getDate() === 1,
                          )
                            ? week.find((day) => day.inRange)
                            : undefined);
                        return (
                          <div
                            key={week[0]?.ms}
                            className="grid grid-rows-[1.5rem_repeat(7,1rem)] gap-1"
                          >
                            <span
                              className="relative text-xs text-muted-foreground"
                              aria-hidden="true"
                            >
                              {monthDay ? (
                                <span className="absolute whitespace-nowrap">
                                  {new Date(monthDay.ms).getMonth() + 1}月
                                </span>
                              ) : null}
                            </span>
                            {week.map((day) => {
                              if (!day.inRange)
                                return <span key={day.ms} className="size-4" aria-hidden="true" />;
                              const value = metricValue(day.bucket, metric);
                              const unknownSessions = day.bucket?.sessionsWithoutUsage ?? 0;
                              const partialSessions = day.bucket?.sessionsPartial ?? 0;
                              const partial =
                                metric === "tokens"
                                  ? unknownSessions > 0 || partialSessions > 0
                                  : metric === "messages" && partialSessions > 0;
                              const exact = `${value === null ? "—（未知）" : formatNumber(value)} ${unit}`;
                              const basis =
                                metric === "tokens"
                                  ? "实测 / 直接换算的已知会话累计 Token，含计入总量的缓存；按会话开始日期归属，不代表当天请求增量，不含估算。"
                                  : metric === "sessions"
                                    ? "客户端记录的会话数，按会话开始日期归属。"
                                    : "已采集请求明细条数，按所属会话开始日期归属；不代表全部请求，分页或续接可能导致明细不完整。";
                              const unknownNote =
                                unknownSessions > 0
                                  ? metric === "tokens"
                                    ? `${value !== null ? "仅含已知总量；" : ""}${formatNumber(unknownSessions)} 个会话用量未知，未计入已知总量。`
                                    : `${formatNumber(unknownSessions)} 个会话 Token 用量未知，会话与已采集请求明细数量仍可统计。`
                                  : "";
                              const coverage = `${unknownNote}${partialSessions > 0 ? `${formatNumber(partialSessions)} 个会话请求明细不完整；已知总量不一定缺失。` : ""}`;
                              const scope = dailyScope(day.ms, selectedRange);
                              return (
                                <HeatmapCell
                                  key={day.ms}
                                  value={value}
                                  maxValue={maxValue}
                                  partial={partial}
                                  className="size-4"
                                  onClick={onSelectDay ? () => onSelectDay(day.ms) : undefined}
                                  label={`${dateLabel(day.ms)}，${label}：${exact}。${basis}${coverage}`}
                                >
                                  <UsageTooltip
                                    title={`${dateLabel(day.ms)} · ${scope}`}
                                    totalLabel={dayTotalLabel(scope)}
                                    total={metricValue(day.bucket, "tokens")}
                                    requests={day.bucket?.messages}
                                    sessions={day.bucket?.sessions}
                                    unknown={unknownSessions}
                                    partial={partialSessions}
                                    basis="sessions"
                                  />
                                </HeatmapCell>
                              );
                            })}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </div>
                <div className="flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground">
                  <span>
                    {first && last ? `${dateLabel(first.ms)} — ${dateLabel(last.ms)}` : rangeLabel}
                  </span>
                  <HeatmapLegend maxValue={maxValue} unit={unit} />
                </div>
                <DataExplanation
                  summary={`按会话开始日 · 会话累计 · 不含估算${buckets?.some((day) => day.sessionsWithoutUsage > 0) ? " · 存在未知用量" : ""}${buckets?.some((day) => day.sessionsPartial > 0) ? " · 存在明细不完整" : ""}`}
                >
                  <p>
                    {metric === "tokens"
                      ? "按会话开始日期归属；Token 为会话累计用量，不代表当天请求增量。无用量记录显示 —，部分未知或明细不完整以标记说明。"
                      : metric === "sessions"
                        ? "按会话开始日期归属；会话数量不受 Token 是否已知影响。"
                        : "请求明细按所属会话开始日期归属，为已采集记录，可能不完整；不代表客户端的全部请求。"}
                    悬停或键盘聚焦日期查看精确值与口径。
                    {onSelectDay ? "点击日期查看当天开始的会话。" : ""}
                  </p>
                </DataExplanation>
              </>
            )}
          </TabsContent>
        </CardContent>
      </Tabs>
    </DashboardCard>
  );
}
