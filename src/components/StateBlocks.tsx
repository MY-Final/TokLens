/** 加载 / 空 / 错误 / 等待采集 的统一状态块（全部使用语义 token）。 */

import type { ComponentProps, ReactNode } from "react";
import { AlertTriangle, Inbox, Loader2 } from "lucide-react";

import { DashboardCard } from "@/components/DashboardCard";
import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { overviewKpiGridClass } from "@/lib/overview-layout";
import { cn } from "@/lib/utils";

/** 装饰性占位块：仅用 `bg-muted` 语义色，并对读屏隐藏。 */
function SkeletonBlock({ className, ...props }: ComponentProps<typeof Skeleton>) {
  return <Skeleton aria-hidden="true" className={cn("bg-muted", className)} {...props} />;
}

function SkeletonRegion({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={className} aria-busy="true" aria-live="polite" role="status">
      <span className="sr-only">{label}</span>
      {children}
    </div>
  );
}

export function LoadingRows({ rows = 3 }: { rows?: number }) {
  return (
    <SkeletonRegion label="正在加载…" className="space-y-2">
      {Array.from({ length: rows }, (_, index) => (
        <SkeletonBlock key={index} className="h-9 w-full" />
      ))}
    </SkeletonRegion>
  );
}

export function LoadingInline({ label = "正在加载…" }: { label?: string }) {
  return (
    <div
      className="flex items-center gap-2 text-sm text-muted-foreground"
      aria-busy="true"
      aria-live="polite"
    >
      <Loader2 className="size-4 animate-spin" aria-hidden="true" />
      {label}
    </div>
  );
}

/** 图表区占位：与真实图表同高（避免加载完成后的跳变），底部保留轴刻度。 */
export function ChartSkeleton({
  blockClassName = "h-56 xl:h-64",
  className,
}: {
  blockClassName?: string;
  className?: string;
}) {
  return (
    <SkeletonRegion label="正在加载图表…" className={cn("space-y-3", className)}>
      <SkeletonBlock className={cn("w-full rounded-xl", blockClassName)} />
      <div className="flex items-center justify-between gap-2" aria-hidden="true">
        {Array.from({ length: 7 }, (_, index) => (
          <SkeletonBlock key={index} className="h-3 w-8 rounded-sm" />
        ))}
      </div>
    </SkeletonRegion>
  );
}

/** 活动日历热力图占位：周列 × 7 日网格，与真实格子同尺度。 */
export function HeatmapSkeleton() {
  return (
    <SkeletonRegion label="正在加载活动日历…" className="space-y-4">
      <div className="flex gap-1 overflow-hidden" aria-hidden="true">
        <div className="grid shrink-0 grid-rows-[1.5rem_repeat(7,1rem)] gap-1">
          <span />
          {Array.from({ length: 7 }, (_, index) => (
            <SkeletonBlock key={index} className="size-4 rounded-sm" />
          ))}
        </div>
        {Array.from({ length: 16 }, (_, week) => (
          <div key={week} className="grid grid-rows-[1.5rem_repeat(7,1rem)] gap-1">
            <SkeletonBlock className="h-4 w-6 rounded-sm" />
            {Array.from({ length: 7 }, (_, day) => (
              <SkeletonBlock key={day} className="size-4 rounded-sm" />
            ))}
          </div>
        ))}
      </div>
      <SkeletonBlock className="h-3 w-40 rounded-sm" />
    </SkeletonRegion>
  );
}

/** 客户端排行占位：名称块 + Token 数值 + 占比条，与真实排行同尺度。 */
export function ClientListSkeleton({ rows = 4 }: { rows?: number }) {
  return (
    <SkeletonRegion label="正在加载客户端用量…" className="space-y-3">
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="space-y-1.5" aria-hidden="true">
          <div className="grid grid-cols-[minmax(0,1fr)_6rem_3.5rem] items-center gap-x-4">
            <div className="flex items-center gap-2.5">
              <SkeletonBlock className="size-6 rounded-lg" />
              <SkeletonBlock className="h-4 w-24 rounded-sm" />
            </div>
            <SkeletonBlock className="h-4 w-16 justify-self-end rounded-sm" />
            <SkeletonBlock className="h-3 w-10 justify-self-end rounded-sm" />
          </div>
          <SkeletonBlock className="h-1.5 w-full rounded-full" />
        </div>
      ))}
    </SkeletonRegion>
  );
}

