import type { LucideIcon } from "lucide-react";
import { StatCard } from "@/components/StatCard";
import { overviewKpiGridClass } from "@/lib/overview-layout";
import { cn } from "@/lib/utils";

export type OverviewKpi = {
  label: string;
  value: string;
  unit?: string;
  hint?: string;
  detail?: string;
  tooltip?: string;
  icon?: LucideIcon;
};

/** 核心指标区：指标独立成紧凑卡片行，数值与口径说明分层呈现。 */
export function OverviewKpiGrid({
  items,
  className,
}: {
  items: OverviewKpi[];
  className?: string;
}) {
  return (
    <div className={cn("grid gap-3", overviewKpiGridClass(items.length), className)}>
      {items.map((item) => (
        <StatCard key={item.label} {...item} compact />
      ))}
    </div>
  );
}
