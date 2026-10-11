/** 总览：统计卡片 + 趋势 + 分布 + 排行 + 最近会话（全部来自真实采集数据）。 */

import { DashboardCard } from "@/components/DashboardCard";
import { CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";

import { CalendarDays, Database, ListTree, MessageSquare } from "lucide-react";

import type { PageKey } from "@/components/AppShell";
import { ClientUsageCard } from "@/components/ClientUsageCard";
import { RankingCard } from "@/components/RankingCard";
import { SessionTable } from "@/components/SessionTable";
import { StatCard } from "@/components/StatCard";
import { ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { TokenTrendChart } from "@/components/TokenTrendChart";
import { useAsyncData } from "@/hooks/use-async-data";
import {
  getClientUsage,
  getDailyTokenTrend,
  getOverviewStats,
  getRankings,
  getSessions,
} from "@/lib/api";
import { formatCompactTokens, formatNumber, formatPercent, formatTokensOrDash } from "@/lib/format";
import type { DateRange } from "@/lib/ranges";

const TOTAL_TOOLTIP =
  "规范总量 = 未命中缓存输入 + 非推理输出 + 推理 + 缓存读取；缓存写入是客户端独立上报值，" +
  "与输入的关系未确认，因此不计入总量。会话总量取自客户端累计值，子集字段已扣除，不会重复计算。";

const MS_PER_DAY = 86_400_000;

export function OverviewPage({
  range,
  refreshToken,
  onNavigate,
}: {
  range: DateRange;
  refreshToken: number;
  onNavigate: (page: PageKey) => void;
}) {
  const keySuffix = `${range.key}:${range.fromMs ?? "all"}:${range.toMs}:${refreshToken}`;
  const todayStart = new Date(new Date().setHours(0, 0, 0, 0)).getTime();

  const overview = useAsyncData(
    () =>
      getOverviewStats({
        fromMs: range.fromMs,
        toMs: range.toMs,
        todayFromMs: todayStart,
      }),
    `overview:${keySuffix}`,
  );
  const trend = useAsyncData(
    () =>
      getDailyTokenTrend({
        fromMs: range.fromMs ?? range.toMs - range.dayCount * MS_PER_DAY,
        dayCount: range.dayCount,
      }),
    `trend:${keySuffix}`,
  );
  const clients = useAsyncData(
    () => getClientUsage({ fromMs: range.fromMs, toMs: range.toMs }),
    `clients:${keySuffix}`,
  );
  const rankings = useAsyncData(
    () => getRankings({ fromMs: range.fromMs, toMs: range.toMs, limit: 5 }),
    `rankings:${keySuffix}`,
  );
  const sessions = useAsyncData(
    () => getSessions({ fromMs: range.fromMs, toMs: range.toMs, limit: 8 }),
    `sessions:${keySuffix}`,
  );

  const stats = overview.data;
  const bucket = stats?.range;
  const today = stats?.today;
  const cacheRead = bucket?.totals.cacheReadTokens ?? 0;
  const input = bucket?.totals.inputTokens ?? 0;
  const cacheHitRate = input + cacheRead > 0 ? formatPercent(cacheRead, input + cacheRead) : "—";

  return (
    <div className="space-y-4">
      {overview.error ? (
        <ErrorBlock message={overview.error} onRetry={overview.reload} />
      ) : overview.loading ? (
        <div className="grid grid-cols-2 gap-4 min-[960px]:grid-cols-4">
          {Array.from({ length: 4 }, (_, index) => (
            <DashboardCard key={index} className="px-5">
              <LoadingRows rows={2} />
            </DashboardCard>
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-4 min-[960px]:grid-cols-4">
          <StatCard
            label="今日 Token"
            value={formatCompactTokens(today?.totals.totalTokens ?? 0)}
            detail={`${formatNumber(today?.totals.totalTokens ?? 0)} Token`}
            hint={`${formatNumber(today?.sessions ?? 0)} 个会话 · ${formatNumber(today?.messages ?? 0)} 条明细`}
            tooltip={TOTAL_TOOLTIP}
            icon={CalendarDays}
          />
          <StatCard
            label="区间 Token"
            value={formatCompactTokens(bucket?.totals.totalTokens ?? 0)}
            detail={`${formatNumber(bucket?.totals.totalTokens ?? 0)} Token`}
            unit={`（${range.label}）`}
            hint={bucket?.totals.totalTokens ? "Codex 占比 100% · 实测" : "Codex · 暂无用量"}
            tooltip={TOTAL_TOOLTIP}
            icon={Database}
          />
          <StatCard
            label="会话数量"
            value={formatNumber(bucket?.sessions ?? 0)}
            hint={
              bucket && bucket.sessionsWithoutUsage > 0
                ? `其中 ${formatNumber(bucket.sessionsWithoutUsage)} 个无用量数据`
                : bucket?.sessions
                  ? "全部会话都有用量数据"
                  : "所选区间暂无会话"
            }
            tooltip="按客户端会话/线程去重后的数量；没有 Token 记录的会话也计入会话数，但不贡献 Token 合计。"
            icon={ListTree}
          />
          <StatCard
            label="请求明细"
            value={formatNumber(bucket?.messages ?? 0)}
            hint={`缓存命中率 ${cacheHitRate}`}
            tooltip="请求级记录数（按客户端去重键去重）。缓存命中率只统计缓存读取与未命中输入，不含输出。"
            icon={MessageSquare}
          />
        </div>
      )}

      <TokenTrendChart
        rangeLabel={range.label}
        buckets={trend.data}
        loading={trend.loading}
        error={trend.error}
        onRetry={trend.reload}
      />

      <div className="grid gap-4 lg:grid-cols-2">
        <ClientUsageCard
          usage={clients.data}
          loading={clients.loading}
          error={clients.error}
          onRetry={clients.reload}
        />
        <RankingCard
          title="模型排行"
          description="按会话主模型聚合 · Top 5（并非逐请求模型）"
          entries={rankings.data?.models ?? null}
          loading={rankings.loading}
          error={rankings.error}
          onRetry={rankings.reload}
          emptyTitle="还没有模型数据"
          emptyDescription="执行一次同步后即可看到本机模型用量。"
        />
      </div>

      <RankingCard
        title="项目排行"
        description="按会话工作目录聚合 · Top 5"
        entries={rankings.data?.projects ?? null}
        loading={rankings.loading}
        error={rankings.error}
        onRetry={rankings.reload}
        emptyTitle="还没有项目数据"
        emptyDescription="会话记录里没有可用的工作目录信息。"
        shortenKeys
      />

      <DashboardCard>
        <CardHeader className="gap-1.5 px-5">
          <CardTitle className="text-sm">最近会话</CardTitle>
          <CardDescription className="text-xs">仅展示统计元数据与用量</CardDescription>
        </CardHeader>
        <CardContent className="px-5">
          <SessionTable
            sessions={sessions.data}
            loading={sessions.loading}
            error={sessions.error}
            onRetry={sessions.reload}
            onOpenAll={() => onNavigate("sessions")}
          />
        </CardContent>
      </DashboardCard>

      <p className="text-xs leading-relaxed text-muted-foreground">
        口径说明：所有数值来自本机 Codex 会话文件的实测字段。 缺失值显示为"—"，不按 0
        计入合计；费用没有可靠来源，页面不展示任何金额。
        {bucket && bucket.sessionsPartial > 0
          ? ` 当前区间有 ${formatNumber(bucket.sessionsPartial)} 个会话的请求明细不完整（源文件被分页或续接），其总量仍取客户端累计值。`
          : ""}
        {bucket && bucket.sessionsTotalMismatch > 0
          ? ` 另有 ${formatNumber(bucket.sessionsTotalMismatch)} 个会话的客户端总量与其分项之和不一致，已按客户端值计入并标记。`
          : ""}
        {today
          ? ` 今日合计（本地日历）${formatTokensOrDash(today.totals.totalTokens)} Token。`
          : ""}
      </p>
    </div>
  );
}
