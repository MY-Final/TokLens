/** 同步状态与控制：侧边栏与数据源页共用。 */

import { MoreHorizontal, RefreshCw } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { formatDateTime, formatNumber, syncStatusLabel } from "@/lib/format";
import type { SyncState } from "@/lib/usage-types";
import { cn } from "@/lib/utils";

export function SyncControl({
  sync,
  busy,
  onSync,
  size = "sm",
  compact = false,
  paused = false,
  onOpenSettings,
}: {
  sync: SyncState | null | undefined;
  busy: boolean;
  onSync: (force?: boolean) => void;
  size?: "sm" | "default";
  compact?: boolean;
  paused?: boolean;
  onOpenSettings?: () => void;
}) {
  const running = busy || (sync?.running ?? false);
  const label = syncStatusLabel(sync);
  if (compact)
    return (
      <div className="flex items-center gap-1">
        <Popover>
          <PopoverTrigger asChild>
            <Button
              size="sm"
              variant="ghost"
              className={cn("text-xs", sync?.error ? "text-destructive" : "text-muted-foreground")}
              aria-label="查看采集状态与详情"
            >
              <span
                className={cn(
                  "size-1.5 rounded-full",
                  sync?.error ? "bg-destructive" : "bg-primary",
                )}
                aria-hidden="true"
              />
              {running
                ? paused
                  ? "同步中 · 待暂停"
                  : "同步中"
                : paused
                  ? sync?.error
                    ? "已暂停 · 同步异常"
                    : "采集已暂停"
                  : sync?.error
                    ? "同步异常"
                    : sync?.finishedAtMs
                      ? "已同步"
                      : "尚未同步"}
            </Button>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-80 space-y-3 text-xs">
            <p className="font-medium">采集状态</p>
            <p role="status">{label}</p>
            {paused && <p>采集已暂停，恢复后继续读取本机数据。</p>}
            <p className="text-muted-foreground">
              上次完成：{formatDateTime(sync?.finishedAtMs ?? null)}
            </p>
            {sync?.report && (
              <p className="leading-relaxed text-muted-foreground">
                扫描 {formatNumber(sync.report.filesSeen)} 个文件，解析{" "}
                {formatNumber(sync.report.filesParsed)} 个，跳过{" "}
                {formatNumber(sync.report.filesSkippedUnchanged)} 个未变化文件，导入{" "}
                {formatNumber(sync.report.sessionsImported)} 个会话。失败文件{" "}
                {formatNumber(sync.report.filesFailed)} 个。
              </p>
            )}
            {sync?.error && <p className="break-words text-destructive">{sync.error}</p>}
            {paused && onOpenSettings && (
              <Button size="sm" variant="outline" onClick={onOpenSettings}>
                打开设置
              </Button>
            )}
          </PopoverContent>
        </Popover>
        <Button
          size="icon-sm"
          variant="ghost"
          disabled={running || paused}
          onClick={() => onSync(false)}
          aria-label="刷新用量"
        >
          <RefreshCw className={running ? "size-4 animate-spin" : "size-4"} aria-hidden="true" />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="icon-sm" variant="ghost" aria-label="更多采集操作">
              <MoreHorizontal className="size-4" aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem disabled={running || paused} onSelect={() => onSync(true)}>
              重新扫描全部数据
            </DropdownMenuItem>
            {onOpenSettings && (
              <DropdownMenuItem onSelect={onOpenSettings}>打开设置</DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    );
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
            ? paused
              ? "同步中 · 待暂停"
              : "同步中"
            : paused
              ? sync?.error
                ? "已暂停 · 同步异常"
                : "采集已暂停"
              : sync?.error
                ? "同步异常"
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
        <Button
          size={size}
          variant="ghost"
          disabled={running || paused}
          onClick={() => onSync(false)}
        >
          <RefreshCw
            className={running ? "size-3.5 animate-spin" : "size-3.5"}
            aria-hidden="true"
          />
          {running ? "同步中…" : "刷新"}
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size={size} variant="ghost" aria-label="更多采集操作">
              <MoreHorizontal className="size-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem disabled={running || paused} onSelect={() => onSync(true)}>
              重新扫描全部数据
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {!compact && sync?.finishedAtMs ? (
        <p className="text-xs text-muted-foreground">
          完成时间：{formatDateTime(sync.finishedAtMs)}
        </p>
      ) : null}
    </div>
  );
}
