import { useTokenFormat } from "@/hooks/use-token-format";
/** 会话明细表：只展示元数据与用量，不含任何正文。 */

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { ClientIdentity } from "@/components/ClientIdentity";
import { ModelIdentity } from "@/components/ModelIdentity";
import type { SessionSummary } from "@/lib/usage-types";
import { formatDateTime, formatNumber, shortenPath } from "@/lib/format";
import { cn } from "@/lib/utils";

function CoverageBadge({ session }: { session: SessionSummary }) {
  if (session.totalTokens === null) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge variant="outline" className="font-normal text-muted-foreground">
            无用量数据
          </Badge>
        </TooltipTrigger>
        <TooltipContent className="max-w-64 text-xs leading-relaxed">
          尚未导入可用的会话总量，显示为“—”（未知）。可在数据源管理检查实际路径与采集状态；未知总量不参与合计。
        </TooltipContent>
      </Tooltip>
    );
  }
  if (session.coverage === "partial") {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge variant="secondary" className="font-normal">
            明细不完整
          </Badge>
        </TooltipTrigger>
        <TooltipContent className="max-w-64 text-xs leading-relaxed">
          目前无法确认请求明细完整，例如日志续接、缺少原生序号或字段。已知的会话总量仍按客户端口径展示。
        </TooltipContent>
      </Tooltip>
    );
  }
  return null;
}

export function SessionTable({
  sessions,
  loading,
  error,
  onRetry,
  onOpenAll,
  onOpenSession,
  ariaLabel = "会话明细",
}: {
  sessions: SessionSummary[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  onOpenAll?: () => void;
  onOpenSession?: (session: SessionSummary) => void;
  ariaLabel?: string;
}) {
  const { formatTokensOrDash } = useTokenFormat();
  const rows = sessions ?? [];
  return (
    <div className="space-y-3">
      {error ? (
        <ErrorBlock message={error} onRetry={onRetry} />
      ) : loading ? (
        <LoadingRows rows={6} />
      ) : rows.length === 0 ? (
        <EmptyBlock
          title="没有匹配的会话"
          description="调整时间范围或筛选条件，或在数据源管理里执行一次同步。"
        />
      ) : (
        <div className="min-w-0">
          <Table
            aria-label={ariaLabel}
            className={cn("table-fixed text-xs", onOpenSession ? "min-w-6xl" : "min-w-[1056px]")}
          >
            <colgroup>
              <col className="w-40" />
              <col className="w-40" />
              <col className="w-48" />
              <col />
              <col className="w-20" />
              <col className="w-28" />
              <col className="w-32" />
              {onOpenSession ? <col className="w-24" /> : null}
            </colgroup>
            <TableHeader className="[&_th]:font-medium [&_th]:text-muted-foreground">
              <TableRow>
                <TableHead className="pl-4">开始时间</TableHead>
                <TableHead>客户端</TableHead>
                <TableHead className="min-w-40">模型</TableHead>
                <TableHead>项目</TableHead>
                <TableHead className="text-right tabular-nums">明细</TableHead>
                <TableHead className="text-right tabular-nums">Token</TableHead>
                <TableHead>状态</TableHead>
                {onOpenSession ? <TableHead className="pr-4 text-right">操作</TableHead> : null}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((session) => (
                <TableRow
                  key={`${session.client}:${session.clientSessionId}`}
                  className="border-border/40 transition-colors duration-150 hover:bg-muted/60"
                >
                  <TableCell className="pl-4 text-xs text-muted-foreground tabular-nums">
                    {formatDateTime(session.startedAtMs)}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    <ClientIdentity client={session.client} compact />
                  </TableCell>
                  <TableCell className="min-w-40 whitespace-nowrap">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <span
                          tabIndex={0}
                          className="block max-w-full rounded-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                        >
                          <ModelIdentity
                            model={session.model}
                            compact
                            truncate
                            className="flex w-full"
                            labelClassName="font-mono text-xs"
                          />
                        </span>
                      </TooltipTrigger>
                      <TooltipContent className="max-w-80 font-mono text-xs break-all">
                        {session.model?.trim() ? session.model : "模型未记录"}
                      </TooltipContent>
                    </Tooltip>
                  </TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">
                    <span className="block truncate" title={session.projectPath ?? undefined}>
                      {shortenPath(session.projectPath)}
                    </span>
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs tabular-nums">
                    {formatNumber(session.messages)}
                  </TableCell>
                  <TableCell
                    className="text-right font-mono text-xs tabular-nums"
                    title={
                      session.totalTokens === null
                        ? "用量未知"
                        : `${formatNumber(session.totalTokens)} Token`
                    }
                  >
                    {formatTokensOrDash(session.totalTokens)}
                  </TableCell>
                  <TableCell>
                    <CoverageBadge session={session} />
                  </TableCell>
                  {onOpenSession ? (
                    <TableCell className="pr-4 text-right">
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-8 px-2 text-xs"
                        aria-label={`查看会话 ${session.clientSessionId} 的详情`}
                        onClick={() => onOpenSession(session)}
                      >
                        查看详情
                      </Button>
                    </TableCell>
                  ) : null}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
      {onOpenAll && rows.length > 0 ? (
        <div className="flex justify-end">
          <Button size="sm" variant="ghost" onClick={onOpenAll}>
            查看全部会话
          </Button>
        </div>
      ) : null}
    </div>
  );
}
