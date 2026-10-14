/**
 * 面板类型的展示体：每个 `type` 一个小组件，只做字段搬运与展示格式化。
 * 与 `registry.tsx`（适配器/取数归一）分离，保持各自"只导出组件 / 只导出配置"。
 */

import { addDays } from "date-fns";
import { CalendarDays, Database, ListTree, MessageSquare, type LucideIcon } from "lucide-react";

import { CalendarHeatmap } from "@/components/CalendarHeatmap";
import { ClientUsageCard } from "@/components/ClientUsageCard";
import { RankingCard } from "@/components/RankingCard";
import { RequestActivityPanel } from "@/components/RequestActivityPanel";
import { SessionTable } from "@/components/SessionTable";
import { StatCard } from "@/components/StatCard";
import { TokenTrendChart } from "@/components/TokenTrendChart";
import { Skeleton } from "@/components/ui/skeleton";
import { useTokenFormat } from "@/hooks/use-token-format";
import { formatNumber } from "@/lib/format";
import { knownRequestTokens } from "@/lib/request-analysis";
import { createCustomRange, type DateRange } from "@/lib/ranges";
import {
  panelTitle,
  resolvePanelRange,
  type PanelRenderContext,
  type PanelSpec,
} from "@/lib/panel-spec";
import type {
  ClientUsage,
  DailyTokenBucket,
  OverviewStats,
  RankingEntry,
  RequestActivity,
  RequestAnalysis,
  RequestAnalysisGroup,
  SessionSummary,
  StatsBucket,
} from "@/lib/usage-types";

const noop = () => undefined;

const TOTAL_TOOLTIP =
  "按客户端口径归一化：Codex / OpenCode 使用客户端累计或聚合值，其他客户端按去重后的请求求和。缓存写入依据各客户端口径计入总量；缺失值保持未知。";

type PanelBodyProps = {
  spec: PanelSpec;
  context: PanelRenderContext;
};

function knownTotal(value: StatsBucket | undefined): number | null {
  return !value || (value.sessions > 0 && value.sessionsWithoutUsage === value.sessions)
    ? null
    : value.totals.totalTokens;
}

function totalHint(value: StatsBucket | undefined): string | undefined {
  return value?.sessionsWithoutUsage
    ? `已知子集 · ${formatNumber(value.sessionsWithoutUsage)} 个会话总量未知`
    : value?.sessions
      ? "已知用量"
      : "所选区间暂无会话";
}

function trendRangeLabel(range: DateRange): string {
  return range.fromMs !== null && range.fromMs < range.dayStartMs
    ? `${range.label} · 仅展示最近 400 天`
    : range.label;
}

export function StatSkeleton() {
  return (
    <div className="space-y-3" role="status" aria-busy="true" aria-live="polite">
      <span className="sr-only">正在加载指标…</span>
      <div className="flex items-center gap-2" aria-hidden="true">
        <Skeleton className="size-7 rounded-lg bg-muted" />
        <Skeleton className="h-3 w-20 rounded-sm bg-muted" />
      </div>
      <Skeleton className="h-7 w-32 rounded-sm bg-muted" aria-hidden="true" />
      <Skeleton className="h-3 w-24 rounded-sm bg-muted" aria-hidden="true" />
    </div>
  );
}

/** stat：单个指标卡，字段由 typeOptions.field 选择。 */
export type StatData =
  { basis: "session"; stats: OverviewStats } | { basis: "request"; analysis: RequestAnalysis };

type StatCardModel = {
  label: string;
  value: string;
  unit?: string;
  hint?: string;
  detail?: string;
  tooltip: string;
  icon: LucideIcon;
};

function statCardModel(
  data: StatData,
  spec: PanelSpec,
  range: DateRange,
  formatTokensOrDash: (value: number | null) => string,
): StatCardModel {
  const field = spec.typeOptions.field ?? "rangeTotal";
  const session = data.basis === "session" ? data.stats : null;
  const request = data.basis === "request" ? data.analysis : null;
  const cacheHitRate =
    session?.range.cacheHitBps == null ? "—" : `${(session.range.cacheHitBps / 100).toFixed(1)}%`;

  if (field === "todayTotal") {
    const total = session ? knownTotal(session.today) : null;
    return {
      label: "今日 Token",
      value: formatTokensOrDash(total),
      detail: total === null ? "总量未知" : `${formatNumber(total)} Token`,
      hint: session ? totalHint(session.today) : undefined,
      tooltip: TOTAL_TOOLTIP,
      icon: CalendarDays,
    };
  }
  if (field === "sessions") {
    const count = session ? session.range.sessions : (request?.summary.totals.sessions ?? 0);
    const hint = session
      ? session.range.sessionsWithoutUsage > 0
        ? `其中 ${formatNumber(session.range.sessionsWithoutUsage)} 个无用量数据`
        : session.range.sessions
          ? undefined
          : "所选区间暂无会话"
      : undefined;
    return {
      label: "会话数量",
      value: formatNumber(count),
      hint,
      tooltip:
        "按客户端会话/线程去重后的数量；没有 Token 记录的会话也计入会话数，但不贡献 Token 合计。",
      icon: ListTree,
    };
  }
  if (field === "messages") {
    const count = session ? session.range.messages : (request?.summary.totals.requests ?? 0);
    return {
      label: "请求明细",
      value: formatNumber(count),
      hint: session ? `缓存命中率 ${cacheHitRate}` : undefined,
      tooltip: session
        ? `请求级记录按客户端去重。缓存命中率只使用输入与缓存读取同时已知的 ${formatNumber(session.range.cacheEligibleSessions)} 个会话，不含输出；字段缺失的会话不进入分母。`
        : "请求级记录按客户端去重，不含估算与缺失时间的请求。",
      icon: MessageSquare,
    };
  }
  const total = session
    ? knownTotal(session.range)
    : request
      ? knownRequestTokens(request.summary)
      : null;
  return {
    label: "区间 Token",
    value: formatTokensOrDash(total),
    detail: total === null ? "总量未知" : `${formatNumber(total)} Token`,
    hint: session ? totalHint(session.range) : undefined,
    unit: `（${range.label}）`,
    tooltip: TOTAL_TOOLTIP,
    icon: Database,
  };
}

