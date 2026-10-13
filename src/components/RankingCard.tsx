import { useTokenFormat } from "@/hooks/use-token-format";
/** 条形仅表示相对榜首大小；排行与数值均来自 Rust 查询。 */

import { Folder } from "lucide-react";

import { DashboardCard } from "@/components/DashboardCard";
import { ModelIdentity } from "@/components/ModelIdentity";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import type { RankingEntry } from "@/lib/usage-types";
import { formatNumber, shortenPath } from "@/lib/format";

export function RankingCard({
  title,
  description,
  entries,
  loading,
  error,
  onRetry,
  emptyTitle,
  emptyDescription,
  shortenKeys = false,
  onOpenEntry,
}: {
  title: string;
  description: string;
  entries: RankingEntry[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  emptyTitle: string;
  emptyDescription: string;
  shortenKeys?: boolean;
  onOpenEntry?: (key: string | null) => void;
}) {
  const { formatTokens } = useTokenFormat();
  const rows = entries ?? [];
  const max = rows[0]?.totalTokens ?? 0;
  return (
    <DashboardCard className="h-full">
      <CardHeader className="gap-1.5 px-5">
        <CardTitle className="text-sm">{title}</CardTitle>
        <CardDescription className="text-xs">{description}</CardDescription>
      </CardHeader>
      <CardContent className="px-5">
        {error ? (
          <ErrorBlock message={error} onRetry={onRetry} />
        ) : loading ? (
          <LoadingRows rows={5} />
        ) : rows.length === 0 ? (
          <EmptyBlock title={emptyTitle} description={emptyDescription} />
        ) : (
          <Table className="table-fixed text-xs">
            <TableHeader>
              <TableRow className="border-border/50 hover:bg-transparent">
                <TableHead className="h-8 w-7 pl-0 text-xs">#</TableHead>
                <TableHead className="h-8 text-xs">
                  {shortenKeys ? "项目路径" : "模型名称"}
                </TableHead>
                <TableHead className="hidden h-8 w-1/3 text-xs xl:table-cell">相对用量</TableHead>
                <TableHead className="h-8 w-14 text-right text-xs">会话</TableHead>
                <TableHead className="h-8 w-20 pr-0 text-right text-xs">Token</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((entry, index) => {
                const label =
                  entry.key === null ? "未知" : shortenKeys ? shortenPath(entry.key) : entry.key;
                const percent = max > 0 ? (entry.totalTokens / max) * 100 : 0;
                const unknown = entry.sessionsWithoutUsage === entry.sessions;
                return (
                  <TableRow
                    key={`${entry.key ?? "unknown"}-${index}`}
                    className="border-border/40 transition-colors hover:bg-muted/50"
                  >
                    <TableCell className="py-3 pl-0 font-mono text-muted-foreground">
                      {index + 1}
                    </TableCell>
                    <TableCell className="py-3">
                      {shortenKeys ? (
                        <div className="flex min-w-0 items-center gap-2">
                          <Folder
                            className="size-3.5 shrink-0 text-muted-foreground"
                            aria-hidden="true"
                          />
                          <span
                            className="truncate font-mono text-xs"
                            title={entry.key ?? "客户端未记录"}
                          >
                            {label}
                          </span>
                        </div>
                      ) : (
                        <ModelIdentity
                          model={entry.key}
                          compact
                          truncate
                          labelClassName="font-mono text-xs font-normal"
                        />
                      )}
                      {!unknown ? (
                        <Progress
                          value={percent}
                          aria-valuenow={percent}
                          aria-valuemin={0}
                          aria-valuemax={100}
                          aria-label={`${label}，相对榜首用量`}
                          className="mt-2 h-1 bg-muted xl:hidden"
                        />
                      ) : null}
                    </TableCell>
                    <TableCell className="hidden xl:table-cell">
                      {!unknown ? (
                        <Progress
                          value={percent}
                          aria-valuenow={percent}
                          aria-valuemin={0}
                          aria-valuemax={100}
                          aria-label={`${label}，相对榜首用量`}
                          className="h-1.5 bg-muted"
                        />
                      ) : (
                        <span className="text-muted-foreground">总量未知</span>
                      )}
                    </TableCell>
                    <TableCell className="text-right font-mono text-xs text-muted-foreground">
                      {onOpenEntry ? (
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-auto p-0 text-xs"
                          aria-label={`查看 ${label} 会话`}
                          onClick={() => onOpenEntry(entry.key)}
                        >
                          {formatNumber(entry.sessions)}
                        </Button>
                      ) : (
                        formatNumber(entry.sessions)
                      )}
                    </TableCell>
                    <TableCell
                      className="pr-0 text-right font-mono text-xs"
                      title={unknown ? "总量未知" : `${formatNumber(entry.totalTokens)} Token`}
                    >
                      {unknown ? "—" : formatTokens(entry.totalTokens)}
                      {entry.sessionsWithoutUsage > 0 ? (
                        <div className="text-muted-foreground">{unknown ? "未知" : "已知子集"}</div>
                      ) : null}
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </DashboardCard>
  );
}
