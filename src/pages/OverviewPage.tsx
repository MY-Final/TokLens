import { useTokenFormat } from "@/hooks/use-token-format";
/** 总览：统计卡片 + 趋势 + 分布 + 排行 + 最近会话（全部来自真实采集数据）。 */

import { DashboardCard } from "@/components/DashboardCard";
import { DataExplanation } from "@/components/DataExplanation";
import { CardContent, CardHeader, CardTitle } from "@/components/ui/card";

import { CalendarDays, Database, ListTree, MessageSquare } from "lucide-react";
import { addDays, startOfDay } from "date-fns";
import { useState } from "react";

import type { PageKey } from "@/components/AppShell";
import { ClientUsageCard } from "@/components/ClientUsageCard";
import { CalendarHeatmap } from "@/components/CalendarHeatmap";
import { RequestActivityView } from "@/components/RequestActivityView";
import { UsageBasisSelect } from "@/components/UsageBasisSelect";
import type { UsageBasis } from "@/components/UsageBasisSelect";
import { FilterSelectContent } from "@/components/FilterSelectContent";
import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
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
import { formatNumber } from "@/lib/format";
import type { SessionFilters, StatsBucket } from "@/lib/usage-types";
import { createCustomRange, rangeScope, type DateRange } from "@/lib/ranges";

const TOTAL_TOOLTIP =
  "按客户端口径归一化：Codex / OpenCode 使用客户端累计或聚合值，其他客户端按去重后的请求求和。缓存写入依据各客户端口径计入总量；缺失值保持未知。";

type OverviewPageProps = {
  range: DateRange;
  nowMs: number;
  refreshToken: number;
  onNavigate: (page: PageKey) => void;
  onOpenSessions: (filters?: Partial<SessionFilters>, range?: DateRange) => void;
  basis: UsageBasis;
  onBasisChange: (basis: UsageBasis) => void;
};

export function OverviewPage(props: OverviewPageProps) {
  const { basis, onBasisChange } = props;
  return (
    <div className="space-y-4">
      <UsageBasisSelect value={basis} onChange={onBasisChange} />
      {basis === "requests" ? (
        <RequestActivityView
          range={props.range}
          refreshToken={props.refreshToken}
          onOpenSessions={props.onOpenSessions}
        />
      ) : (
        <SessionOverview {...props} />
      )}
    </div>
  );
}

