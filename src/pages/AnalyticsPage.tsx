/** 用量分析：分桶构成 + 趋势 + 排行，均按所选区间与客户端筛选。 */

import { EmptyBlock } from "@/components/StateBlocks";
import { DashboardCard } from "@/components/DashboardCard";
import { ClientDateHeatmap } from "@/components/ClientDateHeatmap";
import { FilterSelectContent } from "@/components/FilterSelectContent";

import { useState } from "react";

import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { RankingCard } from "@/components/RankingCard";
import { StatCard } from "@/components/StatCard";
import { ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { TokenTrendChart } from "@/components/TokenTrendChart";
import { useAsyncData } from "@/hooks/use-async-data";
import { getDailyTokenTrend, getOverviewStats, getRankings } from "@/lib/api";
import type { ClientKey } from "@/lib/usage-types";
import { formatCompactTokens, formatNumber, formatPercent } from "@/lib/format";
import type { DateRange } from "@/lib/ranges";

const CLIENT_FILTERS: ReadonlyArray<{ value: string; label: string; client: ClientKey | null }> = [
  { value: "all", label: "全部客户端", client: null },
  { value: "codex", label: "Codex", client: "codex" },
  { value: "claude_code", label: "Claude Code", client: "claude_code" },
  { value: "opencode", label: "OpenCode", client: "opencode" },
  { value: "dsh", label: "DSH", client: "dsh" },
  { value: "kimi_code", label: "Kimi Code", client: "kimi_code" },
  { value: "mimocode", label: "MiMoCode", client: "mimocode" },
  { value: "pi_desktop", label: "Pi Desktop", client: "pi_desktop" },
];

export function AnalyticsPage({ range, refreshToken }: { range: DateRange; refreshToken: number }) {
  const [clientValue, setClientValue] = useState("all");
  const selected = CLIENT_FILTERS.find((entry) => entry.value === clientValue) ?? CLIENT_FILTERS[0];
  const client = selected?.client ?? null;
  const keySuffix = `${range.key}:${clientValue}:${range.fromMs ?? "all"}:${range.toMs}:${refreshToken}`;
  const trendRangeLabel =
    range.fromMs !== null && range.fromMs < range.dayStartMs
      ? `${range.label} · 仅展示最近 400 天`
      : range.label;

  const overview = useAsyncData(
    () =>
      getOverviewStats({
        fromMs: range.fromMs,
        toMs: range.toMs,
        todayFromMs: 0,
        client,
      }),
    `analytics-overview:${keySuffix}`,
  );
  const trend = useAsyncData(
    () =>
      getDailyTokenTrend({
        fromMs: range.fromMs ?? range.dayStartMs,
        toMs: range.toMs,
        dayStartMs: range.dayStartMs,
        dayCount: range.dayCount,
        client,
      }),
    `analytics-trend:${keySuffix}`,
  );
  const rankings = useAsyncData(
    () => getRankings({ fromMs: range.fromMs, toMs: range.toMs, client, limit: 12 }),
    `analytics-rankings:${keySuffix}`,
  );

  const totals = overview.data?.range.totals;
  const total = totals?.totalTokens ?? 0;
  const buckets: ReadonlyArray<{ label: string; value: number; note: string; counted: boolean }> = [
    {
      label: "输入（未命中缓存）",
      value: totals?.inputTokens ?? 0,
      note: "按各客户端语义分离缓存读取",
      counted: true,
    },
    {
      label: "输出（不含推理）",
      value: totals?.outputTokens ?? 0,
      note: "包含关系已扣除；缺失推理字段不推算",
      counted: true,
    },
    {
      label: "推理",
      value: totals?.reasoningTokens ?? 0,
      note: "按客户端记录与包含关系归一化",
      counted: true,
    },
    {
      label: "缓存读取",
      value: totals?.cacheReadTokens ?? 0,
      note: "命中缓存的输入",
      counted: true,
    },
    {
      label: "缓存写入",
      value:
        client === null ? (totals?.countedCacheWriteTokens ?? 0) : (totals?.cacheWriteTokens ?? 0),
      note:
        client === "codex" || client === "opencode"
          ? "客户端单独记录，不计入规范总量"
          : "仅统计计入规范总量的缓存写入",
      counted:
        client === null ||
        client === "claude_code" ||
        client === "dsh" ||
        client === "kimi_code" ||
        client === "mimocode" ||
        client === "pi_desktop",
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-3">
        <Select value={clientValue} onValueChange={setClientValue}>
          <SelectTrigger size="sm" className="w-52" aria-label="选择客户端">
            <SelectValue />
          </SelectTrigger>
          <FilterSelectContent>
            {CLIENT_FILTERS.map((entry) => (
              <SelectItem key={entry.value} value={entry.value}>
                {entry.label}
              </SelectItem>
            ))}
          </FilterSelectContent>
        </Select>
        <span className="text-xs text-muted-foreground">
          统计本机 Codex、Claude Code、OpenCode、DSH、Kimi Code、MiMoCode 与 Pi Desktop 的已知用量。
        </span>
      </div>

      {overview.error ? <ErrorBlock message={overview.error} onRetry={overview.reload} /> : null}

      {overview.error ? null : overview.loading ? (
        <LoadingRows rows={2} />
      ) : (
        <div className="grid grid-cols-2 gap-4 min-[960px]:grid-cols-4">
          <StatCard
            label="区间 Token"
            value={formatCompactTokens(total)}
            unit={`（${range.label}）`}
          />
          <StatCard label="会话数量" value={formatNumber(overview.data?.range.sessions ?? 0)} />
          <StatCard label="请求明细" value={formatNumber(overview.data?.range.messages ?? 0)} />
          <StatCard
            label="缓存读取占比"
            value={formatPercent(totals?.cacheReadTokens ?? 0, total)}
            hint="缓存读取 ÷ 规范总量"
          />
        </div>
      )}

      <DashboardCard>
        <CardHeader>
          <CardTitle>Token 分桶构成</CardTitle>
          <CardDescription>
            分桶按各客户端口径归一化；缓存写入依据各客户端口径计入规范总量。
          </CardDescription>
        </CardHeader>
        <CardContent>
          {!overview.loading && (overview.data?.range.sessionsTotalMismatch ?? 0) > 0 ? (
            <p className="mb-2 text-xs text-muted-foreground">
              其中 {formatNumber(overview.data?.range.sessionsTotalMismatch ?? 0)}{" "}
              个会话的客户端总量与其分项之和不一致，
              总量按客户端记录值计入（会话明细里可逐条查看）。
            </p>
          ) : null}
          {overview.error ? (
            <ErrorBlock message={overview.error} onRetry={overview.reload} />
          ) : overview.loading ? (
            <LoadingRows rows={4} />
          ) : !overview.data?.range.sessions ? (
            <EmptyBlock
              title="所选区间没有用量数据"
              description="调整时间范围，或先同步本机客户端会话。"
            />
          ) : (
            <ul className="divide-y divide-border">
              {buckets.map((bucket) => (
                <li key={bucket.label} className="flex items-center justify-between gap-4 py-2.5">
                  <div className="min-w-0">
                    <p className="text-sm">
                      {bucket.label}
                      {bucket.counted ? null : (
                        <span className="ml-2 text-xs text-muted-foreground">不计入总量</span>
                      )}
                    </p>
                    <p className="text-xs text-muted-foreground">{bucket.note}</p>
                  </div>
                  <div className="flex shrink-0 items-center gap-4 tabular-nums">
                    <span className="w-14 text-right text-xs text-muted-foreground">
                      {formatPercent(bucket.value, total)}
                    </span>
                    <span className="w-20 text-right font-mono text-xs">
                      {formatNumber(bucket.value)}
                    </span>
                  </div>
                </li>
              ))}
              <li className="flex items-center justify-between gap-4 py-2.5">
                <p className="text-sm font-medium">规范总量</p>
                <span className="w-20 text-right font-mono text-xs font-medium tabular-nums">
                  {formatNumber(total)}
                </span>
              </li>
            </ul>
          )}
        </CardContent>
      </DashboardCard>

      <TokenTrendChart
        rangeLabel={trendRangeLabel}
        buckets={trend.data}
        loading={trend.loading}
        error={trend.error}
        onRetry={trend.reload}
        description="按会话开始时间归属到本地日期；切换客户端筛选会影响本图与下方排行。"
      />

      <ClientDateHeatmap
        rangeLabel={trendRangeLabel}
        buckets={trend.data}
        loading={trend.loading}
        error={trend.error}
        onRetry={trend.reload}
        client={client}
      />

      <div className="grid gap-4 lg:grid-cols-2">
        <RankingCard
          title="模型排行"
          description="按会话主模型聚合。"
          entries={rankings.data?.models ?? null}
          loading={rankings.loading}
          error={rankings.error}
          onRetry={rankings.reload}
          emptyTitle="没有模型数据"
          emptyDescription="所选区间内没有可统计的会话。"
        />
        <RankingCard
          title="项目排行"
          description="按会话工作目录聚合。"
          entries={rankings.data?.projects ?? null}
          loading={rankings.loading}
          error={rankings.error}
          onRetry={rankings.reload}
          emptyTitle="没有项目数据"
          emptyDescription="会话记录里没有可用的工作目录信息。"
          shortenKeys
        />
      </div>
    </div>
  );
}
