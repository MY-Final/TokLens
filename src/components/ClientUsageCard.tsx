/** 当前仅 Codex 已接入；未实现的采集器不展示虚构用量。 */

import { DashboardCard } from "@/components/DashboardCard";
import { ClientIdentity } from "@/components/ClientIdentity";
import { Badge } from "@/components/ui/badge";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import type { ClientKey, ClientUsage } from "@/lib/api";
import { formatCompactTokens, formatNumber } from "@/lib/format";

const PENDING_CLIENTS: readonly ClientKey[] = ["claude_code", "opencode"];

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
  const codex = usage?.find((row) => row.client === "codex" && row.supported);
  const hasUsage = codex !== undefined && codex.totals.totalTokens > 0;
  return (
    <DashboardCard className="h-full">
      <CardHeader className="gap-1.5 px-5">
        <CardTitle className="text-sm">客户端用量分布</CardTitle>
        <CardDescription className="text-xs">所选区间 · 目前仅 Codex 已接入</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 px-5">
        {error ? (
          <ErrorBlock message={error} onRetry={onRetry} />
        ) : loading ? (
          <LoadingRows rows={3} />
        ) : (
          <>
            {hasUsage ? (
              <div className="space-y-3">
                <div className="flex items-center justify-between gap-2">
                  <ClientIdentity client="codex" />
                  <span className="font-mono text-sm font-semibold text-primary">100%</span>
                </div>
                <Progress
                  value={100}
                  aria-valuenow={100}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-label="Codex 占已接入客户端 Token 用量的 100%"
                  className="h-2 bg-muted"
                />
                <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                  <span>
                    <span className="font-mono">{formatNumber(codex.sessions)}</span> 个会话 ·{" "}
                    <span className="font-mono">{formatNumber(codex.messages)}</span> 条明细
                  </span>
                  <span
                    className="font-mono"
                    title={`${formatNumber(codex.totals.totalTokens)} Token`}
                  >
                    {formatCompactTokens(codex.totals.totalTokens)} Token
                  </span>
                </div>
              </div>
            ) : (
              <EmptyBlock
                title="暂无 Codex 用量"
                description="调整时间范围，或点击刷新导入本机会话。"
              />
            )}
            <div className="space-y-2 pt-1">
              {PENDING_CLIENTS.map((client) => (
                <div
                  key={client}
                  className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-dashed border-border/70 px-3 py-3 opacity-50"
                >
                  <ClientIdentity client={client} compact />
                  <Badge variant="secondary" className="font-normal">
                    等待采集
                  </Badge>
                </div>
              ))}
            </div>
          </>
        )}
      </CardContent>
    </DashboardCard>
  );
}
