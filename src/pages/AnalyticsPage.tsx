/** 用量分析：客户端比较、Token 构成与请求用量归因。 */
import { useState } from "react";
import { RequestAnalysisPanel } from "@/components/RequestAnalysisPanel";
import { useAsyncData } from "@/hooks/use-async-data";
import { getRequestAnalysis } from "@/lib/api";
import type { DateRange } from "@/lib/ranges";
import type { ClientKey } from "@/lib/usage-types";

export function AnalyticsPage({ range, refreshToken }: { range: DateRange; refreshToken: number }) {
  const [client, setClient] = useState<ClientKey | null>(null);
  const analysis = useAsyncData(
    async () => ({
      result: await getRequestAnalysis({ fromMs: range.fromMs, toMs: range.toMs, client }),
      client,
      rangeLabel: range.label,
    }),
    `request-analysis:${range.fromMs ?? "all"}:${range.toMs}:${client ?? "all"}:${refreshToken}`,
  );
  return (
    <RequestAnalysisPanel
      data={analysis.data?.result ?? null}
      selectedClient={analysis.data?.client ?? null}
      rangeLabel={analysis.data?.rangeLabel ?? range.label}
      loading={analysis.loading}
      error={analysis.error}
      onRetry={analysis.reload}
      onSelectClient={setClient}
    />
  );
}
