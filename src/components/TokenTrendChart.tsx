import { useTokenFormat } from "@/hooks/use-token-format";
import { useId, useState } from "react";
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";

import { DataExplanation } from "@/components/DataExplanation";
import { UsageTooltip } from "@/components/UsageTooltip";
import { dailyScope, type DailyRange } from "@/lib/daily-tooltip";
import { DashboardCard } from "@/components/DashboardCard";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { ChartContainer, ChartTooltip, type ChartConfig } from "@/components/ui/chart";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { formatDateTime } from "@/lib/format";
import type { DailyTokenBucket } from "@/lib/usage-types";

const SERIES = [
  { key: "cacheRead", label: "缓存读取", color: "var(--chart-1)", dotClass: "bg-chart-1" },
  { key: "input", label: "输入", color: "var(--chart-2)", dotClass: "bg-chart-2" },
  { key: "output", label: "输出", color: "var(--chart-3)", dotClass: "bg-chart-3" },
  { key: "reasoning", label: "推理", color: "var(--chart-4)", dotClass: "bg-chart-4" },
  {
    key: "cacheWrite",
    label: "缓存写入（计入总量）",
    color: "var(--chart-5)",
    dotClass: "bg-chart-5",
  },
] as const;
const TOTAL_SERIES = [
  { key: "codex", label: "Codex", color: "var(--client-codex)", dotClass: "bg-client-codex" },
  {
    key: "claude_code",
    label: "Claude Code",
    color: "var(--client-claude)",
    dotClass: "bg-client-claude",
  },
  {
    key: "opencode",
    label: "OpenCode",
    color: "var(--client-opencode)",
    dotClass: "bg-client-opencode",
  },
  { key: "dsh", label: "DSH", color: "var(--client-dsh)", dotClass: "bg-client-dsh" },
  { key: "kimi_code", label: "Kimi Code", color: "var(--client-kimi)", dotClass: "bg-client-kimi" },
  { key: "mimocode", label: "MiMoCode", color: "var(--client-mimo)", dotClass: "bg-client-mimo" },
  { key: "pi_desktop", label: "Pi Desktop", color: "var(--client-pi)", dotClass: "bg-client-pi" },
] as const;
const CHART_CONFIG = {
  codex: { label: "Codex", color: "var(--client-codex)" },
  claude_code: { label: "Claude Code", color: "var(--client-claude)" },
  opencode: { label: "OpenCode", color: "var(--client-opencode)" },
  dsh: { label: "DSH", color: "var(--client-dsh)" },
  kimi_code: { label: "Kimi Code", color: "var(--client-kimi)" },
  mimocode: { label: "MiMoCode", color: "var(--client-mimo)" },
  pi_desktop: { label: "Pi Desktop", color: "var(--client-pi)" },
  cacheWrite: { label: "缓存写入（计入总量）", color: "var(--chart-5)" },
  cacheRead: { label: "缓存读取", color: "var(--chart-1)" },
  input: { label: "输入（未命中缓存）", color: "var(--chart-2)" },
  output: { label: "输出（不含推理）", color: "var(--chart-3)" },
  reasoning: { label: "推理", color: "var(--chart-4)" },
} satisfies ChartConfig;

