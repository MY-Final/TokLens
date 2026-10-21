import { useTokenFormat } from "@/hooks/use-token-format";
/** Client distribution: percentages are calculated by the backend. */

import { DashboardCard } from "@/components/DashboardCard";
import { ClientIdentity } from "@/components/ClientIdentity";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { EmptyBlock, ErrorBlock, ClientListSkeleton } from "@/components/StateBlocks";
import { useSyncing } from "@/hooks/use-sync-status";
import { useStatisticMode } from "@/hooks/use-statistic-mode";
import type { ClientKey, ClientUsage, RequestAnalysisGroup } from "@/lib/usage-types";
import { clientLabel, isClientKey } from "@/lib/client-labels";
import { formatNumber, formatShare } from "@/lib/format";

const BAR_CLASSES: Record<ClientKey, string> = {
  codex: "[&_[data-slot=progress-indicator]]:bg-client-codex",
  claude_code: "[&_[data-slot=progress-indicator]]:bg-client-claude",
  opencode: "[&_[data-slot=progress-indicator]]:bg-client-opencode",
  dsh: "[&_[data-slot=progress-indicator]]:bg-client-dsh",
  kimi_code: "[&_[data-slot=progress-indicator]]:bg-client-kimi",
  mimocode: "[&_[data-slot=progress-indicator]]:bg-client-mimo",
  pi_desktop: "[&_[data-slot=progress-indicator]]:bg-client-pi",
  workbuddy: "[&_[data-slot=progress-indicator]]:bg-client-workbuddy",
  copilot: "[&_[data-slot=progress-indicator]]:bg-client-copilot",
  gemini_cli: "[&_[data-slot=progress-indicator]]:bg-client-gemini",
  grok: "[&_[data-slot=progress-indicator]]:bg-client-grok",
  mcode: "[&_[data-slot=progress-indicator]]:bg-client-mcode",
  cline: "[&_[data-slot=progress-indicator]]:bg-client-cline",
};

/** 排行行：名称、已知 Token、请求数与占比分列，数值右对齐便于比对。 */
const RANKING_COLUMNS = "grid grid-cols-[minmax(0,1fr)_6rem_3.5rem] items-center gap-x-4";

type ClientUsageCardProps = {
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  onOpenClient?: (client: ClientKey) => void;
} & (
  | { basis?: "sessions"; usage: ClientUsage[] | null }
  | { basis: "requests"; usage: RequestAnalysisGroup[] | null }
);

export function ClientUsageCard(props: ClientUsageCardProps) {
  const { loading, error, onRetry } = props;
  const syncing = useSyncing();
  const { formatTokens } = useTokenFormat();
  const { attribution } = useStatisticMode();
  const rows =
    props.basis === "requests"
      ? (props.usage ?? []).flatMap(({ key, metrics }) =>
          isClientKey(key) && metrics.totals.requests > 0
            ? [
                {
                  client: key,
                  sessions: metrics.totals.sessions,
                  count: metrics.totals.requests,
                  messages: metrics.totals.requests,
                  unknown: metrics.totals.requestsWithoutUsage,
                  total: metrics.totals.totals.totalTokens,
                  shareBps: metrics.shareBps,
                },
              ]
            : [],
        )
      : (props.usage ?? [])
          .filter((row) => row.sessions > 0)
          .map((row) => ({
            client: row.client,
            sessions: row.sessions,
            count: row.sessions,
            messages: row.messages,
            unknown: row.sessionsWithoutUsage,
            total: row.totals.totalTokens,
            shareBps: row.sessionsWithoutUsage === row.sessions ? null : row.shareBps,
          }));
  const countLabel = props.basis === "requests" ? "条请求" : "条明细";
  const unknownLabel = props.basis === "requests" ? "条请求" : "个会话";
  // 排行按已知 Token 降序；总量未知的项排在最后。
  const sorted = [...rows].sort((a, b) => {
    const aUnknown = a.unknown === a.count;
    const bUnknown = b.unknown === b.count;
    if (aUnknown !== bUnknown) return aUnknown ? 1 : -1;
    return b.total - a.total;
  });
  return (
    <DashboardCard className="h-full">
      <CardHeader className="gap-1.5 px-5">
        <CardTitle className="text-sm font-medium">客户端用量排行</CardTitle>
        <CardDescription className="text-xs">
          {attribution} · 已知 Token 占比 · 按 Token 降序
        </CardDescription>
      </CardHeader>
      <CardContent className="px-5">
        {error ? (
          <ErrorBlock message={error} onRetry={onRetry} />
        ) : rows.length === 0 && (loading || syncing) ? (
          <ClientListSkeleton rows={4} />
        ) : rows.length === 0 ? (
          <EmptyBlock title="暂无客户端用量" description="调整时间范围，或点击刷新导入本机会话。" />
        ) : (
          <div className="space-y-3">
            <div
              className={`${RANKING_COLUMNS} border-b border-border/60 pb-2 text-xs text-muted-foreground`}
            >
              <span>客户端</span>
              <span className="text-right">已知 Token</span>
              <span className="text-right">占比</span>
            </div>
            {sorted.map((row) => (
              <div
                key={row.client}
                className="space-y-1.5"
                title={`${clientLabel(row.client)} · 已知 Token ${
                  row.unknown === row.count ? "未知" : formatNumber(row.total)
                } · 占比 ${formatShare(row.shareBps)} · ${formatNumber(row.sessions)} 个会话 · ${formatNumber(row.messages)} ${countLabel}`}
              >
                <div className={RANKING_COLUMNS}>
                  <div className="min-w-0 space-y-0.5">
                    {props.onOpenClient ? (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-auto p-0"
                        onClick={() => props.onOpenClient?.(row.client)}
                        aria-label={`查看 ${clientLabel(row.client)} 会话`}
                      >
                        <ClientIdentity client={row.client} compact />
                      </Button>
                    ) : (
                      <ClientIdentity client={row.client} compact />
                    )}
                    <p className="truncate text-xs text-muted-foreground">
                      {formatNumber(row.sessions)} 个会话 · {formatNumber(row.messages)}{" "}
                      {countLabel}
                    </p>
                  </div>
                  <span
                    className="text-right font-mono text-sm font-semibold tabular-nums"
                    title={
                      row.unknown === row.count ? "总量未知" : `${formatNumber(row.total)} Token`
                    }
                  >
                    {row.unknown === row.count ? "—" : formatTokens(row.total)}
                  </span>
                  <span className="text-right font-mono text-xs text-muted-foreground tabular-nums">
                    {formatShare(row.shareBps)}
                  </span>
                </div>
                {row.shareBps !== null ? (
                  <Progress
                    value={row.shareBps / 100}
                    aria-label={`${clientLabel(row.client)} Token 用量占比`}
                    className={`h-1.5 bg-muted ${BAR_CLASSES[row.client]}`}
                  />
                ) : null}
                {row.unknown > 0 ? (
                  <p className="text-xs text-muted-foreground">
                    {row.unknown < row.count ? "已知子集 · " : ""}
                    {formatNumber(row.unknown)} {unknownLabel} 总量未知
                  </p>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </DashboardCard>
  );
}
