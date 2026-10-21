import { useTokenFormat } from "@/hooks/use-token-format";
import { RequestActivityPanel } from "@/components/RequestActivityPanel";
import { ClientUsageCard } from "@/components/ClientUsageCard";
import { OverviewKpiGrid, type OverviewKpi } from "@/components/OverviewKpiGrid";
import { useAsyncData } from "@/hooks/use-async-data";
import { getRequestActivity } from "@/lib/api";
import { localDayBoundaries, localHourBoundaries } from "@/lib/ranges";
import { rangeScope, type DateRange } from "@/lib/ranges";
import type { ClientKey, RequestTotals, SessionFilters } from "@/lib/usage-types";
import { GROK_USAGE_NOTE } from "@/lib/client-labels";
import { formatNumber } from "@/lib/format";
import { Coins, ListTree, MessageSquare } from "lucide-react";
import { ErrorBlock, StatCardsSkeleton } from "@/components/StateBlocks";

const TOKEN_TOOLTIP =
  "统计到的 Token 总量：按请求发生时间把每条请求的 Token 加总。部分请求没有用量记录时只显示已知部分，缺失的不会按 0 计算。";
const REQUEST_TOOLTIP =
  "区间内已记录的请求条数，按请求发生时间归属；没有时间的请求与估算记录不计入。";
const SESSION_TOOLTIP = "区间内有请求记录的会话数量，同一会话跨天只算一次；没有请求的会话不计入。";

/** 核心指标：只使用后端已返回的统计值，区分未记录、未知与已知子集。 */
function requestKpis(
  totals: RequestTotals | null,
  formatTokens: (value: number) => string,
): OverviewKpi[] {
  const requests = totals?.requests ?? 0;
  const sessions = totals?.sessions ?? 0;
  const unknown = totals?.requestsWithoutUsage ?? 0;
  const recorded = requests > 0;
  const tokenKnown = recorded && (totals?.requestsWithUsage ?? 0) > 0;
  const totalTokens = totals?.totals.totalTokens ?? 0;
  return [
    {
      label: "已知 Token",
      value: tokenKnown ? formatTokens(totalTokens) : "—",
      detail: tokenKnown ? `${formatNumber(totalTokens)} Token` : undefined,
      // 只在有例外时给提示，正常情况不占用版面。
      hint: !recorded
        ? "所选区间未记录请求"
        : tokenKnown
          ? unknown > 0
            ? `${formatNumber(unknown)} 条请求用量未知`
            : undefined
          : "请求用量未知",
      tooltip: TOKEN_TOOLTIP,
      icon: Coins,
    },
    {
      label: "请求数",
      value: formatNumber(requests),
      hint: recorded ? undefined : "所选区间未记录请求",
      tooltip: REQUEST_TOOLTIP,
      icon: MessageSquare,
    },
    {
      label: "有请求会话数",
      value: formatNumber(sessions),
      hint: sessions === 0 ? "所选区间没有会话" : undefined,
      tooltip: SESSION_TOOLTIP,
      icon: ListTree,
    },
  ];
}

export function RequestActivityView({
  range,
  refreshToken,
  client = null,
  onOpenSessions,
}: {
  range: DateRange;
  refreshToken: number;
  client?: ClientKey | null;
  onOpenSessions?: (filters: Partial<SessionFilters>) => void;
}) {
  const { formatTokens } = useTokenFormat();
  const activity = useAsyncData(
    () =>
      getRequestActivity({
        fromMs: range.fromMs,
        toMs: range.toMs,
        dayBoundaries: localDayBoundaries(range),
        hourlyBoundaries: localHourBoundaries(range),
        client,
      }),
    `requests:${client ?? "all"}:${range.fromMs ?? "all"}:${range.toMs}:${range.dayStartMs}:${range.dayCount}:${refreshToken}`,
    `${rangeScope(range)}:${client ?? "all"}`,
  );
  const rangeLabel =
    range.fromMs !== null && range.fromMs < range.dayStartMs
      ? `${range.label} · 图表仅展示最近 400 天，合计包含完整区间`
      : range.label;
  const error = activity.error;
  const hasData = !!activity.data;
  const kpis = requestKpis(activity.data?.range ?? null, formatTokens);
  return (
    <div className="space-y-4">
      {client === "grok" ||
      (client === null &&
        activity.data?.clients.some(
          (entry) => entry.key === "grok" && entry.metrics.totals.requests > 0,
        )) ? (
        <p className="text-xs text-muted-foreground">{GROK_USAGE_NOTE}</p>
      ) : null}
      {error ? <ErrorBlock message={error} onRetry={activity.reload} /> : null}
      {!error && activity.loading && !hasData ? (
        <StatCardsSkeleton cards={3} />
      ) : hasData ? (
        <OverviewKpiGrid items={kpis} />
      ) : null}
      {error && !hasData ? null : (
        <>
          <RequestActivityPanel
            data={activity.data}
            loading={activity.loading}
            rangeLabel={rangeLabel}
            client={client}
            selectedRange={range}
          />
          <ClientUsageCard
            onOpenClient={
              onOpenSessions
                ? (selectedClient) => onOpenSessions({ client: selectedClient, requestTime: true })
                : undefined
            }
            basis="requests"
            usage={activity.data?.clients ?? null}
            loading={activity.loading}
            error={null}
            onRetry={activity.reload}
          />
        </>
      )}
    </div>
  );
}
