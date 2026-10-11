/** Client distribution: percentages are calculated by the backend. */

import { DashboardCard } from "@/components/DashboardCard";
import { ClientIdentity } from "@/components/ClientIdentity";
import { Badge } from "@/components/ui/badge";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import type { ClientKey, ClientUsage } from "@/lib/usage-types";
import { clientLabel } from "@/lib/client-labels";
import { formatCompactTokens, formatNumber } from "@/lib/format";

const BAR_CLASSES: Record<ClientKey, string> = {
  codex: "[&_[data-slot=progress-indicator]]:bg-client-codex",
  claude_code: "[&_[data-slot=progress-indicator]]:bg-client-claude",
  opencode: "[&_[data-slot=progress-indicator]]:bg-client-opencode",
  dsh: "[&_[data-slot=progress-indicator]]:bg-client-dsh",
  kimi_code: "[&_[data-slot=progress-indicator]]:bg-client-kimi",
  mimocode: "[&_[data-slot=progress-indicator]]:bg-client-mimo",
  pi_desktop: "[&_[data-slot=progress-indicator]]:bg-client-pi",
};

export function ClientUsageCard({
  usage,
  loading,
  error,
  onRetry,
}: {
  usage: ClientUsage[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
}) {
  return (
    <DashboardCard className="h-full">
      <CardHeader className="gap-1.5 px-5">
        <CardTitle className="text-sm">客户端用量分布</CardTitle>
        <CardDescription className="text-xs">所选区间 · 本机采集的已知 Token 总量</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 px-5">
        {error ? (
          <ErrorBlock message={error} onRetry={onRetry} />
        ) : loading ? (
          <LoadingRows rows={4} />
        ) : (
          <>
            {!usage?.some((row) => row.sessions > 0) ? (
              <EmptyBlock
                title="暂无客户端用量"
                description="调整时间范围，或点击刷新导入本机会话。"
              />
            ) : null}
            {usage?.map((row) => (
              <div key={row.client} className="space-y-2.5">
                <div className="flex items-center justify-between gap-2">
                  <ClientIdentity client={row.client} compact />
                  {row.supported && row.sessions > 0 ? (
                    <span className="font-mono text-xs text-muted-foreground">
                      {(row.shareBps / 100).toFixed(1)}%
                    </span>
                  ) : (
                    <Badge variant="secondary" className="font-normal">
                      {row.supported ? "暂无数据" : "等待采集"}
                    </Badge>
                  )}
                </div>
                {row.supported && row.sessions > 0 ? (
                  <>
                    <Progress
                      value={row.shareBps / 100}
                      aria-label={`${clientLabel(row.client)} Token 用量占比`}
                      className={`h-1.5 bg-muted ${BAR_CLASSES[row.client]}`}
                    />
                    <div className="flex flex-wrap items-center justify-between gap-2 font-mono text-xs text-muted-foreground">
                      <span>
                        {formatNumber(row.sessions)} 个会话 · {formatNumber(row.messages)} 条明细
                      </span>
                      <span title={`${formatNumber(row.totals.totalTokens)} Token`}>
                        {formatCompactTokens(row.totals.totalTokens)} Token
                      </span>
                    </div>
                  </>
                ) : null}
              </div>
            ))}
          </>
        )}
      </CardContent>
    </DashboardCard>
  );
}
