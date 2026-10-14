/** 会话详情仅呈现本地统计元数据，不读取或展示正文。 */
import { ClientIdentity } from "@/components/ClientIdentity";
import { ModelIdentity } from "@/components/ModelIdentity";
import { EmptyBlock, ErrorBlock, DetailSkeleton } from "@/components/StateBlocks";
import { useSyncing } from "@/hooks/use-sync-status";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useTokenFormat } from "@/hooks/use-token-format";
import { DataExplanation } from "@/components/DataExplanation";
import type { AsyncData } from "@/hooks/use-async-data";
import {
  formatDateTime,
  formatNumber,
  formatTokensOrDash as formatExactOrDash,
} from "@/lib/format";
import type { RequestDetail, SessionDetail, SessionSummary } from "@/lib/usage-types";
import { GROK_USAGE_NOTE } from "@/lib/client-labels";

const TOTAL_BASIS_LABELS: Record<string, string> = {
  client_cumulative: "客户端累计值",
  client_reported: "客户端自报值",
  sum_of_messages: "请求明细求和",
};
const COVERAGE_LABELS: Record<string, string> = {
  complete: "明细完整",
  partial: "明细不完整",
  none: "无请求明细",
};
const SOURCE_LABELS: Record<string, string> = {
  measured: "实测",
  derived: "换算",
  estimated: "估算",
};
const TOKEN_FIELDS = [
  ["inputTokens", "输入"],
  ["outputTokens", "输出"],
  ["reasoningTokens", "推理"],
  ["cacheReadTokens", "缓存读取"],
  ["cacheWriteTokens", "缓存写入"],
  ["totalTokens", "总 Token"],
] as const;

function RequestRow({ request }: { request: RequestDetail }) {
  const { formatTokensOrDash } = useTokenFormat();
  const missing = TOKEN_FIELDS.filter(([field]) => request[field] === null).map(
    ([, label]) => label,
  );
  return (
    <TableRow>
      <TableCell className="tabular-nums" title={formatExactOrDash(request.seq)}>
        <span className="block truncate">{formatExactOrDash(request.seq)}</span>
      </TableCell>
      <TableCell className="whitespace-nowrap tabular-nums">
        <div>{formatDateTime(request.createdAtMs)}</div>
        <div className="text-muted-foreground">结束 {formatDateTime(request.completedAtMs)}</div>
      </TableCell>
      <TableCell className="font-mono break-all whitespace-normal">
        <ModelIdentity model={request.model} compact unknownLabel="—" labelClassName="text-xs" />
        <div className="text-muted-foreground">{request.agent ?? "—"}</div>
      </TableCell>
      {TOKEN_FIELDS.map(([field]) => (
        <TableCell
          key={field}
          className="text-right font-mono tabular-nums"
          title={`${formatExactOrDash(request[field])} Token`}
        >
          <span className="block truncate">{formatTokensOrDash(request[field])}</span>
        </TableCell>
      ))}
      <TableCell className="space-y-1 whitespace-normal">
        <Badge variant={request.sourceKind === "estimated" ? "secondary" : "outline"}>
          {SOURCE_LABELS[request.sourceKind] ?? request.sourceKind}
        </Badge>
        <div className="font-mono break-all text-muted-foreground">
          {request.sourceRef ?? "来源引用未记录"}
        </div>
      </TableCell>
      <TableCell className="space-y-1 break-all whitespace-normal text-muted-foreground">
        <div>{request.isSidechain ? "子代理 / 旁链" : "主链"}</div>
        {request.createdAtMs === null ? <div>缺失请求时间</div> : null}
        {missing.length > 0 ? <div>缺失：{missing.join("、")}</div> : null}
        {request.unknownRelations.length > 0 ? (
          <div className="break-all">未知关系：{request.unknownRelations.join("、")}</div>
        ) : null}
      </TableCell>
    </TableRow>
  );
}

