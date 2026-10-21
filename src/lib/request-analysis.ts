import type { RequestAnalysisMetrics } from "@/lib/usage-types";
import { formatShare } from "@/lib/format";

/** A known zero remains zero; entirely unknown request totals stay unknown. */
export function knownRequestTokens(metrics: RequestAnalysisMetrics): number | null {
  return metrics.totals.requests > 0 && metrics.totals.requestsWithUsage === 0
    ? null
    : metrics.totals.totals.totalTokens;
}

/** The native query supplies basis points; this only formats their percentage unit. */
export function formatBasisPoints(value: number | null): string {
  return formatShare(value);
}
