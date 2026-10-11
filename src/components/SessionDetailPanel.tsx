/** 会话详情仅呈现本地统计元数据，不读取或展示正文。 */
import { ClientIdentity } from "@/components/ClientIdentity";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
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
import type { AsyncData } from "@/hooks/use-async-data";
import { formatDateTime, formatNumber, formatTokensOrDash } from "@/lib/format";
import type { RequestDetail, SessionDetail, SessionSummary } from "@/lib/usage-types";

const TOTAL_BASIS_LABELS = {
  client_cumulative: "客户端累计值",
  client_reported: "客户端自报值",
  sum_of_messages: "请求明细求和",
};
const COVERAGE_LABELS = { complete: "明细完整", partial: "明细不完整", none: "无请求明细" };
const SOURCE_LABELS = { measured: "实测", derived: "换算", estimated: "估算" };
const TOKEN_FIELDS = [
  ["inputTokens", "输入"],
  ["outputTokens", "输出"],
  ["reasoningTokens", "推理"],
  ["cacheReadTokens", "缓存读取"],
  ["cacheWriteTokens", "缓存写入"],
  ["totalTokens", "总 Token"],
] as const;

function RequestRow({ request }: { request: RequestDetail }) {
  const missing = TOKEN_FIELDS.filter(([field]) => request[field] === null).map(
    ([, label]) => label,
  );
  return (
    <TableRow>
      <TableCell className="tabular-nums">{formatTokensOrDash(request.seq)}</TableCell>
      <TableCell className="whitespace-nowrap tabular-nums">
        <div>{formatDateTime(request.createdAtMs)}</div>
        <div className="text-muted-foreground">结束 {formatDateTime(request.completedAtMs)}</div>
      </TableCell>
      <TableCell className="max-w-48 font-mono break-all">
        <div>{request.model ?? "—"}</div>
        <div className="text-muted-foreground">{request.agent ?? "—"}</div>
      </TableCell>
      {TOKEN_FIELDS.map(([field]) => (
        <TableCell key={field} className="text-right font-mono tabular-nums">
          {formatTokensOrDash(request[field])}
        </TableCell>
      ))}
      <TableCell className="space-y-1">
        <Badge variant={request.sourceKind === "estimated" ? "secondary" : "outline"}>
          {SOURCE_LABELS[request.sourceKind]}
        </Badge>
        <div className="max-w-64 font-mono break-all text-muted-foreground">
          {request.sourceRef ?? "来源引用未记录"}
        </div>
      </TableCell>
      <TableCell className="min-w-40 space-y-1 text-muted-foreground">
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
          <DialogDescription>
            展示整个会话，不受页面日期范围限制。仅展示本地统计元数据；不展示提示词、会话正文或工具输出。时间为本地时区。
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 space-y-5 overflow-y-auto pr-1">
          <div className="flex flex-wrap items-center gap-2">
            <ClientIdentity client={session.client} />
            <Badge variant="outline">{COVERAGE_LABELS[session.coverage]}</Badge>
            <Badge variant="outline">{TOTAL_BASIS_LABELS[session.totalBasis]}</Badge>
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
              ...TOKEN_FIELDS.map(([field, label]) => [label, formatTokensOrDash(session[field])]),
            ].map(([label, value]) => (
              <div key={label} className="min-w-0 space-y-1">
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="font-mono break-all tabular-nums">{value}</dd>
              </div>
            ))}
          </dl>
          <p className="text-xs leading-relaxed text-muted-foreground">
            输入、输出、推理与缓存读取已按客户端口径归一化，重叠子集已扣除。 缓存写入仅在 Claude
            Code、DSH、Kimi Code、MiMoCode、Pi Desktop 口径中计入总量；
            其他客户端的缓存写入作为原始字段单独展示，不应再次加到总量。
            “—”表示缺失或未知，不是零；会话总量独立于请求明细覆盖情况。
          </p>
          {session.unknownRelations.length > 0 ? (
            <p className="rounded-md border p-3 text-xs break-all">
              未知字段关系：{session.unknownRelations.join("、")}
            </p>
          ) : null}
          <section className="space-y-3" aria-label="请求元数据">
            <h3 className="text-sm font-medium">请求元数据</h3>
            {detail.error ? (
              <ErrorBlock message={`无法加载会话详情：${detail.error}`} onRetry={detail.reload} />
            ) : detail.loading ? (
              <LoadingRows rows={5} />
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
                    description="客户端可能仅记录会话累计用量；同步新记录后可重试。会话累计值仍独立展示。"
                  />
                ) : (
                  <div className="overflow-x-auto rounded-md border">
                    <Table className="text-xs">
                      <TableHeader>
                        <TableRow>
                          <TableHead>序号</TableHead>
                          <TableHead>开始 / 结束</TableHead>
                          <TableHead>模型 / 代理</TableHead>
                          {TOKEN_FIELDS.map(([, label]) => (
                            <TableHead key={label} className="text-right whitespace-nowrap">
                              {label}
                            </TableHead>
                          ))}
                          <TableHead>来源等级 / 引用</TableHead>
                          <TableHead>状态</TableHead>
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
                <p className="text-xs text-muted-foreground">
                  请求按后端时间顺序展示。实测为客户端记录，换算来自真实字段，估算单独标注。
                </p>
              </>
            ) : null}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}