function SessionOverview({ range, nowMs, refreshToken, onOpenSessions }: OverviewPageProps) {
  const { formatTokensOrDash } = useTokenFormat();
  const [calendarPeriod, setCalendarPeriod] = useState("year");
  const keySuffix = `${range.key}:${range.fromMs ?? "all"}:${range.toMs}:${refreshToken}`;
  const todayStart = startOfDay(nowMs).getTime();

  const overview = useAsyncData(
    () =>
      getOverviewStats({
        fromMs: range.fromMs,
        toMs: range.toMs,
        todayFromMs: todayStart,
      }),
    `overview:${todayStart}:${keySuffix}`,
    rangeScope(range),
  );
  const trend = useAsyncData(
    () =>
      getDailyTokenTrend({
        fromMs: range.fromMs ?? range.dayStartMs,
        toMs: range.toMs,
        dayStartMs: range.dayStartMs,
        dayCount: range.dayCount,
      }),
    `trend:${keySuffix}`,
    rangeScope(range),
  );
  const calendarStart = addDays(startOfDay(new Date(range.toMs - 1)), -364).getTime();
  const calendar = useAsyncData(
    () => getDailyTokenTrend({ fromMs: calendarStart, toMs: range.toMs, dayCount: 365 }),
    `calendar:${calendarStart}:${range.toMs}:${refreshToken}`,
    `calendar:${calendarStart}`,
  );
  const calendarData = calendarPeriod === "year" ? calendar : trend;
  const trendRangeLabel =
    range.fromMs !== null && range.fromMs < range.dayStartMs
      ? `${range.label} · 仅展示最近 400 天`
      : range.label;
  const clients = useAsyncData(
    () => getClientUsage({ fromMs: range.fromMs, toMs: range.toMs }),
    `clients:${keySuffix}`,
    rangeScope(range),
  );
  const rankings = useAsyncData(
    () => getRankings({ fromMs: range.fromMs, toMs: range.toMs, limit: 5 }),
    `rankings:${keySuffix}`,
    rangeScope(range),
  );
  const sessions = useAsyncData(
    () => getSessions({ fromMs: range.fromMs, toMs: range.toMs, limit: 8 }),
    `sessions:${keySuffix}`,
    rangeScope(range),
  );

  const stats = overview.data;
  const bucket = stats?.range;
  const today = stats?.today;
  const cacheHitRate =
    bucket?.cacheHitBps == null ? "—" : `${(bucket.cacheHitBps / 100).toFixed(1)}%`;
  const knownTotal = (value: StatsBucket | undefined) =>
    !value || (value.sessions > 0 && value.sessionsWithoutUsage === value.sessions)
      ? null
      : value.totals.totalTokens;
  const totalHint = (value: StatsBucket | undefined) =>
    value?.sessionsWithoutUsage
      ? `已知子集 · ${formatNumber(value.sessionsWithoutUsage)} 个会话总量未知`
      : value?.sessions
        ? "已知用量"
        : "所选区间暂无会话";

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
            value={formatTokensOrDash(knownTotal(today))}
            detail={
              knownTotal(today) === null
                ? "总量未知"
                : `${formatNumber(knownTotal(today) ?? 0)} Token`
            }
            hint={totalHint(today)}
            tooltip={TOTAL_TOOLTIP}
            icon={CalendarDays}
          />
          <StatCard
            label="区间 Token"
            value={formatTokensOrDash(knownTotal(bucket))}
            detail={
              knownTotal(bucket) === null
                ? "总量未知"
                : `${formatNumber(knownTotal(bucket) ?? 0)} Token`
            }
            unit={`（${range.label}）`}
            hint={totalHint(bucket)}
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
                  ? undefined
                  : "所选区间暂无会话"
            }
            tooltip="按客户端会话/线程去重后的数量；没有 Token 记录的会话也计入会话数，但不贡献 Token 合计。"
            icon={ListTree}
          />
          <StatCard
            label="请求明细"
            value={formatNumber(bucket?.messages ?? 0)}
            hint={`缓存命中率 ${cacheHitRate}`}
            tooltip={`请求级记录按客户端去重。缓存命中率只使用输入与缓存读取同时已知的 ${formatNumber(bucket?.cacheEligibleSessions ?? 0)} 个会话，不含输出；字段缺失的会话不进入分母。`}
            icon={MessageSquare}
          />
        </div>
      )}

      <TokenTrendChart
        selectedRange={range}
        rangeLabel={trendRangeLabel}
        buckets={trend.data}
        loading={trend.loading}
        error={trend.error}
        onRetry={trend.reload}
      />

      <CalendarHeatmap
        onSelectDay={(day) => {
          const from = Math.max(
            day,
            calendarPeriod === "year" ? calendarStart : (range.fromMs ?? day),
          );
          const to = Math.min(addDays(day, 1).getTime(), range.toMs);
          onOpenSessions({}, createCustomRange(from, to));
        }}
        selectedRange={
          calendarPeriod === "year" ? { fromMs: calendarStart, toMs: range.toMs } : range
        }
        rangeLabel={calendarPeriod === "year" ? "最近一年（365 天）" : trendRangeLabel}
        buckets={calendarData.data}
        loading={calendarData.loading}
        error={calendarData.error}
        onRetry={calendarData.reload}
        controls={
          <Select value={calendarPeriod} onValueChange={setCalendarPeriod}>
            <SelectTrigger size="sm" className="w-36" aria-label="选择活动日历时间范围">
              <SelectValue />
            </SelectTrigger>
            <FilterSelectContent>
              <SelectItem value="year">最近一年</SelectItem>
              <SelectItem value="selected">跟随页面日期</SelectItem>
            </FilterSelectContent>
          </Select>
        }
      />

      <div className="grid gap-4 lg:grid-cols-2">
        <ClientUsageCard
          onOpenClient={(client) => onOpenSessions({ client })}
          usage={clients.data}
          loading={clients.loading}
          error={clients.error}
          onRetry={clients.reload}
        />
        <RankingCard
          onOpenEntry={(model) => onOpenSessions({ model })}
          title="模型排行"
          description="会话主模型 · Top 5"
          entries={rankings.data?.models ?? null}
          loading={rankings.loading}
          error={rankings.error}
          onRetry={rankings.reload}
          emptyTitle="还没有模型数据"
          emptyDescription="执行一次同步后即可看到本机模型用量。"
        />
      </div>

      <RankingCard
        onOpenEntry={(projectPath) => onOpenSessions({ projectPath })}
        title="项目排行"
        description="项目目录 · Top 5"
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
        </CardHeader>
        <CardContent className="px-5">
          <SessionTable
            sessions={sessions.data}
            loading={sessions.loading}
            error={sessions.error}
            onRetry={sessions.reload}
            onOpenAll={() => onOpenSessions()}
          />
        </CardContent>
      </DashboardCard>

      <DataExplanation
        summary={`会话累计 · 不含估算${bucket?.sessionsPartial ? ` · ${formatNumber(bucket.sessionsPartial)} 个明细不完整` : ""}${bucket?.sessionsTotalMismatch ? ` · ${formatNumber(bucket.sessionsTotalMismatch)} 个总量异常` : ""}`}
      >
        <p>按会话开始日归属累计用量。— 表示未知，不计入合计。</p>
        <p>模型按会话主模型归属，非逐请求模型；项目按会话目录归属。</p>
        <p>明细不完整时仍展示会话累计值；分项与客户端总量不一致时保留客户端值。</p>
      </DataExplanation>
    </div>
  );
}
