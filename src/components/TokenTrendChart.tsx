import { useId, useState, type ComponentProps } from "react";
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";

import { DashboardCard } from "@/components/DashboardCard";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { formatCompactTokens, formatDateTime, formatNumber } from "@/lib/format";
import type { DailyTokenBucket } from "@/lib/api";

const SERIES = [
  { key: "cacheRead", label: "缓存读取", color: "var(--chart-1)", dotClass: "bg-chart-1" },
  { key: "input", label: "输入", color: "var(--chart-2)", dotClass: "bg-chart-2" },
  { key: "output", label: "输出", color: "var(--chart-3)", dotClass: "bg-chart-3" },
  { key: "reasoning", label: "推理", color: "var(--chart-4)", dotClass: "bg-chart-4" },
] as const;
const TOTAL_SERIES = [
  { key: "codex", label: "Codex", color: "var(--chart-1)", dotClass: "bg-chart-1" },
] as const;
const CHART_CONFIG = {
  codex: { label: "Codex", color: "var(--chart-1)" },
  cacheRead: { label: "缓存读取", color: "var(--chart-1)" },
  input: { label: "输入（未命中缓存）", color: "var(--chart-2)" },
  output: { label: "输出（不含推理）", color: "var(--chart-3)" },
  reasoning: { label: "推理", color: "var(--chart-4)" },
} satisfies ChartConfig;

function TokenTooltip(props: ComponentProps<typeof ChartTooltipContent>) {
  if (!props.active || !props.payload?.length) return null;
  return (
    <div className="rounded-lg border border-border/70 bg-popover p-3 text-popover-foreground shadow-lg">
      <ChartTooltipContent
        {...props}
        className="min-w-56 border-0 bg-transparent p-0 shadow-none"
        labelFormatter={(label) => `${String(label)} · 本地日期`}
        formatter={(value, name) => (
          <div className="flex w-full items-center justify-between gap-6 py-0.5">
            <span className="text-muted-foreground">
              {CHART_CONFIG[String(name) as keyof typeof CHART_CONFIG]?.label ?? String(name)}
            </span>
            <span className="font-mono font-medium tabular-nums">
              {formatNumber(Number(value))}{" "}
              <span className="font-sans font-normal text-muted-foreground">Token</span>
            </span>
          </div>
        )}
      />
      <div className="mt-2 space-y-1 border-t border-border/60 pt-2 text-xs text-muted-foreground">
        <p>客户端实测 · 含缓存读取，不含缓存写入</p>
        <p>费用暂不可用 · 无可靠价格来源</p>
      </div>
    </div>
  );
}

export function TokenTrendChart({
  buckets,
  loading,
  error,
  onRetry,
  description,
  rangeLabel,
}: {
  buckets: DailyTokenBucket[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  description?: string;
  rangeLabel: string;
}) {
  const [mode, setMode] = useState("client");
  const gradientId = useId().replace(/:/g, "");
  const data = (buckets ?? []).map((bucket) => ({
    label: formatDateTime(bucket.dayStartMs).split(" ")[0],
    codex: bucket.totals.totalTokens,
    input: bucket.totals.inputTokens,
    output: bucket.totals.outputTokens,
    reasoning: bucket.totals.reasoningTokens,
    cacheRead: bucket.totals.cacheReadTokens,
  }));
  const hasData = data.some((entry) => entry.codex > 0);
  const series = mode === "client" ? TOTAL_SERIES : SERIES;
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
              Codex 总量
            </TabsTrigger>
            <TabsTrigger value="tokens" className="text-xs">
              Token 构成
            </TabsTrigger>
          </TabsList>
        </Tabs>
      </CardHeader>
      <CardContent className="space-y-4 px-5">
        <div className="min-h-56">
          {error ? (
            <ErrorBlock message={error} onRetry={onRetry} />
          ) : loading ? (
            <LoadingRows rows={5} />
          ) : !hasData ? (
            <EmptyBlock
              title="所选区间没有 Token 记录"
              description="换个时间范围，或点击刷新同步 Codex 数据。"
            />
          ) : (
            <ChartContainer
              config={CHART_CONFIG}
              className="aspect-auto h-56 w-full xl:h-64"
              aria-label={`${rangeLabel} Codex 每日 Token ${mode === "client" ? "总量" : "构成"}趋势`}
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
                  tickFormatter={(value: number) => formatCompactTokens(value)}
                  tickLine={false}
                  axisLine={false}
                  tickMargin={8}
                  width={60}
                />
                <ChartTooltip
                  content={<TokenTooltip />}
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
                    dot={data.length === 1}
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
        <p className="text-xs leading-relaxed text-muted-foreground">
          {description ??
            "按会话开始日期归属累计用量，不代表当天请求增量。Token 构成分桶互不重叠，缓存写入单独记录、不计入总量。"}
        </p>
      </CardContent>
    </DashboardCard>
  );
}
