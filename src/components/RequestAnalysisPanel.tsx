import { DataExplanation } from "@/components/DataExplanation";
import { RequestAnalysisDetails } from "@/components/RequestAnalysisDetails";
import { ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { TabsContent } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { useTokenFormat } from "@/hooks/use-token-format";
import { GROK_USAGE_NOTE, clientLabel } from "@/lib/client-labels";
import { formatNumber } from "@/lib/format";
import { knownRequestTokens } from "@/lib/request-analysis";
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
  const scope = selectedClient ? clientLabel(selectedClient) : "全部客户端";
  const selectedTab = selectedClient ?? "all";
  const totals = data?.summary.totals;
  const knownTokens = data && totals?.requests ? knownRequestTokens(data.summary) : null;

  return (
    <TabsContent value={selectedTab} className="min-w-0 space-y-5">
      <div className="flex flex-wrap items-baseline gap-3">
        <h2 className="text-base font-medium">{scope} · 模型用量</h2>
        {data ? (
          <span className="text-xs text-muted-foreground">
            {formatNumber(data.models.length)} 个模型分组
          </span>
        ) : null}
        {onOpenSessions ? (
          <Button size="sm" variant="outline" onClick={() => onOpenSessions({})}>
            查看区间会话
          </Button>
        ) : null}
      </div>
      {error ? <ErrorBlock message={error} onRetry={onRetry} /> : null}
      {selectedClient === "grok" ||
      (selectedClient === null &&
        data?.clients.some(
          (entry) => entry.key === "grok" && entry.metrics.totals.requests > 0,
        )) ? (
        <p className="text-xs text-muted-foreground">{GROK_USAGE_NOTE}</p>
      ) : null}
      {loading ? (
        <LoadingRows rows={6} />
      ) : data ? (
        <>
          <dl className="flex flex-wrap gap-x-6 gap-y-2 text-xs text-muted-foreground">
            <div className="flex items-baseline gap-2">
              <dt>已知 Token</dt>
              <dd
                className="font-mono text-sm text-foreground tabular-nums"
                title={
                  knownTokens === null ? "未记录可用总量" : `${formatNumber(knownTokens)} Token`
                }
              >
                {formatTokensOrDash(knownTokens)}
              </dd>
            </div>
            <div className="flex items-baseline gap-2">
              <dt>请求</dt>
              <dd className="font-mono text-sm text-foreground tabular-nums">
                {formatNumber(data.summary.totals.requests)}
              </dd>
            </div>
            {data.summary.totals.requestsWithoutUsage > 0 ? (
              <div className="flex items-baseline gap-2">
                <dt>总量未知</dt>
                <dd className="text-foreground">
                  {formatNumber(data.summary.totals.requestsWithoutUsage)} 条请求
                </dd>
              </div>
            ) : null}
          </dl>
          <RequestAnalysisDetails
            key={detailScope}
            data={data}
            scope={scope}
            onOpenSessions={onOpenSessions}
          />
        </>
      ) : null}

      <DataExplanation summary="请求时间 · 按会话主模型归属 · 不含估算">
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