export function StatPanelBody({ data, spec, context }: { data: StatData } & PanelBodyProps) {
  const { formatTokensOrDash } = useTokenFormat();
  const range = resolvePanelRange(spec, context);
  const model = statCardModel(data, spec, range, formatTokensOrDash);
  return (
    <StatCard
      label={model.label}
      value={model.value}
      unit={model.unit}
      hint={model.hint}
      tooltip={model.tooltip}
      icon={model.icon}
      detail={model.detail}
    />
  );
}

/** token_trend：会话累计日趋势。 */
export function TokenTrendBody({
  data,
  spec,
  context,
}: { data: DailyTokenBucket[] } & PanelBodyProps) {
  const range = resolvePanelRange(spec, context);
  return (
    <TokenTrendChart
      selectedRange={range}
      rangeLabel={trendRangeLabel(range)}
      buckets={data}
      loading={false}
      error={null}
      onRetry={noop}
      defaultMode={spec.typeOptions.stack}
    />
  );
}

/** request_trend：请求时间趋势（整块复用含多 Tab 的面板）。 */
export function RequestTrendBody({
  data,
  spec,
  context,
}: { data: RequestActivity } & PanelBodyProps) {
  const range = resolvePanelRange(spec, context);
  return (
    <RequestActivityPanel
      data={data}
      loading={false}
      error={null}
      onRetry={noop}
      rangeLabel={trendRangeLabel(range)}
      client={spec.query.client}
      selectedRange={range}
      defaultMetric={spec.typeOptions.metric}
    />
  );
}

/** client_distribution：按口径分发到客户端分布卡。 */
export type ClientDistributionData =
  | { basis: "sessions"; usage: ClientUsage[] }
  | { basis: "requests"; usage: RequestAnalysisGroup[] };

export function ClientDistributionBody({
  data,
  spec,
  context,
}: { data: ClientDistributionData } & PanelBodyProps) {
  const range = resolvePanelRange(spec, context);
  if (data.basis === "requests") {
    return (
      <ClientUsageCard
        basis="requests"
        usage={data.usage}
        loading={false}
        error={null}
        onRetry={noop}
        onOpenClient={(client) => context.onOpenSessions({ client, requestTime: true }, range)}
      />
    );
  }
  return (
    <ClientUsageCard
      basis="sessions"
      usage={data.usage}
      loading={false}
      error={null}
      onRetry={noop}
      onOpenClient={(client) => context.onOpenSessions({ client }, range)}
    />
  );
}

/** ranking：模型 / 项目排行。 */
export type RankingData = { entries: RankingEntry[]; dimension: "model" | "project" };

function rankingDescription(dimension: "model" | "project"): string {
  return dimension === "project" ? "项目目录 · 按区间已知 Token" : "会话主模型 · 按区间已知 Token";
}

export function RankingBody({ data, spec, context }: { data: RankingData } & PanelBodyProps) {
  const range = resolvePanelRange(spec, context);
  const projectDimension = data.dimension === "project";
  const title = spec.title?.trim() || (projectDimension ? "项目排行" : "模型排行");
  return (
    <RankingCard
      title={title}
      description={rankingDescription(data.dimension)}
      entries={data.entries}
      loading={false}
      error={null}
      onRetry={noop}
      emptyTitle={projectDimension ? "还没有项目数据" : "还没有模型数据"}
      emptyDescription="执行一次同步后即可看到本机用量。"
      shortenKeys={projectDimension}
      onOpenEntry={(key) => {
        context.onOpenSessions(projectDimension ? { projectPath: key } : { model: key }, range);
      }}
    />
  );
}

/** session_table：会话明细表。 */
export function SessionTableBody({
  data,
  spec,
  context,
}: { data: SessionSummary[] } & PanelBodyProps) {
  const range = resolvePanelRange(spec, context);
  return (
    <SessionTable
      sessions={data}
      loading={false}
      error={null}
      onRetry={noop}
      ariaLabel={panelTitle(spec)}
      onOpenAll={() => context.onOpenSessions(undefined, range)}
    />
  );
}

/** activity_calendar：会话累计活动日历。 */
export function ActivityCalendarBody({
  data,
  spec,
  context,
}: { data: DailyTokenBucket[] } & PanelBodyProps) {
  const range = resolvePanelRange(spec, context);
  const rangeLabel = range.dayCount > 365 ? "最近一年（365 天）" : trendRangeLabel(range);
  return (
    <CalendarHeatmap
      buckets={data}
      loading={false}
      error={null}
      onRetry={noop}
      rangeLabel={rangeLabel}
      selectedRange={range}
      onSelectDay={(day) => {
        const from = Math.max(day, range.fromMs ?? day);
        const to = Math.min(addDays(day, 1).getTime(), range.toMs);
        if (from >= to) return;
        context.onOpenSessions({}, createCustomRange(from, to));
      }}
    />
  );
}
