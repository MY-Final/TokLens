/** 指标卡片：一个数值 + 说明，不做装饰性排版。 */

import { DashboardCard } from "@/components/DashboardCard";

import { Info, type LucideIcon } from "lucide-react";

import { CardContent } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

export type StatCardProps = {
  label: string;
  value: string;
  unit?: string;
  hint?: string;
  tooltip?: string;
  icon?: LucideIcon;
  detail?: string;
  /** 紧凑模式：指标区使用更小的字号与内边距。 */
  compact?: boolean;
};

export function StatCard({
  label,
  value,
  unit,
  hint,
  tooltip,
  icon: Icon,
  detail,
  compact,
}: StatCardProps) {
  return (
    <DashboardCard className="gap-0 py-0">
      <CardContent className={cn("px-4 py-4 lg:px-5", compact ? "space-y-2 py-3.5" : "space-y-3")}>
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          {Icon ? (
            <span
              className={cn(
                "flex shrink-0 items-center justify-center bg-primary/10 text-primary",
                compact ? "size-6 rounded-md" : "size-7 rounded-lg",
              )}
            >
              <Icon className="size-3.5" aria-hidden="true" />
            </span>
          ) : null}
          <span className="truncate">{label}</span>
          {tooltip ? (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-label={`${label}口径说明`}
                  className="ml-auto rounded-sm text-muted-foreground/70 transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
                >
                  <Info className="size-3.5" aria-hidden="true" />
                </button>
              </TooltipTrigger>
              <TooltipContent className="max-w-64 text-xs leading-relaxed">
                {tooltip}
              </TooltipContent>
            </Tooltip>
          ) : null}
        </div>
        <div className="flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
          <span
            className={cn(
              "font-mono font-bold tracking-tight tabular-nums",
              compact ? "text-xl" : "text-2xl",
            )}
            title={detail}
          >
            {value}
          </span>
          {unit ? <span className="text-xs text-muted-foreground">{unit}</span> : null}
        </div>
        {hint ? (
          <p
            className={cn(
              "text-xs leading-relaxed text-muted-foreground tabular-nums",
              compact && "line-clamp-2",
            )}
          >
            {hint}
          </p>
        ) : null}
      </CardContent>
    </DashboardCard>
  );
}
