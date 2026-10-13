import { useTokenFormat } from "@/hooks/use-token-format";
/** Client distribution: percentages are calculated by the backend. */

import { DashboardCard } from "@/components/DashboardCard";
import { ClientIdentity } from "@/components/ClientIdentity";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import type { ClientKey, ClientUsage, RequestAnalysisGroup } from "@/lib/usage-types";
import { clientLabel, isClientKey } from "@/lib/client-labels";
import { formatNumber } from "@/lib/format";

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
};

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
  const { formatTokens } = useTokenFormat();
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
  return (
    <DashboardCard className="h-full">
      <CardHeader className="gap-1.5 px-5">
        <CardTitle className="text-sm">客户端用量分布</CardTitle>
        <CardDescription className="text-xs">
          {props.basis === "requests" ? "按请求时间" : "按会话开始日"} · 已知 Token 占比
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 px-5">
        {error ? (
          <ErrorBlock message={error} onRetry={onRetry} />
        ) : loading ? (
          <LoadingRows rows={4} />
        ) : (
          <>
            {rows.length === 0 ? (
              <EmptyBlock
                title="暂无客户端用量"
                description="调整时间范围，或点击刷新导入本机会话。"
              />
            ) : null}
            {rows.map((row) => (
              <div key={row.client} className="space-y-2.5">
                <div className="flex items-center justify-between gap-2">
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
                  <span className="font-mono text-xs text-muted-foreground">
                    {row.shareBps === null ? "—" : `${(row.shareBps / 100).toFixed(1)}%`}
                  </span>
                </div>
                <>
                  {row.shareBps !== null ? (
                    <Progress
                      value={row.shareBps / 100}
                      aria-label={`${clientLabel(row.client)} Token 用量占比`}
                      className={`h-1.5 bg-muted ${BAR_CLASSES[row.client]}`}
                    />
                  ) : null}
                  <div className="flex flex-wrap items-center justify-between gap-2 font-mono text-xs text-muted-foreground">
                    <span>
                      {formatNumber(row.sessions)} 个会话 · {formatNumber(row.messages)}{" "}
                      {props.basis === "requests" ? "条请求" : "条明细"}
                    </span>
                    <span
                      title={
                        row.unknown === row.count ? "总量未知" : `${formatNumber(row.total)} Token`
                      }
                    >
                      {row.unknown === row.count ? "—" : formatTokens(row.total)} Token
                    </span>
                  </div>
                  {row.unknown > 0 ? (
                    <p className="text-xs text-muted-foreground">
                      {row.unknown < row.count ? "已知子集 · " : ""}
                      {formatNumber(row.unknown)} {props.basis === "requests" ? "条请求" : "个会话"}
                      总量未知
                    </p>
                  ) : null}
                </>
              </div>
            ))}
          </>
        )}
      </CardContent>
    </DashboardCard>
  );
}
