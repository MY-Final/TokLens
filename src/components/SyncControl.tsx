/** 同步状态与控制：侧边栏与数据源页共用。 */

import { RefreshCw } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatDateTime, formatNumber, syncStatusLabel } from "@/lib/format";
import type { SyncState } from "@/lib/usage-types";
import { cn } from "@/lib/utils";

export function SyncControl({
  sync,
  busy,
  onSync,
  size = "sm",
  compact = false,
}: {
  sync: SyncState | null | undefined;
  busy: boolean;
  onSync: (force?: boolean) => void;
  size?: "sm" | "default";
  compact?: boolean;
}) {
  const running = busy || (sync?.running ?? false);
  const label = syncStatusLabel(sync);
  return (
    <div className={compact ? "flex flex-wrap items-center gap-2" : "space-y-3"}>
      <div className="flex items-center gap-2" role="status" aria-live="polite">
        <Badge
          variant="outline"
          className={cn(
            "gap-2 rounded-full border-border/70 px-2.5 py-1 font-normal",
            sync?.error ? "text-destructive" : "text-muted-foreground",
          )}
          title={label}
        >
          <span
            className={cn("size-1.5 rounded-full", sync?.error ? "bg-destructive" : "bg-primary")}
            aria-hidden="true"
          />
          {running
            ? "同步中"
            : sync?.error
              ? "同步失败"
              : sync?.finishedAtMs
                ? "已同步"
                : "尚未同步"}
        </Badge>
        {!compact ? (
          <span className="truncate text-xs text-muted-foreground" title={label}>
            {label}
          </span>
        ) : null}
      </div>
      {!compact && sync?.report ? (
        <p className="text-xs text-muted-foreground tabular-nums">
          解析 {formatNumber(sync.report.filesParsed)} 个文件、跳过{" "}
          {formatNumber(sync.report.filesSkippedUnchanged)} 个未变化文件，导入{" "}
          {formatNumber(sync.report.sessionsImported)} 个会话
        </p>
      ) : null}
      {!compact && sync?.error ? <p className="text-xs text-destructive">{sync.error}</p> : null}
      <div className="flex items-center gap-2">
        <Button size={size} variant="ghost" disabled={running} onClick={() => onSync(false)}>
          <RefreshCw
            className={running ? "size-3.5 animate-spin" : "size-3.5"}
            aria-hidden="true"
          />
          {running ? "同步中…" : "刷新"}
        </Button>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button size={size} variant="ghost" disabled={running} onClick={() => onSync(true)}>
              重新扫描
            </Button>
          </TooltipTrigger>
          <TooltipContent className="max-w-64 text-xs leading-relaxed">
            忽略增量水位，重新解析全部 Codex 会话文件（用于修复或文件被替换后重建）。
          </TooltipContent>
        </Tooltip>
      </div>
      {!compact && sync?.finishedAtMs ? (
        <p className="text-xs text-muted-foreground">
          完成时间：{formatDateTime(sync.finishedAtMs)}
        </p>
      ) : null}
    </div>
  );
}
