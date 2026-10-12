import { RequestActivityPanel } from "@/components/RequestActivityPanel";
import { ClientUsageCard } from "@/components/ClientUsageCard";
import { useAsyncData } from "@/hooks/use-async-data";
import { getRequestActivity } from "@/lib/api";
import { localDayBoundaries } from "@/lib/ranges";
import { rangeScope, type DateRange } from "@/lib/ranges";
import type { ClientKey } from "@/lib/usage-types";
import { GROK_USAGE_NOTE } from "@/lib/client-labels";

export function RequestActivityView({
  range,
  refreshToken,
  client = null,
}: {
  range: DateRange;
  refreshToken: number;
  client?: ClientKey | null;
}) {
  const activity = useAsyncData(
    () =>
      getRequestActivity({
        fromMs: range.fromMs,
        toMs: range.toMs,
        dayBoundaries: localDayBoundaries(range),
        client,
      }),
    `requests:${client ?? "all"}:${range.fromMs ?? "all"}:${range.toMs}:${range.dayStartMs}:${range.dayCount}:${refreshToken}`,
    `${rangeScope(range)}:${client ?? "all"}`,
  );
  const rangeLabel =
    range.fromMs !== null && range.fromMs < range.dayStartMs
      ? `${range.label} · 图表仅展示最近 400 天，合计包含完整区间`
      : range.label;
  return (
    <div className="space-y-4">
      {client === "grok" ||
      (client === null &&
        activity.data?.clients.some(
          (entry) => entry.key === "grok" && entry.metrics.totals.requests > 0,
        )) ? (
        <p className="text-xs text-muted-foreground">{GROK_USAGE_NOTE}</p>
      ) : null}
      <RequestActivityPanel
        data={activity.data}
        loading={activity.loading}
        error={activity.error}
        onRetry={activity.reload}
        rangeLabel={rangeLabel}
        client={client}
        selectedRange={range}
      />
      <ClientUsageCard
        basis="requests"
        usage={activity.data?.clients ?? null}
        loading={activity.loading}
        error={activity.error}
        onRetry={activity.reload}
      />
    </div>
  );
}
