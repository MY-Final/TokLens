/**
 * 面板类型注册表：type → 既有组件的 props 适配器。
 *
 * 纪律：`transform` 只做字段搬运（形状归一），展示体只做格式化，绝不聚合 / 去重 / 估算；
 * 未知与零的展示判定集中在展示体（见 docs/generative-panel-spec.md ③）。
 */

import type { ReactNode } from "react";

import {
  ChartSkeleton,
  ClientListSkeleton,
  HeatmapSkeleton,
  RankingSkeleton,
  TableSkeleton,
} from "@/components/StateBlocks";
import {
  ActivityCalendarBody,
  ClientDistributionBody,
  RankingBody,
  RequestTrendBody,
  SessionTableBody,
  StatPanelBody,
  StatSkeleton,
  TokenTrendBody,
  type ClientDistributionData,
  type RankingData,
  type StatData,
} from "@/components/panels/panel-bodies";
import type { PanelSkeletonMap } from "@/components/panels/PanelShell";
import {
  resolveTimeBasis,
  type PanelRenderContext,
  type PanelSpec,
  type PanelType,
} from "@/lib/panel-spec";
import type {
  ClientUsage,
  DailyTokenBucket,
  OverviewStats,
  RankingEntry,
  Rankings,
  RequestActivity,
  RequestAnalysis,
  RequestAnalysisGroup,
  SessionSummary,
} from "@/lib/usage-types";

const DEFAULT_RANKING_LIMIT = 8;

export type PanelAdapter<TData> = {
  /** 把 `run_panel_query` 的原始载荷搬运为组件所需形状（不聚合、不重算口径）。 */
  transform: (payload: unknown, spec: PanelSpec, context: PanelRenderContext) => TData;
  isEmpty: (data: TData) => boolean;
  render: (data: TData, spec: PanelSpec, context: PanelRenderContext) => ReactNode;
};

type PanelAdapterAny = PanelAdapter<unknown>;

function define<TData>(adapter: PanelAdapter<TData>): PanelAdapterAny {
  return adapter as unknown as PanelAdapterAny;
}

export const PANEL_SKELETONS: PanelSkeletonMap = {
  stat: <StatSkeleton />,
  token_trend: <ChartSkeleton />,
  request_trend: <ChartSkeleton />,
  client_distribution: <ClientListSkeleton />,
  ranking: <RankingSkeleton />,
  session_table: <TableSkeleton />,
  activity_calendar: <HeatmapSkeleton />,
};

function toRankingEntry(group: RequestAnalysisGroup): RankingEntry {
  return {
    key: group.key,
    sessions: group.metrics.totals.sessions,
    messages: group.metrics.totals.requests,
    totalTokens: group.metrics.totals.totals.totalTokens,
    sessionsWithoutUsage: group.metrics.totals.requestsWithoutUsage,
  };
}

export const PANEL_ADAPTERS: Record<PanelType, PanelAdapterAny> = {
  stat: define<StatData>({
    transform: (payload, spec) =>
      resolveTimeBasis(spec) === "request"
        ? { basis: "request", analysis: payload as RequestAnalysis }
        : { basis: "session", stats: payload as OverviewStats },
    isEmpty: () => false,
    render: (data, spec, context) => <StatPanelBody data={data} spec={spec} context={context} />,
  }),
  token_trend: define<DailyTokenBucket[]>({
    transform: (payload) => payload as DailyTokenBucket[],
    isEmpty: (buckets) => !buckets.some((bucket) => bucket.sessions > 0),
    render: (data, spec, context) => <TokenTrendBody data={data} spec={spec} context={context} />,
  }),
  request_trend: define<RequestActivity>({
    transform: (payload) => payload as RequestActivity,
    isEmpty: (data) => data.daily.length === 0,
    render: (data, spec, context) => <RequestTrendBody data={data} spec={spec} context={context} />,
  }),
  client_distribution: define<ClientDistributionData>({
    transform: (payload, spec) =>
      resolveTimeBasis(spec) === "request"
        ? { basis: "requests", usage: (payload as RequestAnalysis).clients }
        : { basis: "sessions", usage: payload as ClientUsage[] },
    isEmpty: (data) =>
      data.basis === "requests"
        ? data.usage.length === 0
        : !data.usage.some((row) => row.sessions > 0),
    render: (data, spec, context) => (
      <ClientDistributionBody data={data} spec={spec} context={context} />
    ),
  }),
  ranking: define<RankingData>({
    transform: (payload, spec) => {
      const dimension = spec.typeOptions.dimension ?? "model";
      const limit = spec.typeOptions.limit ?? DEFAULT_RANKING_LIMIT;
      if (resolveTimeBasis(spec) === "request") {
        const analysis = payload as RequestAnalysis;
        const groups = dimension === "model" ? analysis.models : analysis.projects;
        return { entries: groups.slice(0, limit).map(toRankingEntry), dimension };
      }
      const rankings = payload as Rankings;
      const entries = dimension === "model" ? rankings.models : rankings.projects;
      return { entries: entries.slice(0, limit), dimension };
    },
    isEmpty: (data) => data.entries.length === 0,
    render: (data, spec, context) => <RankingBody data={data} spec={spec} context={context} />,
  }),
  session_table: define<SessionSummary[]>({
    transform: (payload) => payload as SessionSummary[],
    isEmpty: (data) => data.length === 0,
    render: (data, spec, context) => <SessionTableBody data={data} spec={spec} context={context} />,
  }),
  activity_calendar: define<DailyTokenBucket[]>({
    transform: (payload) => payload as DailyTokenBucket[],
    isEmpty: (buckets) => !buckets.some((bucket) => bucket.sessions > 0),
    render: (data, spec, context) => (
      <ActivityCalendarBody data={data} spec={spec} context={context} />
    ),
  }),
};
