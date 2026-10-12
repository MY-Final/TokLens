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
}: {
  sessions: SessionSummary[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  onOpenAll?: () => void;
  onOpenSession?: (session: SessionSummary) => void;
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
          title="所选区间没有会话"
          description="调整时间范围，或在数据源管理里执行一次同步。"
        />
      ) : (
        <div className="overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-40">开始时间</TableHead>
                <TableHead className="w-24">客户端</TableHead>
                <TableHead>模型</TableHead>
                <TableHead>项目</TableHead>
                <TableHead className="w-20 text-right">明细</TableHead>
                <TableHead className="w-24 text-right">Token</TableHead>
                <TableHead className="w-32">状态</TableHead>
                {onOpenSession ? <TableHead className="w-24">操作</TableHead> : null}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((session) => (
                <TableRow
                  key={`${session.client}:${session.clientSessionId}`}
                  className="border-border/40 transition-colors hover:bg-muted/50"
                >
                  <TableCell className="text-xs text-muted-foreground tabular-nums">
                    {formatDateTime(session.startedAtMs)}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    <ClientIdentity client={session.client} compact />
                  </TableCell>
                  <TableCell className="text-xs">
                    <ModelIdentity
                      model={session.model}
                      compact
                      labelClassName="font-mono text-xs"
                    />
                  </TableCell>
                  <TableCell
                    className="max-w-48 truncate font-mono text-xs text-muted-foreground"
                    title={session.projectPath ?? undefined}
                  >
                    {shortenPath(session.projectPath)}
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
                    <TableCell>
                      <Button
                        size="sm"
                        variant="ghost"
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