/** 排名占位：序号 + 图标 + 名称条 + 相对用量条 + 数值。 */
export function RankingSkeleton({ rows = 5 }: { rows?: number }) {
  return (
    <SkeletonRegion label="正在加载排行…" className="space-y-3">
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="flex items-center gap-3 py-1" aria-hidden="true">
          <SkeletonBlock className="h-3 w-4 rounded-sm" />
          <SkeletonBlock className="size-6 shrink-0 rounded-md" />
          <div className="min-w-0 flex-1 space-y-2">
            <SkeletonBlock className="h-3 w-32 rounded-sm" />
            <SkeletonBlock className="h-1.5 w-2/3 rounded-full" />
          </div>
          <SkeletonBlock className="h-3 w-12 rounded-sm" />
        </div>
      ))}
    </SkeletonRegion>
  );
}

/** 表格占位：表头 + 近似分列的数据行。 */
export function TableSkeleton({ rows = 6, columns = 7 }: { rows?: number; columns?: number }) {
  return (
    <SkeletonRegion label="正在加载表格…" className="space-y-3">
      <div className="flex items-center gap-4 border-b border-border/50 pb-2" aria-hidden="true">
        {Array.from({ length: columns }, (_, index) => (
          <SkeletonBlock key={index} className="h-3 flex-1 rounded-sm" />
        ))}
      </div>
      {Array.from({ length: rows }, (_, row) => (
        <div key={row} className="flex items-center gap-4" aria-hidden="true">
          {Array.from({ length: columns }, (_, column) => (
            <SkeletonBlock key={column} className="h-4 flex-1 rounded-sm" />
          ))}
        </div>
      ))}
    </SkeletonRegion>
  );
}

/** 统计指标卡片行占位：复用 DashboardCard 外观，避免加载完成后的尺寸跳变。 */
export function StatCardsSkeleton({ cards = 4 }: { cards?: number }) {
  return (
    <SkeletonRegion
      label="正在加载统计指标…"
      className={cn("grid gap-3", overviewKpiGridClass(cards))}
    >
      {Array.from({ length: cards }, (_, index) => (
        <DashboardCard key={index} className="gap-0 py-0" aria-hidden="true">
          <CardContent className="space-y-2 px-4 py-3.5">
            <SkeletonBlock className="h-3 w-20 rounded-sm" />
            <SkeletonBlock className="h-6 w-24 rounded-sm" />
            <SkeletonBlock className="h-3 w-28 rounded-sm" />
          </CardContent>
        </DashboardCard>
      ))}
    </SkeletonRegion>
  );
}

/** 详情面板占位：字段网格 + 文本块。 */
export function DetailSkeleton({ rows = 6 }: { rows?: number }) {
  return (
    <SkeletonRegion label="正在加载详情…" className="space-y-4">
      <div className="grid grid-cols-2 gap-x-6 gap-y-3" aria-hidden="true">
        {Array.from({ length: rows }, (_, index) => (
          <div key={index} className="space-y-2">
            <SkeletonBlock className="h-3 w-16 rounded-sm" />
            <SkeletonBlock className="h-4 w-32 rounded-sm" />
          </div>
        ))}
      </div>
      <SkeletonBlock className="h-4 w-full rounded-sm" />
      <SkeletonBlock className="h-4 w-3/4 rounded-sm" />
    </SkeletonRegion>
  );
}

export function EmptyBlock({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 py-8 text-center">
      <Inbox className="size-5 text-muted-foreground" aria-hidden="true" />
      <p className="text-sm font-medium">{title}</p>
      {description ? (
        <p className="max-w-md text-xs leading-relaxed text-muted-foreground">{description}</p>
      ) : null}
      {action}
    </div>
  );
}

export function ErrorBlock({
  message,
  onRetry,
  retryLabel = "重试",
}: {
  message: string;
  onRetry?: () => void;
  retryLabel?: string;
}) {
  return (
    <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/5 p-3">
      <AlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden="true" />
      <div className="space-y-2">
        <p className="text-sm text-destructive">{message}</p>
        {onRetry ? (
          <Button size="sm" variant="outline" onClick={onRetry}>
            {retryLabel}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
