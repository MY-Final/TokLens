import { useTokenFormat } from "@/hooks/use-token-format";
import { useState } from "react";
import { CartesianGrid, Line, LineChart, Tooltip, XAxis, YAxis } from "recharts";

import { DataExplanation } from "@/components/DataExplanation";
import { ChartTooltipOverlay } from "@/components/ChartTooltipOverlay";
import { UsageTooltip } from "@/components/UsageTooltip";
import { dailyScope, type DailyRange } from "@/lib/daily-tooltip";
import { ClientIdentity } from "@/components/ClientIdentity";
import { DashboardCard } from "@/components/DashboardCard";
import { HeatmapCell, HeatmapLegend } from "@/components/HeatmapCell";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { Button } from "@/components/ui/button";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { ChartContainer } from "@/components/ui/chart";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { clientLabel } from "@/lib/client-labels";
import {
  formatDateTime,
  formatNumber,
  formatTokensOrDash as formatExactOrDash,
} from "@/lib/format";
import type {
  ClientKey,
  RequestActivity,
  RequestDailyBucket,
  RequestTotals,
} from "@/lib/usage-types";

type Metric = "tokens" | "requests";
const CLIENTS: ClientKey[] = [
  "codex",
  "claude_code",
  "opencode",
  "dsh",
  "kimi_code",
  "mimocode",
  "pi_desktop",
  "workbuddy",
];
const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];
const CHART_CONFIG = { value: { label: "已记录请求", color: "var(--chart-1)" } };
const NO_RECORDED_REQUESTS: RequestTotals = {
  sessions: 0,
  requests: 0,
  requestsWithUsage: 0,
  requestsWithoutUsage: 0,
  measuredRequests: 0,
  derivedRequests: 0,
  totals: {
    inputTokens: 0,
    outputTokens: 0,
    reasoningTokens: 0,
    cacheReadTokens: 0,
    cacheWriteTokens: 0,
    countedCacheWriteTokens: 0,
    totalTokens: 0,
  },
};

function dateLabel(ms: number) {
  return formatDateTime(ms).split(" ")[0] ?? "—";
}
function metricValue(totals: RequestTotals | undefined, metric: Metric): number | null {
  if (!totals) return null;
  if (metric === "requests") return totals.requests;
  return totals.requests > 0 && totals.requestsWithUsage === 0 ? null : totals.totals.totalTokens;
}
function description(totals: RequestTotals | undefined, metric: Metric): string {
  if (!totals) return "未返回该日统计，数值未知。";
  if (totals.requests === 0)
    return "未记录请求：0 条；已记录请求的已知 Token 求和为 0。不代表客户端实测总量为零或没有活动。按请求发生日期归属，本地时区，不含估算。";
  const value = metricValue(totals, metric);
  return `${metric === "tokens" ? "已知 Token" : "请求数"}：${formatExactOrDash(value)}。${totals.requestsWithoutUsage > 0 ? `${value !== null && metric === "tokens" ? "Token 仅含已知子集；" : ""}${formatNumber(totals.requestsWithoutUsage)} 条请求用量未知。` : ""}实测 ${formatNumber(totals.measuredRequests)} 条，换算 ${formatNumber(totals.derivedRequests)} 条；不含估算。按请求发生时间归属，本地时区。`;
}

/** Only align backend buckets to local calendar weeks; never aggregate metrics here. */
function calendarWeeks(days: RequestDailyBucket[]) {
  const first = days[0];
  const last = days[days.length - 1];
  if (!first || !last) return [];
  const byDate = new Map(days.map((day) => [dateLabel(day.dayStartMs), day]));
  const cursor = new Date(first.dayStartMs);
  cursor.setHours(0, 0, 0, 0);
  cursor.setDate(cursor.getDate() - ((cursor.getDay() + 6) % 7));
  const end = new Date(last.dayStartMs);
  end.setHours(0, 0, 0, 0);
  end.setDate(end.getDate() + 6 - ((end.getDay() + 6) % 7));
  const weeks: { ms: number; inRange: boolean; bucket: RequestDailyBucket | undefined }[][] = [];
  while (cursor <= end) {
    const week = [];
    for (let i = 0; i < 7; i += 1) {
      const ms = cursor.getTime();
      const key = dateLabel(ms);
      week.push({
        ms,
        inRange: key >= dateLabel(first.dayStartMs) && key <= dateLabel(last.dayStartMs),
        bucket: byDate.get(key),
      });
      cursor.setDate(cursor.getDate() + 1);
    }
    weeks.push(week);
  }
  return weeks;
}

