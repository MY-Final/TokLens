/** 业务卡片统一外观；保持 shadcn 生成组件不变。 */

import type { ComponentProps } from "react";

import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";

export function DashboardCard({ className, ...props }: ComponentProps<typeof Card>) {
  return (
    <Card
      className={cn(
        "gap-5 rounded-xl border-border/60 bg-card/50 py-5 shadow-sm shadow-foreground/[0.02] backdrop-blur-sm",
        className,
      )}
      {...props}
    />
  );
}
