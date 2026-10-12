import { Fragment, useId, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { EmptyBlock } from "@/components/StateBlocks";
import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useTokenFormat } from "@/hooks/use-token-format";
import { formatNumber, shortenPath } from "@/lib/format";
import { formatBasisPoints, knownRequestTokens } from "@/lib/request-analysis";
import type { RequestAnalysis, RequestAnalysisMetrics } from "@/lib/usage-types";

function TokenBreakdown({ metrics }: { metrics: RequestAnalysisMetrics }) {
  const { formatTokensOrDash } = useTokenFormat();
  const sums = metrics.totals.totals;
  const fields = [
    { field: "inputTokens", label: "输入", value: sums.inputTokens, note: "未命中缓存的输入" },
    {
      field: "cacheReadTokens",
      label: "缓存读取",
      value: sums.cacheReadTokens,
      note: "命中缓存的输入",
    },
    { field: "outputTokens", label: "输出", value: sums.outputTokens, note: "不含已分离的推理" },
    {
      field: "reasoningTokens",
      label: "推理",
      value: sums.reasoningTokens,
      note: "已单列的推理 Token",
    },
    {
      field: "cacheWriteTokens",
      label: "缓存写入",
      value: sums.cacheWriteTokens,
      note: "客户端自报缓存写入，是否计入总量取决于客户端口径",
    },
  ];
  return (
    <div className="space-y-3 whitespace-normal">
      <p className="text-xs text-muted-foreground">
        Token 分项 · 实测 {formatNumber(metrics.totals.measuredRequests)} 条 · 换算{" "}
        {formatNumber(metrics.totals.derivedRequests)} 条
      </p>
      <dl className="grid grid-cols-2 gap-4 md:grid-cols-5">
        {fields.map(({ field, label, value, note }) => {
          const known = metrics.knownTokenFields.includes(field) ? value : null;
          return (
            <div key={field} className="min-w-0 space-y-1">
              <dt className="text-xs text-muted-foreground" title={note}>
                {label}
              </dt>
              <dd
                className="truncate font-mono text-sm tabular-nums"
                title={known === null ? "客户端未记录该字段" : `${formatNumber(known)} Token`}
              >
                {formatTokensOrDash(known)}
              </dd>
            </div>
          );
        })}
      </dl>
      {metrics.knownTokenFields.includes("cacheWriteTokens") ? (
        <p className="text-xs text-muted-foreground">
          缓存写入中计入总量：
          <span
            className="font-mono tabular-nums"
            title={`${formatNumber(sums.countedCacheWriteTokens)} Token`}
          >
            {formatTokensOrDash(sums.countedCacheWriteTokens)}
          </span>{" "}
          Token
        </p>
      ) : null}
      <div className="flex flex-wrap gap-x-5 gap-y-1 text-xs text-muted-foreground">
        <span title="已知 Token ÷ 有总量的请求数">
          均量 {formatTokensOrDash(metrics.averageTokens)} Token / 请求
        </span>
        <span
          title={`缓存读取 ÷（输入＋缓存读取），${formatNumber(metrics.cacheKnownRequests)} 条成对已知样本`}
        >
          输入缓存命中率 {formatBasisPoints(metrics.cacheHitBps)} ·{" "}
          {formatNumber(metrics.cacheKnownRequests)} 条样本
        </span>
      </div>
      <p className="text-xs text-muted-foreground">
        分项仅累加已知值，缺失显示 —，不由分项补总量。
        {metrics.totals.requestsWithoutUsage > 0
          ? ` ${formatNumber(metrics.totals.requestsWithoutUsage)} 条请求总量未知。`
          : ""}
      </p>
    </div>
  );
}