export function TokenTrendChart({
  buckets,
  loading,
  error,
  onRetry,
  description,
  rangeLabel,
  selectedRange,
}: {
  buckets: DailyTokenBucket[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  description?: string;
  rangeLabel: string;
  selectedRange?: DailyRange;
}) {
  const { formatTokens } = useTokenFormat();
  const [mode, setMode] = useState("client");
  const gradientId = useId().replace(/:/g, "");
  const clientValue = (bucket: DailyTokenBucket, key: string) => {
    const entry = bucket.clients.find((client) => client.client === key);
    return entry && entry.sessions > 0 && entry.sessions === entry.sessionsWithoutUsage
      ? null
      : (entry?.totalTokens ?? 0);
  };
  const data = (buckets ?? []).map((bucket) => ({
    label: formatDateTime(bucket.dayStartMs).split(" ")[0],
    total:
      bucket.sessions > 0 && bucket.sessions === bucket.sessionsWithoutUsage
        ? null
        : bucket.totals.totalTokens,
    bucket,
    codex: clientValue(bucket, "codex"),
    claude_code: clientValue(bucket, "claude_code"),
    opencode: clientValue(bucket, "opencode"),
    dsh: clientValue(bucket, "dsh"),
    kimi_code: clientValue(bucket, "kimi_code"),
    mimocode: clientValue(bucket, "mimocode"),
    pi_desktop: clientValue(bucket, "pi_desktop"),
    cacheWrite: bucket.totals.countedCacheWriteTokens,
    input: bucket.totals.inputTokens,
    output: bucket.totals.outputTokens,
    reasoning: bucket.totals.reasoningTokens,
    cacheRead: bucket.totals.cacheReadTokens,
  }));
  const hasData = buckets?.some((entry) => entry.sessions > 0) ?? false;
  const visibleClients = TOTAL_SERIES.filter((item) =>
    buckets?.some((bucket) => bucket.clients.some((client) => client.client === item.key)),
  );
  const series = mode === "client" ? visibleClients : SERIES;
  const firstDate = data[0]?.label;
  const lastDate = data[data.length - 1]?.label;
  return (
    <DashboardCard>
      <CardHeader className="flex flex-wrap items-start justify-between gap-3 px-5">
        <div className="space-y-1.5">
          <CardTitle className="text-sm">每日 Token 趋势</CardTitle>
          <CardDescription className="text-xs">
            {rangeLabel} · 本地时区 · 单位 Token
          </CardDescription>
        </div>
        <Tabs value={mode} onValueChange={setMode} aria-label="趋势展示方式">
          <TabsList className="h-8 bg-muted/70">
            <TabsTrigger value="client" className="text-xs">
              客户端对比
            </TabsTrigger>
            <TabsTrigger value="tokens" className="text-xs">
              Token 构成
            </TabsTrigger>
          </TabsList>
        </Tabs>
      </CardHeader>
      <CardContent className="space-y-4 px-5">
        <div className={data.length === 1 ? "min-h-40" : "min-h-56"}>
          {error ? (
            <ErrorBlock message={error} onRetry={onRetry} />
          ) : loading ? (
            <LoadingRows rows={5} />
          ) : !hasData ? (
            <EmptyBlock
              title="所选区间没有 Token 记录"
              description="换个时间范围，或点击刷新同步客户端数据。"
            />
          ) : (
            <ChartContainer
              config={CHART_CONFIG}
              className={
                data.length === 1 ? "aspect-auto h-40 w-full" : "aspect-auto h-56 w-full xl:h-64"
              }
              aria-label={`${rangeLabel} 每日 Token ${mode === "client" ? "总量" : "构成"}趋势`}
            >
              <AreaChart data={data} margin={{ top: 12, right: 8, bottom: 0, left: 0 }}>
                <defs>
                  {series.map((item) => (
                    <linearGradient
                      key={item.key}
                      id={`${gradientId}-${item.key}`}
                      x1="0"
                      y1="0"
                      x2="0"
                      y2="1"
                    >
                      <stop offset="0%" stopColor={item.color} stopOpacity={0.2} />
                      <stop offset="100%" stopColor={item.color} stopOpacity={0.01} />
                    </linearGradient>
                  ))}
                </defs>
                <CartesianGrid
                  stroke="var(--border)"
                  strokeOpacity={0.65}
                  strokeDasharray="4 4"
                  vertical={false}
                />
                <XAxis
                  dataKey="label"
                  tickFormatter={(label: string) => label.slice(5)}
                  tickLine={false}
                  axisLine={false}
                  tickMargin={10}
                  minTickGap={24}
                />
                <YAxis
                  tickFormatter={(value: number) => formatTokens(value)}
                  tickLine={false}
                  axisLine={false}
                  tickMargin={8}
                  width="auto"
                />
                <ChartTooltip
                  content={({ active, label }) => {
                    if (!active) return null;
                    const entry = data.find((day) => day.label === String(label));
                    if (!entry) return null;
                    return (
                      <div className="rounded-md border bg-popover p-3 text-popover-foreground shadow-md">
                        <UsageTooltip
                          title={`${entry.label} · ${dailyScope(entry.bucket.dayStartMs, selectedRange)}`}
                          total={entry.total}
                          requests={entry.bucket.messages}
                          sessions={entry.bucket.sessions}
                          unknown={entry.bucket.sessionsWithoutUsage}
                          partial={entry.bucket.sessionsPartial}
                          basis="sessions"
                          breakdown={series.map((item) => {
                            const client =
                              mode === "client"
                                ? entry.bucket.clients.find((client) => client.client === item.key)
                                : undefined;
                            return {
                              key: item.key,
                              label: item.label,
                              value: entry[item.key],
                              note: client?.sessionsWithoutUsage
                                ? `${client.sessionsWithoutUsage} 个会话用量未知`
                                : undefined,
                            };
                          })}
                        />
                      </div>
                    );
                  }}
                  cursor={{ stroke: "var(--border)", strokeDasharray: "4 4" }}
                />
                {series.map((item) => (
                  <Area
                    key={item.key}
                    type="monotone"
                    dataKey={item.key}
                    stackId={mode === "tokens" ? "tokens" : undefined}
                    stroke={item.color}
                    fill={`url(#${gradientId}-${item.key})`}
                    strokeWidth={2}
                    dot={data.length === 1 ? { r: 6 } : false}
                    activeDot={{ r: 4, strokeWidth: 2, stroke: "var(--card)" }}
                    isAnimationActive={false}
                  />
                ))}
              </AreaChart>
            </ChartContainer>
          )}
        </div>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-4 text-xs text-muted-foreground">
            {series.map((item) => (
              <span key={item.key} className="flex items-center gap-1.5">
                <span className={`size-2 rounded-full ${item.dotClass}`} aria-hidden="true" />
                {item.label}
              </span>
            ))}
          </div>
          {firstDate && lastDate ? (
            <span className="font-mono text-xs text-muted-foreground">
              {firstDate} — {lastDate}
            </span>
          ) : null}
        </div>
        <DataExplanation
          summary={`按会话开始日 · 会话累计 · 不含估算${buckets?.some((day) => day.sessionsWithoutUsage > 0) ? " · 存在未知用量" : ""}${buckets?.some((day) => day.sessionsPartial > 0) ? " · 存在明细不完整" : ""}`}
        >
          <p>
            {description ??
              "按会话开始日期归属累计用量，不代表当天请求增量。Token 分桶互不重叠；缓存写入依据各客户端口径计入总量，缺失字段不推算。"}
          </p>
        </DataExplanation>
      </CardContent>
    </DashboardCard>
  );
}
