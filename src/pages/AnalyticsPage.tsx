/** 用量分析：分桶构成 + 趋势 + 排行，均按所选区间与客户端筛选。 */

import { EmptyBlock } from "@/components/StateBlocks";
import { DashboardCard } from "@/components/DashboardCard";

import { useState } from "react";

import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { RankingCard } from "@/components/RankingCard";
import { StatCard } from "@/components/StatCard";
import { ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { TokenTrendChart } from "@/components/TokenTrendChart";
import { useAsyncData } from "@/hooks/use-async-data";
import { getDailyTokenTrend, getOverviewStats, getRankings } from "@/lib/api";
import type { ClientKey } from "@/lib/usage-types";
import { formatCompactTokens, formatNumber, formatPercent } from "@/lib/format";
import type { DateRange } from "@/lib/ranges";

const MS_PER_DAY = 86_400_000;

const CLIENT_FILTERS: ReadonlyArray<{ value: string; label: string; client: ClientKey | null }> = [
  { value: "all", label: "全部客户端", client: null },
  { value: "codex", label: "Codex", client: "codex" },
  { value: "claude_code", label: "Claude Code（等待采集）", client: "claude_code" },
  { value: "opencode", label: "OpenCode（等待采集）", client: "opencode" },
];

export function AnalyticsPage({ range, refreshToken }: { range: DateRange; refreshToken: number }) {
  const [clientValue, setClientValue] = useState("all");
  const selected = CLIENT_FILTERS.find((entry) => entry.value === clientValue) ?? CLIENT_FILTERS[0];
  const client = selected?.client ?? null;
  const keySuffix = `${range.key}:${clientValue}:${range.fromMs ?? "all"}:${range.toMs}:${refreshToken}`;

  const overview = useAsyncData(
    () =>
      getOverviewStats({
        fromMs: range.fromMs,
        toMs: range.toMs,
        todayFromMs: 0,
      }),
    `analytics-overview:${keySuffix}`,
  );
  const trend = useAsyncData(
    () =>
      getDailyTokenTrend({
        fromMs: range.fromMs ?? range.toMs - range.dayCount * MS_PER_DAY,
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
      note: "已扣除客户端记录中命中缓存的输入部分",
      counted: true,
    },
    {
      label: "输出（不含推理）",
      value: totals?.outputTokens ?? 0,
      note: "已扣除推理 token，避免与 output 重复计算",
      counted: true,
    },
    { label: "推理", value: totals?.reasoningTokens ?? 0, note: "Codex 单独记录", counted: true },
    {
      label: "缓存读取",
      value: totals?.cacheReadTokens ?? 0,
      note: "命中缓存的输入",
      counted: true,
    },
    {
      label: "缓存写入",
      value: totals?.cacheWriteTokens ?? 0,
      note: "与输入的关系未确认，不计入总量",
      counted: false,
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-3">
        <Select value={clientValue} onValueChange={setClientValue}>
          <SelectTrigger size="sm" className="w-52" aria-label="选择客户端">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {CLIENT_FILTERS.map((entry) => (
              <SelectItem
                key={entry.value}
                value={entry.value}
                disabled={entry.value !== "all" && entry.value !== "codex"}
              >
                {entry.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span className="text-xs text-muted-foreground">
          当前只统计已接入的 Codex 数据；Claude Code、OpenCode 采集器尚未实现。
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
            分桶互不重叠，可直接相加得到规范总量；缓存写入独立展示，不计入总量。
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
              description="调整时间范围，或先同步本机 Codex 会话。"
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
        rangeLabel={range.label}
        buckets={trend.data}
        loading={trend.loading}
        error={trend.error}
        onRetry={trend.reload}
        description="按会话开始时间归属到本地日期；切换客户端筛选会影响本图与下方排行。"
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