function ActivityCell({
  ms,
  totals,
  metric,
  maxValue,
  prefix = "",
  className,
  selectedRange,
}: {
  ms: number;
  totals: RequestTotals | undefined;
  metric: Metric;
  maxValue: number;
  prefix?: string;
  className?: string;
  selectedRange?: DailyRange;
}) {
  const note = description(totals, metric);
  return (
    <HeatmapCell
      value={metricValue(totals, metric)}
      maxValue={maxValue}
      partial={metric === "tokens" && (totals?.requestsWithoutUsage ?? 0) > 0}
      label={`${prefix}${dateLabel(ms)}，${note}`}
      className={className}
    >
      <UsageTooltip
        title={`${prefix}${dateLabel(ms)} · ${dailyScope(ms, selectedRange)}`}
        total={metricValue(totals, "tokens")}
        requests={totals?.requests}
        sessions={totals?.sessions}
        unknown={totals?.requestsWithoutUsage}
        basis="requests"
      />
    </HeatmapCell>
  );
}

export function RequestActivityPanel({
  data,
  loading,
  error,
  onRetry,
  rangeLabel,
  client,
  selectedRange,
}: {
  data: RequestActivity | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  rangeLabel: string;
  client?: ClientKey | null;
  selectedRange?: DailyRange;
}) {
  const { formatTokens, formatTokensOrDash } = useTokenFormat();
  const [metric, setMetric] = useState<Metric>("tokens");
  const [chartElement, setChartElement] = useState<HTMLDivElement | null>(null);
  const days = data?.daily ?? [];
  const clients = (client ? [client] : CLIENTS).filter((key) =>
    days.some((day) =>
      day.clients.some((entry) => entry.client === key && entry.totals.requests > 0),
    ),
  );
  const maxValue = Math.max(0, ...days.map((day) => metricValue(day.totals, metric) ?? 0));
  const clientMax = Math.max(
    0,
    ...days.flatMap((day) => day.clients.map((entry) => metricValue(entry.totals, metric) ?? 0)),
  );
  const range = data?.range;
  const coverage = data?.coverage;
  const unit = metric === "tokens" ? "Token" : "条请求";
  const hasData = (range?.requests ?? 0) > 0;
  return (
    <DashboardCard className="min-w-0">
      <CardHeader className="gap-3 px-5">
        <CardTitle className="text-sm">请求用量</CardTitle>
        <CardDescription className="text-xs">{rangeLabel} · 本地时间</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 px-5">
        {error ? (
          <ErrorBlock message={error} onRetry={onRetry} />
        ) : loading ? (
          <LoadingRows rows={5} />
        ) : data ? (
          <>
            <dl className="grid grid-cols-3 gap-4 text-xs">
              {[
                ["已知 Token", formatTokensOrDash(metricValue(range, "tokens"))],
                ["请求数", formatNumber(range?.requests ?? 0)],
                ["有请求会话数", formatNumber(range?.sessions ?? 0)],
              ].map(([label, value]) => (
                <div key={label} className="space-y-1">
                  <dt className="text-muted-foreground">{label}</dt>
                  <dd
                    className="font-mono text-lg tabular-nums"
                    title={
                      label === "已知 Token"
                        ? `${formatExactOrDash(metricValue(range, "tokens"))} Token`
                        : undefined
                    }
                  >
                    {value}
                  </dd>
                </div>
              ))}
            </dl>
            {(range?.requestsWithoutUsage ?? 0) > 0 ? (
              <p className="text-xs text-muted-foreground">
                {range?.requestsWithUsage ? "已知子集 · " : ""}
                {formatNumber(range?.requestsWithoutUsage ?? 0)} 条请求用量未知
              </p>
            ) : null}
            {coverage ? (
              <DataExplanation
                summary={`实测 / 换算 · 不含估算${coverage.sessionsPartial > 0 ? " · 明细不完整" : ""}`}
              >
                <p className="tabular-nums">
                  采集覆盖（所有已采集日期，仅按客户端筛选，与当前日期范围独立）：有时间{" "}
                  {formatNumber(coverage.timedRequests)} / 已记录{" "}
                  {formatNumber(coverage.recordedRequests)}{" "}
                  条。此处为已采集记录计数，不能代表实际请求或 Token 的完整覆盖。无时间{" "}
                  {formatNumber(coverage.untimedRequests)} 条、估算{" "}
                  {formatNumber(coverage.estimatedRequests)} 条不计入时间视图；
                  {formatNumber(coverage.sessionsPartial)} 个会话明细不完整，
                  {formatNumber(coverage.sessionsWithoutRequests)} 个会话没有请求明细。
                </p>
              </DataExplanation>
            ) : null}
            {!hasData ? (
              <EmptyBlock title="暂无请求记录" description="调整日期或点击刷新。" />
            ) : (
              <Tabs defaultValue="trend" className="space-y-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <TabsList aria-label="请求时间视图">
                    <TabsTrigger value="trend">趋势</TabsTrigger>
                    <TabsTrigger value="calendar">日历</TabsTrigger>
                    <TabsTrigger value="clients">客户端</TabsTrigger>
                  </TabsList>
                  <div className="flex gap-1" role="group" aria-label="请求时间统计指标">
                    <Button
                      size="sm"
                      variant={metric === "tokens" ? "secondary" : "ghost"}
                      aria-pressed={metric === "tokens"}
                      onClick={() => setMetric("tokens")}
                    >
                      Token
                    </Button>
                    <Button
                      size="sm"
                      variant={metric === "requests" ? "secondary" : "ghost"}
                      aria-pressed={metric === "requests"}
                      onClick={() => setMetric("requests")}
                    >
                      请求数
                    </Button>
                  </div>
                </div>
                <TabsContent value="trend" className="space-y-3">
                  <ChartContainer
                    ref={setChartElement}
                    config={CHART_CONFIG}
                    className={
                      days.length === 1 ? "aspect-auto h-40 w-full" : "aspect-auto h-56 w-full"
                    }
                    aria-label={`${rangeLabel} 按请求发生日期的${unit}趋势，精确值见下方日期列表`}
                  >
                    <LineChart
                      accessibilityLayer
                      data={days.map((day) => ({
                        label: dateLabel(day.dayStartMs),
                        value: metricValue(day.totals, metric),
                      }))}
                    >
                      <CartesianGrid
                        stroke="var(--border)"
                        vertical={false}
                        strokeDasharray="4 4"
                      />
                      <XAxis
                        dataKey="label"
                        tickFormatter={(label: string) => label.slice(5)}
                        axisLine={false}
                        tickLine={false}
                        minTickGap={24}
                      />
                      <YAxis
                        tickFormatter={(value: number) =>
                          metric === "tokens" ? formatTokens(value) : formatNumber(value)
                        }
                        axisLine={false}
                        tickLine={false}
                        width="auto"
                      />
                      <Tooltip
                        portal={chartElement?.ownerDocument.body}
                        wrapperStyle={{
                          position: "fixed",
                          top: 0,
                          left: 0,
                          zIndex: 40,
                          pointerEvents: "none",
                        }}
                        content={({ active, label, coordinate }) => {
                          if (!active) return null;
                          const bucket = days.find(
                            (day) => dateLabel(day.dayStartMs) === String(label),
                          );
                          return (
                            <ChartTooltipOverlay anchor={chartElement} coordinate={coordinate}>
                              <UsageTooltip
                                title={`${String(label)} · ${bucket ? dailyScope(bucket.dayStartMs, selectedRange) : "本地日期"}`}
                                total={metricValue(bucket?.totals, "tokens")}
                                requests={bucket?.totals.requests}
                                sessions={bucket?.totals.sessions}
                                unknown={bucket?.totals.requestsWithoutUsage}
                                basis="requests"
                                breakdownUnit={unit}
                                breakdown={(bucket?.clients ?? [])
                                  .filter((entry) => entry.totals.requests > 0)
                                  .map((entry) => ({
                                    key: entry.client,
                                    label: clientLabel(entry.client),
                                    value: metricValue(entry.totals, metric),
                                    note:
                                      metric === "tokens" && entry.totals.requestsWithoutUsage > 0
                                        ? `${formatNumber(entry.totals.requestsWithoutUsage)} 条请求用量未知`
                                        : undefined,
                                  }))}
                              />
                            </ChartTooltipOverlay>
                          );
                        }}
                      />
                      <Line
                        dataKey="value"
                        stroke="var(--chart-1)"
                        strokeWidth={2}
                        connectNulls={false}
                        dot={days.length === 1 ? { r: 6 } : false}
                        isAnimationActive={false}
                      />
                    </LineChart>
                  </ChartContainer>
                  <p className="text-xs text-muted-foreground">单位：{unit}</p>
                  <div className="flex gap-1 overflow-x-auto py-2">
                    {days.map((day) => (
                      <ActivityCell
                        selectedRange={selectedRange}
                        key={day.dayStartMs}
                        ms={day.dayStartMs}
                        totals={day.totals}
                        metric={metric}
                        maxValue={maxValue}
                      />
                    ))}
                  </div>
                  <HeatmapLegend maxValue={maxValue} unit={unit} />
                </TabsContent>
                <TabsContent value="calendar" className="space-y-3">
                  <div className="flex gap-2 overflow-x-auto py-2">
                    <div
                      className="grid grid-rows-[1.5rem_repeat(7,1rem)] gap-1 text-xs text-muted-foreground"
                      aria-hidden="true"
                    >
                      <span />
                      {WEEKDAYS.map((day) => (
                        <span key={day}>{day}</span>
                      ))}
                    </div>
                    {calendarWeeks(days).map((week) => (
                      <div
                        key={week[0]?.ms}
                        className="grid grid-rows-[1.5rem_repeat(7,1rem)] gap-1"
                      >
                        <span className="text-xs whitespace-nowrap text-muted-foreground">
                          {week.find((day) => day.inRange && new Date(day.ms).getDate() === 1)
                            ? `${new Date(week.find((day) => day.inRange && new Date(day.ms).getDate() === 1)?.ms ?? 0).getMonth() + 1}月`
                            : ""}
                        </span>
                        {week.map((day) =>
                          day.inRange ? (
                            <ActivityCell
                              selectedRange={selectedRange}
                              key={day.ms}
                              ms={day.ms}
                              totals={day.bucket?.totals}
                              metric={metric}
                              maxValue={maxValue}
                            />
                          ) : (
                            <span key={day.ms} className="size-4" aria-hidden="true" />
                          ),
                        )}
                      </div>
                    ))}
                  </div>
                  <HeatmapLegend maxValue={maxValue} unit={unit} />
                </TabsContent>
                <TabsContent value="clients" className="space-y-3">
                  <div className="overflow-x-auto pb-2">
                    <table
                      className="w-max min-w-full text-xs"
                      aria-label={`${rangeLabel} 客户端请求发生日期${unit}`}
                    >
                      <thead>
                        <tr>
                          <th scope="col" className="sticky left-0 bg-card pr-4 text-left">
                            客户端 / 日期
                          </th>
                          {days.map((day, index) => (
                            <th
                              key={day.dayStartMs}
                              scope="col"
                              className="min-w-8 text-left font-normal text-muted-foreground"
                              aria-label={dateLabel(day.dayStartMs)}
                            >
                              {index % (days.length > 31 ? 7 : 3) === 0
                                ? dateLabel(day.dayStartMs).slice(5)
                                : ""}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {clients.map((key) => (
                          <tr key={key}>
                            <th scope="row" className="sticky left-0 bg-card py-1 pr-4 text-left">
                              <ClientIdentity client={key} compact />
                            </th>
                            {days.map((day) => (
                              <td key={day.dayStartMs} className="py-1 pr-1">
                                <ActivityCell
                                  selectedRange={selectedRange}
                                  ms={day.dayStartMs}
                                  totals={
                                    day.clients.find((entry) => entry.client === key)?.totals ??
                                    NO_RECORDED_REQUESTS
                                  }
                                  metric={metric}
                                  maxValue={clientMax}
                                  prefix={`${clientLabel(key)} · `}
                                  className="size-7"
                                />
                              </td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  <HeatmapLegend maxValue={clientMax} unit={unit} />
                  <p className="text-xs text-muted-foreground">所有客户端共享同一强度标尺。</p>
                </TabsContent>
              </Tabs>
            )}
          </>
        ) : (
          <EmptyBlock title="暂无请求统计" description="同步本地客户端数据后重试。" />
        )}
      </CardContent>
    </DashboardCard>
  );
}
