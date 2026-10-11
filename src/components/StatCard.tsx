/** 指标卡片：一个数值 + 说明，不做装饰性排版。 */

import { DashboardCard } from "@/components/DashboardCard";

import { Info, type LucideIcon } from "lucide-react";

import { CardContent } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

export type StatCardProps = {
  label: string;
  value: string;
  unit?: string;
  hint?: string;
  tooltip?: string;
  icon?: LucideIcon;
  detail?: string;
};

export function StatCard({ label, value, unit, hint, tooltip, icon: Icon, detail }: StatCardProps) {
  return (
    <DashboardCard className="gap-0 py-0">
      <CardContent className="space-y-3 px-4 py-4 lg:px-5">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          {Icon ? (
            <span className="flex size-7 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Icon className="size-3.5" aria-hidden="true" />
            </span>
          ) : null}
          <span>{label}</span>
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
        <div className="flex flex-wrap items-baseline gap-1.5">
          <span className="font-mono text-2xl font-bold tracking-tight tabular-nums" title={detail}>
            {value}
          </span>
          {unit ? <span className="text-[10px] text-muted-foreground">{unit}</span> : null}
        </div>
        {hint ? (
          <p className="min-h-4 text-xs leading-relaxed text-muted-foreground tabular-nums">
            {hint}
          </p>
        ) : null}
      </CardContent>
    </DashboardCard>
  );
}
