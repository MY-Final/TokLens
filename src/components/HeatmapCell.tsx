import { useTokenFormat } from "@/hooks/use-token-format";
import type { ReactNode } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";

const INTENSITY_CLASSES = [
  "bg-muted",
  "bg-chart-1/20",
  "bg-chart-1/40",
  "bg-chart-1/70",
  "bg-chart-1",
] as const;

function intensityClass(value: number | null, maxValue: number): string {
  if (value === null || value <= 0 || maxValue <= 0) return INTENSITY_CLASSES[0];
  const level = Math.min(4, Math.max(1, Math.ceil((value / maxValue) * 4)));
  return INTENSITY_CLASSES[level] ?? INTENSITY_CLASSES[0];
}

/** Color depth is a display scale; values and completeness come from Rust. */
export function HeatmapCell({
  value,
  maxValue,
  label,
  partial = false,
  className,
  children,
}: {
  value: number | null;
  maxValue: number;
  label: string;
  partial?: boolean;
  className?: string;
  children?: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          className={cn(
            "relative flex size-4 shrink-0 items-center justify-center rounded-sm border border-border/60 text-xs text-muted-foreground transition-[outline-color] hover:outline-2 hover:outline-ring focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
            intensityClass(value, maxValue),
            value === null || partial ? "border-dashed border-foreground/50" : "",
            className,
          )}
        >
          {value === null ? <span aria-hidden="true">—</span> : null}
          {partial && value !== null ? (
            <span
              aria-hidden="true"
              className="absolute top-0 right-0 size-1 rounded-full bg-foreground"
            />
          ) : null}
        </button>
      </TooltipTrigger>
      <TooltipContent className="max-w-72 space-y-1 text-xs leading-relaxed">
        {children ?? label}
      </TooltipContent>
    </Tooltip>
  );
}

export function HeatmapLegend({ maxValue, unit }: { maxValue: number; unit: string }) {
  const { formatTokens } = useTokenFormat();
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-muted-foreground">
      <span className="inline-flex items-center gap-1.5">
        <span>0</span>
        {INTENSITY_CLASSES.map((className) => (
          <span
            key={className}
            aria-hidden="true"
            className={cn("size-3 rounded-sm border border-border/60", className)}
          />
        ))}
        <span title={`${formatNumber(maxValue)} ${unit}`}>
          {unit.toLowerCase() === "token" ? formatTokens(maxValue) : formatNumber(maxValue)} {unit}
        </span>
      </span>
      <span className="inline-flex items-center gap-1.5">
        <span
          aria-hidden="true"
          className="flex size-3 items-center justify-center rounded-sm border border-dashed border-foreground/50 bg-muted"
        >
          —
        </span>
        未知
      </span>
      <span className="inline-flex items-center gap-1.5">
        <span
          aria-hidden="true"
          className="relative size-3 rounded-sm border border-dashed border-foreground/50 bg-muted"
        >
          <span className="absolute top-0 right-0 size-1 rounded-full bg-foreground" />
        </span>
        数据不完整
      </span>
    </div>
  );
}