export function RequestAnalysisDetails({ data, scope }: { data: RequestAnalysis; scope: string }) {
  const { formatTokensOrDash } = useTokenFormat();
  const [expandedModel, setExpandedModel] = useState<string | null>(null);
  const detailId = useId();

  if (data.models.length === 0) {
    return (
      <EmptyBlock
        title="当前范围没有请求记录"
        description={`调整日期范围，或到数据源管理检查${scope === "全部客户端" ? "" : ` ${scope} 的`}采集状态。`}
      />
    );
  }

  return (
    <div className="space-y-5">
      <Table className="table-fixed text-xs">
        <TableHeader>
          <TableRow className="bg-muted/40 hover:bg-muted/40">
            <TableHead className="w-[40%]">会话主模型</TableHead>
            <TableHead className="text-right" aria-sort="descending">
              已知 Token ↓
            </TableHead>
            <TableHead className="w-20 text-right">占比</TableHead>
            <TableHead className="w-16 text-right">请求</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {data.models.map((entry, index) => {
            const key = JSON.stringify(entry.key);
            const isExpanded = expandedModel === key;
            const metrics = entry.metrics;
            const total = knownRequestTokens(metrics);
            const label = entry.key ?? "未记录模型";
            const rowId = `${detailId}-${index}`;
            return (
              <Fragment key={key}>
                <TableRow className="border-border/60">
                  <TableCell className="py-3">
                    <button
                      type="button"
                      className="flex w-full items-center gap-2 rounded-sm text-left focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                      onClick={() => setExpandedModel(isExpanded ? null : key)}
                      aria-expanded={isExpanded}
                      aria-controls={rowId}
                    >
                      {isExpanded ? (
                        <ChevronDown
                          className="size-3.5 shrink-0 text-muted-foreground"
                          aria-hidden="true"
                        />
                      ) : (
                        <ChevronRight
                          className="size-3.5 shrink-0 text-muted-foreground"
                          aria-hidden="true"
                        />
                      )}
                      <span className="min-w-0 font-mono break-all whitespace-normal">{label}</span>
                    </button>
                    {metrics.totals.requestsWithoutUsage > 0 ? (
                      <p className="mt-1 pl-5.5 text-muted-foreground">
                        {total === null
                          ? "用量未知"
                          : `已知子集 · ${formatNumber(metrics.totals.requestsWithoutUsage)} 条未知`}
                      </p>
                    ) : null}
                  </TableCell>
                  <TableCell className="py-3 text-right">
                    <span
                      className="block truncate font-mono tabular-nums"
                      title={total === null ? "用量未知" : `${formatNumber(total)} Token`}
                    >
                      {formatTokensOrDash(total)}
                    </span>
                    {metrics.shareBps !== null ? (
                      <Progress
                        value={metrics.shareBps / 100}
                        className="mt-2 ml-auto h-1 max-w-40 bg-muted [&_[data-slot=progress-indicator]]:bg-chart-1"
                        aria-label={`${label} 占所选客户端已知 Token 的比例`}
                      />
                    ) : null}
                  </TableCell>
                  <TableCell className="text-right font-mono tabular-nums">
                    {formatBasisPoints(metrics.shareBps)}
                  </TableCell>
                  <TableCell className="text-right font-mono tabular-nums">
                    {formatNumber(metrics.totals.requests)}
                  </TableCell>
                </TableRow>
                <TableRow id={rowId} hidden={!isExpanded} className="bg-muted/20 hover:bg-muted/20">
                  <TableCell colSpan={4} className="p-4">
                    {isExpanded ? <TokenBreakdown metrics={metrics} /> : null}
                  </TableCell>
                </TableRow>
              </Fragment>
            );
          })}
        </TableBody>
      </Table>

      <details className="group border-t border-border/60 pt-3">
        <summary className="flex w-fit cursor-pointer list-none items-center gap-1.5 rounded-sm text-xs text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none">
          <ChevronRight
            className="size-3.5 transition-transform group-open:rotate-90"
            aria-hidden="true"
          />
          项目用量 · Top 8
        </summary>
        <Table className="mt-3 table-fixed text-xs">
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead>项目目录</TableHead>
              <TableHead className="w-28 text-right">已知 Token</TableHead>
              <TableHead className="w-20 text-right">占比</TableHead>
              <TableHead className="w-16 text-right">请求</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.projects.map((entry) => {
              const total = knownRequestTokens(entry.metrics);
              const label = entry.key === null ? "未记录项目" : shortenPath(entry.key);
              return (
                <TableRow key={JSON.stringify(entry.key)}>
                  <TableCell>
                    <span className="block truncate font-mono" title={entry.key ?? label}>
                      {label}
                    </span>
                    {entry.metrics.totals.requestsWithoutUsage > 0 ? (
                      <span className="text-muted-foreground">
                        {formatNumber(entry.metrics.totals.requestsWithoutUsage)} 条用量未知
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
                    {formatBasisPoints(entry.metrics.shareBps)}
                  </TableCell>
                  <TableCell className="text-right font-mono tabular-nums">
                    {formatNumber(entry.metrics.totals.requests)}
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </details>
    </div>
  );
}
