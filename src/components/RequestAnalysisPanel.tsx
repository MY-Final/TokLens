import { DataExplanation } from "@/components/DataExplanation";
import {
  RequestAnalysisDetails,
  RequestAnalysisProjects,
} from "@/components/RequestAnalysisDetails";
import { DashboardCard } from "@/components/DashboardCard";
import { OverviewKpiGrid } from "@/components/OverviewKpiGrid";
import { ErrorBlock, DetailSkeleton } from "@/components/StateBlocks";
import { useSyncing } from "@/hooks/use-sync-status";
import { useStatisticMode } from "@/hooks/use-statistic-mode";
import { TabsContent } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useTokenFormat } from "@/hooks/use-token-format";
import { GROK_USAGE_NOTE, clientLabel } from "@/lib/client-labels";
import { formatNumber } from "@/lib/format";
import { knownRequestTokens } from "@/lib/request-analysis";
import { Coins, ListTree, MessageSquare } from "lucide-react";
import type { ClientKey, RequestAnalysis, SessionFilters } from "@/lib/usage-types";

export function RequestAnalysisPanel({
  data,
  selectedClient,
  loading,
  error,
  onRetry,
  detailScope,
  onOpenSessions,
}: {
  data: RequestAnalysis | null;
  selectedClient: ClientKey | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  detailScope: string;
  onOpenSessions?: (filters: Partial<SessionFilters>) => void;
}) {
  const { formatTokensOrDash } = useTokenFormat();
  const { attribution } = useStatisticMode();
  const syncing = useSyncing();
  const scope = selectedClient ? clientLabel(selectedClient) : "全部客户端";
  const selectedTab = selectedClient ?? "all";
  const totals = data?.summary.totals;
  const knownTokens = data && totals?.requests ? knownRequestTokens(data.summary) : null;
  const unknownRequests = totals?.requestsWithoutUsage ?? 0;

  return (
    <TabsContent value={selectedTab} className="min-w-0 space-y-4">
      {error ? <ErrorBlock message={error} onRetry={onRetry} /> : null}
      {selectedClient === "grok" ||
      (selectedClient === null &&
        data?.clients.some(
          (entry) => entry.key === "grok" && entry.metrics.totals.requests > 0,
        )) ? (
        <p className="text-xs text-muted-foreground">{GROK_USAGE_NOTE}</p>
      ) : null}
      {!data && (loading || syncing) ? (
        <DetailSkeleton />
      ) : data ? (
        <>
          <OverviewKpiGrid
            items={[
              {
                label: "已知 Token",
                value: formatTokensOrDash(knownTokens),
                detail: knownTokens === null ? "总量未知" : `${formatNumber(knownTokens)} Token`,
                hint:
                  unknownRequests > 0
                    ? `已知子集 · ${formatNumber(unknownRequests)} 条请求用量未知`
                    : undefined,
                tooltip:
                  "按客户端口径归一化后由请求明细求和；没有请求明细的会话使用客户端自报的累计值，缺失值保持未知。",
                icon: Coins,
              },
              {
                label: "请求数",
                value: formatNumber(totals?.requests ?? 0),
                hint:
                  unknownRequests > 0 ? `${formatNumber(unknownRequests)} 条用量未知` : undefined,
                tooltip: "所选区间内已记录的请求条数，按请求发生时间归属。",
                icon: MessageSquare,
              },
              {
                label: "有请求会话数",
                value: formatNumber(totals?.sessions ?? 0),
                tooltip: "所选区间内至少有一条已记录请求的会话数，同一会话跨天只计一次。",
                icon: ListTree,
              },
            ]}
          />
          <DashboardCard>
            <CardHeader className="gap-1.5 px-5">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                  <CardTitle className="text-sm font-medium">{scope} · 模型用量</CardTitle>
                  <CardDescription className="text-xs">
                    {attribution} · 按会话主模型归属 · {formatNumber(data.models.length)} 个模型分组
                  </CardDescription>
                </div>
                {onOpenSessions ? (
                  <Button size="sm" variant="outline" onClick={() => onOpenSessions({})}>
                    查看区间会话
                  </Button>
                ) : null}
              </div>
            </CardHeader>
            <CardContent className="px-5">
              <RequestAnalysisDetails
                key={detailScope}
                data={data}
                scope={scope}
                onOpenSessions={onOpenSessions}
              />
            </CardContent>
          </DashboardCard>
          <DashboardCard>
            <CardHeader className="gap-1.5 px-5">
              <CardTitle className="text-sm font-medium">项目用量</CardTitle>
              <CardDescription className="text-xs">
                {attribution} · 按会话目录归属 · 默认折叠
              </CardDescription>
            </CardHeader>
            <CardContent className="px-5">
              <RequestAnalysisProjects data={data} onOpenSessions={onOpenSessions} />
            </CardContent>
          </DashboardCard>
        </>
      ) : null}

      <DataExplanation summary={`${attribution} · 按会话主模型归属 · 不含估算`}>
        <p>
          同一会话切换模型时，仍归入会话主模型；当前无法准确拆分每次请求的实际模型。全部客户端下，相同模型合并统计。
        </p>
        <p>
          模型列表包含完整区间的全部模型分组。占比以所选客户端完整区间的全部已知 Token
          为分母，未知用量不补零。项目保留 Top 8，分母不截断。
        </p>
        <p>
          分项独立累加已知值，不由分项补总量；缓存写入单列客户端自报值，计入总量的部分另行标注。均量＝已知
          Token ÷ 有总量的请求数；缓存命中率仅使用输入和缓存读取都已知的请求，两项为换算值。
        </p>
        {data ? (
          <p>
            全历史覆盖（不受日期筛选）：排除无时间 {formatNumber(data.coverage.untimedRequests)}{" "}
            条、估算 {formatNumber(data.coverage.estimatedRequests)} 条；
            {formatNumber(data.coverage.sessionsPartial)} 个会话明细不完整，
            {formatNumber(data.coverage.sessionsWithoutRequests)}{" "}
            个会话没有请求明细，仅代表已记录请求。
          </p>
        ) : null}
      </DataExplanation>
    </TabsContent>
  );
}
