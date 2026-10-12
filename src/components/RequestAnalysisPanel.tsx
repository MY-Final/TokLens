import { ClientIdentity } from "@/components/ClientIdentity";
import { DashboardCard } from "@/components/DashboardCard";
import { DataExplanation } from "@/components/DataExplanation";
import { RequestAnalysisDetails } from "@/components/RequestAnalysisDetails";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { Button } from "@/components/ui/button";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useTokenFormat } from "@/hooks/use-token-format";
import { clientLabel, isClientKey } from "@/lib/client-labels";
import { formatNumber } from "@/lib/format";
import { formatBasisPoints, knownRequestTokens } from "@/lib/request-analysis";
import type { ClientKey, RequestAnalysis } from "@/lib/usage-types";

export function RequestAnalysisPanel({
  data,
  selectedClient,
  rangeLabel,
  loading,
  error,
  onRetry,
  onSelectClient,
}: {
  data: RequestAnalysis | null;
  selectedClient: ClientKey | null;
  rangeLabel: string;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  onSelectClient: (client: ClientKey | null) => void;
}) {
  const { formatTokensOrDash } = useTokenFormat();
  const leader = data?.clients.find((entry) => entry.metrics.totals.requestsWithUsage > 0);
  const leaderName = leader && isClientKey(leader.key) ? clientLabel(leader.key) : "—";
  const summary = data?.summary;
  const scope = selectedClient ? clientLabel(selectedClient) : "全部客户端";
  return (
    <div className="space-y-4">
      <DataExplanation summary="按请求时间 · 实测 / 直接换算 · 不含估算">
        <p>客户端比较覆盖完整所选区间。点击客户端后，下方构成、均量与排行仅分析该客户端。</p>
        <p>
          请求均量＝已知总 Token ÷ 有总量记录的请求数；不把未知请求按零计入。缓存命中率＝缓存读取
          ÷（未命中输入＋缓存读取），仅使用两个字段都已知的请求。这两项为直接换算。
        </p>
        <p>
          模型按会话主模型归属请求用量，并非逐请求模型；项目按会话目录归属。不以 Token
          推算费用或生产力。
        </p>
        {data ? (
          <p>
            全历史采集覆盖（与所选日期独立）：缺少时间 {formatNumber(data.coverage.untimedRequests)}{" "}
            条、估算 {formatNumber(data.coverage.estimatedRequests)} 条已排除；
            {formatNumber(data.coverage.sessionsPartial)} 个会话明细不完整。分析只代表已记录请求。
          </p>
        ) : null}
      </DataExplanation>
      {error ? <ErrorBlock message={error} onRetry={onRetry} /> : null}
      {loading ? (
        <LoadingRows rows={6} />
      ) : data ? (
        <>
          <DashboardCard className="gap-0 py-4">
            <CardContent className="px-5">
              <dl className="grid grid-cols-3 gap-4">
                {[
                  {
                    label: "区间用量最多",
                    value: leaderName,
                    note: leader
                      ? `${formatTokensOrDash(knownRequestTokens(leader.metrics))} Token · ${formatBasisPoints(leader.metrics.shareBps)}`
                      : "没有已知 Token，暂无法比较",
                    exact: "按已知 Token 比较全部客户端",
                  },
                  {
                    label: "单次请求平均用量",
                    value: formatTokensOrDash(summary?.averageTokens ?? null),
                    note: `${scope} · 仅已知用量请求`,
                    exact:
                      summary?.averageTokens === null
                        ? "没有已知总量，均量未知"
                        : `${formatNumber(summary?.averageTokens ?? 0)} Token / 请求`,
                  },
                  {
                    label: "输入缓存命中率",
                    value: formatBasisPoints(summary?.cacheHitBps ?? null),
                    note: `${scope} · ${formatNumber(summary?.cacheKnownRequests ?? 0)} 条字段已知`,
                    exact: "缓存读取 ÷（未命中输入＋缓存读取），仅使用两个字段都已知的请求",
                  },
                ].map((metric) => (
                  <div key={metric.label} className="min-w-0 space-y-1.5">
                    <dt className="text-xs text-muted-foreground">{metric.label}</dt>
                    <dd
                      className="font-mono text-lg font-semibold break-all tabular-nums"
                      title={metric.exact}
                    >
                      {metric.value}
                    </dd>
                    <dd className="text-xs leading-relaxed text-muted-foreground">{metric.note}</dd>
                  </div>
                ))}
              </dl>
            </CardContent>
          </DashboardCard>
          <DashboardCard>
            <CardHeader className="flex flex-wrap items-start justify-between gap-3 px-5">
              <div className="space-y-1.5">
                <CardTitle className="text-sm">客户端用量对比</CardTitle>
                <CardDescription className="text-xs">
                  {rangeLabel} · 按已知 Token 降序 · 点击客户端查看构成与排行
                </CardDescription>
              </div>
              <Button
                size="sm"
                variant={selectedClient === null ? "secondary" : "outline"}
                aria-pressed={selectedClient === null}
                onClick={() => onSelectClient(null)}
              >
                全部客户端
              </Button>
            </CardHeader>
            <CardContent className="px-5">
              {data.clients.length === 0 ? (
                <EmptyBlock
                  title="此区间未记录可定位的请求"
                  description="调整日期，或刷新同步本地客户端数据；缺少时间和估算请求不进入分析。"
                />
              ) : (
                <Table className="text-xs">
                  <TableHeader>
                    <TableRow className="hover:bg-transparent">
                      <TableHead className="pl-0">客户端</TableHead>
                      <TableHead className="text-right">已知 Token</TableHead>
                      <TableHead className="text-right">占比</TableHead>
                      <TableHead className="text-right">请求</TableHead>
                      <TableHead className="text-right">Token / 请求</TableHead>
                      <TableHead className="pr-0 text-right">缓存命中</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {data.clients.map((entry) => {
                      const key = entry.key;
                      const metrics = entry.metrics;
                      const total = knownRequestTokens(metrics);
                      return (
                        <TableRow
                          key={key}
                          data-state={selectedClient === key ? "selected" : undefined}
                        >
                          <TableCell className="pl-0">
                            {isClientKey(key) ? (
                              <Button
                                variant="ghost"
                                size="sm"
                                className="h-auto px-0 py-1"
                                aria-pressed={selectedClient === key}
                                aria-label={`分析 ${clientLabel(key)} 用量`}
                                onClick={() => onSelectClient(key)}
                              >
                                <ClientIdentity client={key} compact />
                              </Button>
                            ) : (
                              (key ?? "未知客户端")
                            )}
                            {metrics.totals.requestsWithoutUsage > 0 ? (
                              <p className="mt-1 text-muted-foreground">
                                {formatNumber(metrics.totals.requestsWithoutUsage)} 条用量未知
                              </p>
                            ) : null}
                          </TableCell>
                          <TableCell
                            className="text-right font-mono tabular-nums"
                            title={total === null ? "用量未知" : `${formatNumber(total)} Token`}
                          >
                            {formatTokensOrDash(total)}
                          </TableCell>
                          <TableCell className="text-right font-mono tabular-nums">
                            {formatBasisPoints(metrics.shareBps)}
                          </TableCell>
                          <TableCell className="text-right font-mono tabular-nums">
                            {formatNumber(metrics.totals.requests)}
                          </TableCell>
                          <TableCell
                            className="text-right font-mono tabular-nums"
                            title={
                              metrics.averageTokens === null
                                ? "没有已知总量，均量未知"
                                : `${formatNumber(metrics.averageTokens)} Token / 请求`
                            }
                          >
                            {formatTokensOrDash(metrics.averageTokens)}
                          </TableCell>
                          <TableCell className="pr-0 text-right font-mono tabular-nums">
                            {formatBasisPoints(metrics.cacheHitBps)}
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </DashboardCard>
          <RequestAnalysisDetails data={data} scope={scope} />
        </>
      ) : null}
    </div>
  );
}
