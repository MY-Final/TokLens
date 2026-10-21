import { useTokenFormat } from "@/hooks/use-token-format";
import { useState } from "react";
import { Bar, BarChart, CartesianGrid, Line, LineChart, Tooltip, XAxis, YAxis } from "recharts";

import { DataExplanation } from "@/components/DataExplanation";
import { ChartTooltipOverlay } from "@/components/ChartTooltipOverlay";
import { UsageTooltip } from "@/components/UsageTooltip";
import { dailyScope, type DailyRange } from "@/lib/daily-tooltip";
import { ClientIdentity } from "@/components/ClientIdentity";
import { DashboardCard } from "@/components/DashboardCard";
import { HeatmapCell, HeatmapLegend } from "@/components/HeatmapCell";
import { EmptyBlock, ChartSkeleton } from "@/components/StateBlocks";
import { useSyncing } from "@/hooks/use-sync-status";
import { Button } from "@/components/ui/button";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { ChartContainer } from "@/components/ui/chart";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CLIENT_KEYS, clientLabel } from "@/lib/client-labels";
import {
  dayTotalLabel,
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
const CLIENTS = CLIENT_KEYS;
const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];
const CHART_CONFIG = { value: { label: "已记录请求", color: "var(--chart-1)" } };
/** 趋势图高度：宽窗口更高，窄窗口保持可读比例。 */
const CHART_HEIGHT = "aspect-auto h-64 w-full min-[1200px]:h-72 min-[1600px]:h-80";
/** 桶数不超过该值时用柱状图；更多桶改用折线，避免柱子挤成细线。 */
const BAR_BUCKET_LIMIT = 120;
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
/** 时区短名（GMT+8），用于区分夏令时重复出现的小时。 */
function timeZoneLabel(ms: number) {
  const parts = new Intl.DateTimeFormat("zh-CN", { timeZoneName: "short" }).formatToParts(
    new Date(ms),
  );
  return parts.find((part) => part.type === "timeZoneName")?.value ?? "";
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
  const scope = dailyScope(ms, selectedRange);
  return (
    <HeatmapCell
      value={metricValue(totals, metric)}
      maxValue={maxValue}
      partial={metric === "tokens" && (totals?.requestsWithoutUsage ?? 0) > 0}
      label={`${prefix}${dateLabel(ms)}，${note}`}
      className={className}
    >
      <UsageTooltip
        title={`${prefix}${dateLabel(ms)} · ${scope}`}
        totalLabel={dayTotalLabel(scope)}
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
  rangeLabel,
  client,
  selectedRange,
}: {
  data: RequestActivity | null;
  loading: boolean;
  rangeLabel: string;
  client?: ClientKey | null;
  selectedRange?: DailyRange;
}) {
  const { formatTokens, formatTokensOrDash } = useTokenFormat();
  const [metric, setMetric] = useState<Metric>("tokens");
  const [chartElement, setChartElement] = useState<HTMLDivElement | null>(null);
  const syncing = useSyncing();
  const days = data?.daily ?? [];
  const hours = data?.hourly ?? [];
  const hasHourly = hours.length > 0;
  const trendBuckets = hasHourly
    ? hours.map((hour) => ({ startMs: hour.hourStartMs, totals: hour.totals, clients: [] }))
    : days.map((day) => ({ startMs: day.dayStartMs, totals: day.totals, clients: day.clients }));
  const trendLabel = (ms: number) =>
    hasHourly ? (formatDateTime(ms).split(" ")[1] ?? "—") : dateLabel(ms);
  const trendScope = (ms: number) =>
    hasHourly
      ? `${formatDateTime(ms)} · ${timeZoneLabel(ms)} · 本地小时`
      : `${dateLabel(ms)} · ${dailyScope(ms, selectedRange)}`;
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
  const chartData = trendBuckets.map((bucket) => ({
    startMs: bucket.startMs,
    value: metricValue(bucket.totals, metric),
  }));
  // 用量是按小时 / 按天求和的离散量，柱子能如实表达每个区间的总量；
  // 桶数过多时柱子会挤成细线，改用折线表达整体走势。
  const useBars = trendBuckets.length > 0 && trendBuckets.length <= BAR_BUCKET_LIMIT;
  const chartGrid = <CartesianGrid stroke="var(--border)" vertical={false} strokeDasharray="4 4" />;
  const chartAxes = (
    <>
      <XAxis
        dataKey="startMs"
        tickFormatter={(ms: number) => (hasHourly ? trendLabel(ms) : trendLabel(ms).slice(5))}
        axisLine={false}
        tickLine={false}
        minTickGap={24}
        tick={{ fontSize: 11, fill: "var(--muted-foreground)" }}
      />
      <YAxis
        tickFormatter={(value: number) =>
          metric === "tokens" ? formatTokens(value) : formatNumber(value)
        }
        axisLine={false}
        tickLine={false}
        width="auto"
        tick={{ fontSize: 11, fill: "var(--muted-foreground)" }}
      />
    </>
  );
  const chartTooltip = (
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
        const bucket = trendBuckets.find((entry) => entry.startMs === Number(label));
        return (
          <ChartTooltipOverlay anchor={chartElement} coordinate={coordinate}>
            <UsageTooltip
              title={bucket ? trendScope(bucket.startMs) : "本地时间"}
              totalLabel={
                hasHourly
                  ? "已知 Token"
                  : dayTotalLabel(dailyScope(bucket?.startMs ?? 0, selectedRange))
              }
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
  );
  return (
    <DashboardCard className="min-w-0">
      <CardHeader className="gap-3 px-5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="space-y-1">
            <CardTitle className="text-sm font-medium">用量趋势</CardTitle>
            <CardDescription className="text-xs">{rangeLabel} · 本地时间</CardDescription>
          </div>
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
      </CardHeader>
      <CardContent className="space-y-4 px-5">
        {(!data || !hasData) && (loading || syncing) ? (
          <ChartSkeleton blockClassName={CHART_HEIGHT} />
        ) : data ? (
          <>
            {!hasData ? (
              <EmptyBlock title="暂无请求记录" description="调整日期或点击刷新。" />
            ) : (
              <Tabs defaultValue="trend" className="space-y-4">
                <TabsList aria-label="请求时间视图">
                  <TabsTrigger value="trend">{hasHourly ? "小时趋势" : "趋势"}</TabsTrigger>
                  <TabsTrigger value="calendar">日历</TabsTrigger>
                  <TabsTrigger value="clients">客户端</TabsTrigger>
                </TabsList>
                <TabsContent value="trend" className="space-y-3">
                  <ChartContainer
                    ref={setChartElement}
                    config={CHART_CONFIG}
                    className={trendBuckets.length === 1 ? "aspect-auto h-40 w-full" : CHART_HEIGHT}
                    aria-label={`${rangeLabel} 按请求发生${hasHourly ? "小时" : "日期"}的${unit}趋势，精确值见下方列表`}
                  >
                    {useBars ? (
                      <BarChart accessibilityLayer data={chartData}>
                        {chartGrid}
                        {chartAxes}
                        {chartTooltip}
                        <Bar
                          dataKey="value"
                          fill="var(--chart-1)"
                          radius={[2, 2, 0, 0]}
                          maxBarSize={28}
                          isAnimationActive={false}
                        />
                      </BarChart>
                    ) : (
                      <LineChart accessibilityLayer data={chartData}>
                        {chartGrid}
                        {chartAxes}
                        {chartTooltip}
                        <Line
                          dataKey="value"
                          stroke="var(--chart-1)"
                          strokeWidth={2}
                          connectNulls={false}
                          dot={trendBuckets.length === 1 ? { r: 6 } : false}
                          isAnimationActive={false}
                        />
                      </LineChart>
                    )}
                  </ChartContainer>
                  {hasHourly ? (
                    <p className="text-xs text-muted-foreground">
                      单位：{unit} · 统计截至{" "}
                      {selectedRange ? formatDateTime(selectedRange.toMs - 1) : "查询截止时间"}
                    </p>
                  ) : (
                    <p className="text-xs text-muted-foreground">单位：{unit}</p>
                  )}
                  {hasHourly ? (
                    <div className="@container/hourly">
                      <div
                        role="list"
                        aria-label="每小时统计，精确值可聚焦查看"
                        className="grid grid-cols-4 gap-x-2 gap-y-3 py-2 @sm/hourly:grid-cols-6 @2xl/hourly:grid-cols-8 @4xl/hourly:grid-cols-12"
                      >
                        {hours.map((hour) => {
                          const status =
                            hour.totals.requests === 0
                              ? "未记录"
                              : hour.totals.requestsWithUsage === 0
                                ? "Token 未知"
                                : hour.totals.requestsWithoutUsage > 0
                                  ? "已知子集"
                                  : null;
                          return (
                            <div
                              key={hour.hourStartMs}
                              role="listitem"
                              tabIndex={0}
                              className="min-w-0 space-y-1 rounded-sm text-xs focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
                              aria-label={`${trendScope(hour.hourStartMs)} · ${description(hour.totals, metric)}`}
                              title={`${trendScope(hour.hourStartMs)} · ${description(hour.totals, metric)}`}
                            >
                              <p className="text-muted-foreground">
                                {trendLabel(hour.hourStartMs)}
                              </p>
                              <p className="font-mono font-medium tabular-nums">
                                {metric === "tokens"
                                  ? formatTokensOrDash(metricValue(hour.totals, metric))
                                  : formatExactOrDash(metricValue(hour.totals, metric))}
                              </p>
                              {status ? <p className="text-muted-foreground">{status}</p> : null}
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  ) : (
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
                  )}
                  {hasHourly ? null : <HeatmapLegend maxValue={maxValue} unit={unit} />}
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
                  <div className="pb-2">
                    <Table
                      className="w-max min-w-full text-xs"
                      aria-label={`${rangeLabel} 客户端请求发生日期${unit}`}
                    >
                      <TableHeader className="[&_tr]:border-0">
                        <TableRow className="border-0 hover:bg-transparent">
                          <TableHead
                            scope="col"
                            className="sticky left-0 h-auto bg-card p-0 pr-4 text-left"
                          >
                            客户端 / 日期
                          </TableHead>
                          {days.map((day, index) => (
                            <TableHead
                              key={day.dayStartMs}
                              scope="col"
                              className="h-auto min-w-8 p-0 text-left font-normal text-muted-foreground"
                              aria-label={dateLabel(day.dayStartMs)}
                            >
                              {index % (days.length > 31 ? 7 : 3) === 0
                                ? dateLabel(day.dayStartMs).slice(5)
                                : ""}
                            </TableHead>
                          ))}
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {clients.map((key) => (
                          <TableRow key={key} className="border-0 hover:bg-transparent">
                            <TableHead
                              scope="row"
                              className="sticky left-0 h-auto bg-card p-0 py-1 pr-4 text-left"
                            >
                              <ClientIdentity client={key} compact />
                            </TableHead>
                            {days.map((day) => (
                              <TableCell key={day.dayStartMs} className="p-0 py-1 pr-1">
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
                              </TableCell>
                            ))}
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                  <HeatmapLegend maxValue={clientMax} unit={unit} />
                  <p className="text-xs text-muted-foreground">所有客户端共享同一强度标尺。</p>
                </TabsContent>
              </Tabs>
            )}
            {coverage ? (
              <DataExplanation
                summary={`实测 / 换算 · 不含估算${coverage.sessionsPartial > 0 ? " · 明细不完整" : ""}`}
              >
                <p>未记录不代表没有活动；未知 Token 不补零，混合记录只展示已知子集。</p>
                <p>未结束的小时只包含截止前的记录；采集覆盖无法确认完整，不计算昨日同期增减。</p>
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
          </>
        ) : (
          <EmptyBlock title="暂无请求统计" description="同步本地客户端数据后重试。" />
        )}
      </CardContent>
    </DashboardCard>
  );
}
