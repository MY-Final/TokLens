import { useState } from "react";
import { DashboardCard } from "@/components/DashboardCard";
import { EmptyBlock } from "@/components/StateBlocks";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useTokenFormat } from "@/hooks/use-token-format";
import { formatNumber, shortenPath } from "@/lib/format";
import { formatBasisPoints, knownRequestTokens } from "@/lib/request-analysis";
import type { RequestAnalysis } from "@/lib/usage-types";

export function RequestAnalysisDetails({ data, scope }: { data: RequestAnalysis; scope: string }) {
  const { formatTokensOrDash } = useTokenFormat();
  const [dimension, setDimension] = useState("models");
  const totals = data.summary.totals;
  const sums = totals.totals;
  const buckets = [
    {
      field: "cacheReadTokens",
      label: "缓存读取",
      value: sums.cacheReadTokens,
      note: "命中缓存的输入",
      className: "[&_[data-slot=progress-indicator]]:bg-chart-1",
    },
    {
      field: "inputTokens",
      label: "输入",
      value: sums.inputTokens,
      note: "未命中缓存的输入",
      className: "[&_[data-slot=progress-indicator]]:bg-chart-2",
    },
    {
      field: "outputTokens",
      label: "输出",
      value: sums.outputTokens,
      note: "不含已分离的推理",
      className: "[&_[data-slot=progress-indicator]]:bg-chart-3",
    },
    {
      field: "reasoningTokens",
      label: "推理",
      value: sums.reasoningTokens,
      note: "只统计已记录的字段",
      className: "[&_[data-slot=progress-indicator]]:bg-chart-4",
    },
    {
      field: "cacheWriteTokens",
      label: "缓存写入",
      value: sums.countedCacheWriteTokens,
      note: "仅计入总量的部分",
      className: "[&_[data-slot=progress-indicator]]:bg-chart-5",
    },
  ]
    .map((bucket) => ({
      ...bucket,
      value: data.summary.knownTokenFields.includes(bucket.field) ? bucket.value : null,
    }))
    .sort((a, b) =>
      a.value === null ? (b.value === null ? 0 : 1) : b.value === null ? -1 : b.value - a.value,
    );
  const max = Math.max(0, ...buckets.map((entry) => entry.value ?? 0));
  const rankings = dimension === "models" ? data.models : data.projects;
  return (
    <div className="grid items-start gap-4 xl:grid-cols-2">
      <DashboardCard>
        <CardHeader className="gap-1.5 px-5">
          <CardTitle className="text-sm">Token 花在哪里</CardTitle>
          <CardDescription className="text-xs">
            {scope} · 已记录请求的已知分项 · 条形相对最大分项
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3 px-5">
          {totals.requests === 0 ? (
            <EmptyBlock title="没有可分析的请求" description="选择其他客户端或时间范围。" />
          ) : (
            <>
              <div className="flex flex-wrap items-baseline justify-between gap-2 border-b pb-3 text-xs">
                <span className="text-muted-foreground">
                  已知总量 · {formatNumber(totals.requests)} 条请求
                </span>
                <span
                  className="font-mono font-medium tabular-nums"
                  title={`${knownRequestTokens(data.summary) === null ? "未知" : formatNumber(sums.totalTokens)} Token`}
                >
                  {formatTokensOrDash(knownRequestTokens(data.summary))} Token
                </span>
              </div>
              <ul className="space-y-3">
                {buckets.map((bucket) => (
                  <li key={bucket.label}>
                    <div className="mb-1.5 flex items-center justify-between gap-3 text-xs">
                      <span>
                        {bucket.label}
                        <span className="ml-2 text-muted-foreground">{bucket.note}</span>
                      </span>
                      <span
                        className="shrink-0 font-mono tabular-nums"
                        title={`${bucket.value === null ? "未知" : formatNumber(bucket.value)} Token`}
                      >
                        {formatTokensOrDash(bucket.value)}
                      </span>
                    </div>
                    <Progress
                      className={`h-1.5 bg-muted ${bucket.className}`}
                      value={max > 0 ? ((bucket.value ?? 0) / max) * 100 : 0}
                      aria-label={`${bucket.label}，相对最大分项的已知用量`}
                    />
                  </li>
                ))}
              </ul>
              <p className="text-xs leading-relaxed text-muted-foreground">
                各分项仅累加已知字段，缺失部分不推算；总量独立取请求记录，不能用分项补造。缓存写入按客户端口径计入。
              </p>
              {totals.requestsWithoutUsage > 0 ? (
                <p className="text-xs text-muted-foreground">
                  {formatNumber(totals.requestsWithoutUsage)} 条请求总量未知，仍保留已知分项。
                </p>
              ) : null}
            </>
          )}
        </CardContent>
      </DashboardCard>
      <DashboardCard>
        <CardHeader className="flex flex-wrap items-start justify-between gap-3 px-5">
          <div className="space-y-1.5">
            <CardTitle className="text-sm">主要用量来源</CardTitle>
            <CardDescription className="text-xs">
              {scope} · Top 8 ·{" "}
              {dimension === "models" ? "按会话主模型归属，非逐请求模型" : "按会话项目目录归属"}
            </CardDescription>
          </div>
          <Tabs value={dimension} onValueChange={setDimension} aria-label="用量来源维度">
            <TabsList className="h-8">
              <TabsTrigger className="text-xs" value="models">
                模型
              </TabsTrigger>
              <TabsTrigger className="text-xs" value="projects">
                项目
              </TabsTrigger>
            </TabsList>
          </Tabs>
        </CardHeader>
        <CardContent className="px-5">
          {rankings.length === 0 ? (
            <EmptyBlock
              title="没有用量来源记录"
              description="选择其他客户端或时间范围，或刷新本地数据。"
            />
          ) : (
            <Table className="table-fixed text-xs">
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead className="pl-0">
                    {dimension === "models" ? "会话主模型" : "项目"}
                  </TableHead>
                  <TableHead className="w-24 text-right">已知 Token</TableHead>
                  <TableHead className="w-16 text-right">占比</TableHead>
                  <TableHead className="w-16 pr-0 text-right">请求</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rankings.map((entry) => {
                  const metrics = entry.metrics;
                  const total = knownRequestTokens(metrics);
                  const label =
                    entry.key === null
                      ? dimension === "models"
                        ? "未记录模型"
                        : "未记录项目"
                      : dimension === "models"
                        ? entry.key
                        : shortenPath(entry.key);
                  return (
                    <TableRow key={entry.key ?? "unknown"}>
                      <TableCell className="py-2.5 pl-0">
                        <span className="block truncate font-mono" title={entry.key ?? label}>
                          {label}
                        </span>
                        {metrics.totals.requestsWithoutUsage > 0 ? (
                          <span className="text-muted-foreground">
                            {formatNumber(metrics.totals.requestsWithoutUsage)} 条用量未知
                          </span>
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
                      <TableCell className="pr-0 text-right font-mono tabular-nums">
                        {formatNumber(metrics.totals.requests)}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
          {rankings.length > 0 ? (
            <p className="mt-3 text-xs text-muted-foreground">
              占比基于当前客户端范围的全部已知 Token，Top 8 之外的用量仍在分母中。
            </p>
          ) : null}
        </CardContent>
      </DashboardCard>
    </div>
  );
}
