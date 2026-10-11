import { RequestActivityPanel } from "@/components/RequestActivityPanel";
import { useAsyncData } from "@/hooks/use-async-data";
import { getRequestActivity } from "@/lib/api";
import { localDayBoundaries } from "@/lib/ranges";
import type { DateRange } from "@/lib/ranges";
import type { ClientKey } from "@/lib/usage-types";

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
  );
  const rangeLabel =
    range.fromMs !== null && range.fromMs < range.dayStartMs
      ? `${range.label} · 图表仅展示最近 400 天，合计包含完整区间`
      : range.label;
  return (
    <RequestActivityPanel
      data={activity.data}
      loading={activity.loading}
      error={activity.error}
      onRetry={activity.reload}
      rangeLabel={rangeLabel}
      client={client}
    />
  );
}