export function SessionDetailPanel({
  selectedSession,
  detail,
  onClose,
  offset,
  onOffsetChange,
}: {
  selectedSession: SessionSummary;
  detail: AsyncData<SessionDetail>;
  onClose: () => void;
  offset: number;
  onOffsetChange: (offset: number) => void;
}) {
  const { formatTokensOrDash } = useTokenFormat();
  const syncing = useSyncing();
  const session = detail.data?.session ?? selectedSession;
  const coverage = detail.data?.requestCoverage;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        className="flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden sm:max-w-5xl"
        showCloseButton={false}
      >
        <DialogHeader className="shrink-0">
          <div className="flex items-center justify-between gap-3">
            <DialogTitle>会话详情</DialogTitle>
            <Button variant="ghost" size="sm" onClick={onClose}>
              关闭
            </Button>
          </div>
          <DialogDescription>完整会话（不受日期筛选限制） · 本地时间</DialogDescription>
          {session.client === "grok" ? (
            <p className="text-xs text-muted-foreground">{GROK_USAGE_NOTE}</p>
          ) : null}
        </DialogHeader>
        <div className="min-h-0 space-y-5 overflow-y-auto pr-1">
          <div className="flex flex-wrap items-center gap-2">
            <ClientIdentity client={session.client} />
            <Badge variant="outline">{COVERAGE_LABELS[session.coverage] ?? session.coverage}</Badge>
            <Badge variant="outline">
              {TOTAL_BASIS_LABELS[session.totalBasis] ?? session.totalBasis}
            </Badge>
          </div>
          <dl className="grid grid-cols-2 gap-x-6 gap-y-3 text-xs">
            {[
              ["会话 ID", session.clientSessionId],
              ["模型", session.model ?? "—"],
              ["项目", session.projectPath ?? "—"],
              [
                "开始 / 结束",
                `${formatDateTime(session.startedAtMs)} / ${formatDateTime(session.endedAtMs)}`,
              ],
              ...TOKEN_FIELDS.map(([field, label]) => [
                label,
                formatTokensOrDash(session[field]),
                `${formatExactOrDash(session[field])} Token`,
              ]),
            ].map(([label, value, exact]) => (
              <div key={label} className="min-w-0 space-y-1">
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="font-mono break-all tabular-nums" title={exact}>
                  {label === "模型" ? (
                    <ModelIdentity
                      model={session.model}
                      compact
                      unknownLabel="—"
                      labelClassName="text-xs"
                    />
                  ) : (
                    value
                  )}
                </dd>
              </div>
            ))}
          </dl>
          <DataExplanation summary="— 为未知">
            <p>
              输入、输出已扣除重叠子集。缓存写入仅在 Claude Code、DSH、Kimi Code、MiMoCode、Pi
              Desktop 中计入总量，其他客户端不重复相加。— 为未知，非零；会话总量独立于明细覆盖。
            </p>
            <p>实测来自客户端记录，换算来自真实字段，估算单独标记。</p>
          </DataExplanation>
          {session.unknownRelations.length > 0 ? (
            <p className="rounded-md border p-3 text-xs break-all">
              未知字段关系：{session.unknownRelations.join("、")}
            </p>
          ) : null}
          <section className="space-y-3" aria-label="请求元数据">
            <h3 className="text-sm font-medium">请求元数据</h3>
            {detail.error ? (
              <ErrorBlock message={`无法加载会话详情：${detail.error}`} onRetry={detail.reload} />
            ) : !detail.data && (detail.loading || syncing) ? (
              <DetailSkeleton />
            ) : detail.data ? (
              <>
                {coverage ? (
                  <p className="text-xs leading-relaxed text-muted-foreground tabular-nums">
                    全会话共 {formatNumber(coverage.total)} 条 · 有时间{" "}
                    {formatNumber(coverage.timed)} 条 / 无时间 {formatNumber(coverage.untimed)} 条 ·
                    有用量 {formatNumber(coverage.withUsage)} 条 / 无用量{" "}
                    {formatNumber(coverage.withoutUsage)} 条 · 估算{" "}
                    {formatNumber(coverage.estimated)} 条{coverage.partial ? " · 明细不完整" : ""}
                  </p>
                ) : null}
                {detail.data.requests.length === 0 ? (
                  <EmptyBlock
                    title="没有可用请求明细"
                    description="客户端可能仅记录累计用量，刷新后重试。"
                  />
                ) : (
                  <div className="rounded-md border [&>[data-slot=table-container]]:max-h-[60dvh] [&>[data-slot=table-container]]:overflow-auto">
                    <Table className="min-w-7xl table-fixed text-xs">
                      <TableHeader className="sticky top-0 z-10 bg-background">
                        <TableRow>
                          <TableHead className="w-12">序号</TableHead>
                          <TableHead className="w-40">开始 / 结束</TableHead>
                          <TableHead className="w-40">模型 / 代理</TableHead>
                          {TOKEN_FIELDS.map(([, label]) => (
                            <TableHead key={label} className="w-20 text-right whitespace-nowrap">
                              {label}
                            </TableHead>
                          ))}
                          <TableHead className="w-56">来源等级 / 引用</TableHead>
                          <TableHead className="w-52">状态</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {detail.data.requests.map((request) => (
                          <RequestRow key={request.dedupKey} request={request} />
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                )}
                <div className="flex items-center justify-between gap-3">
                  <p className="text-xs text-muted-foreground tabular-nums">
                    第 {formatNumber(offset / 100 + 1)} 页 · 每页最多 100 条
                  </p>
                  <div className="flex gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={offset === 0}
                      onClick={() => onOffsetChange(Math.max(0, offset - 100))}
                    >
                      上一页
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={!coverage || offset + 100 >= coverage.total}
                      onClick={() => onOffsetChange(offset + 100)}
                    >
                      下一页
                    </Button>
                  </div>
                </div>
              </>
            ) : null}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}
