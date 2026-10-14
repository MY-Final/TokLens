/** 用量分析：按客户端浏览完整模型用量与请求分项。 */
import { RequestAnalysisPanel } from "@/components/RequestAnalysisPanel";
import { useAsyncData } from "@/hooks/use-async-data";
import { getRequestAnalysis } from "@/lib/api";
import { rangeScope, type DateRange } from "@/lib/ranges";
import type { ClientKey, SessionFilters } from "@/lib/usage-types";

export function AnalyticsPage({
  range,
  refreshToken,
  client,
  onOpenSessions,
}: {
  range: DateRange;
  refreshToken: number;
  client: ClientKey | null;
  onOpenSessions: (filters: Partial<SessionFilters>) => void;
}) {
  const analysis = useAsyncData(
    async () => ({
      result: await getRequestAnalysis({ fromMs: range.fromMs, toMs: range.toMs, client }),
      client,
    }),
    `request-analysis:${range.fromMs ?? "all"}:${range.toMs}:${client ?? "all"}:${refreshToken}`,
    `${rangeScope(range)}:${client ?? "all"}`,
  );
  return (
    <RequestAnalysisPanel
      data={analysis.data?.result ?? null}
      selectedClient={client}
      detailScope={`${rangeScope(range)}:${client ?? "all"}`}
      loading={analysis.loading}
      error={analysis.error}
      onRetry={analysis.reload}
      onOpenSessions={(filters) => onOpenSessions({ ...filters, client, requestTime: true })}
    />
  );
}
