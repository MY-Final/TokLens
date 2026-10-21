import { Info } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useStatisticMode } from "@/hooks/use-statistic-mode";

export type UsageBasis = "requests" | "sessions";

const TRIGGER_CLASS =
  "h-8 rounded-md px-3 text-sm data-[state=active]:bg-primary data-[state=active]:text-primary-foreground data-[state=active]:shadow-none dark:data-[state=active]:border-transparent dark:data-[state=active]:bg-primary dark:data-[state=active]:text-primary-foreground";

/** 统计口径切换：与时间范围分开呈现，当前项为实心主色。 */
export function UsageBasisSelect({
  value,
  onChange,
}: {
  value: UsageBasis;
  onChange: (value: UsageBasis) => void;
}) {
  const { explain, attribution } = useStatisticMode();
  return (
    <div className="flex min-w-0 items-center gap-2">
      <span className="hidden shrink-0 text-xs text-muted-foreground lg:inline">统计口径</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            aria-label={`统计口径说明：${attribution}`}
            className="hidden rounded-sm text-muted-foreground/70 transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring lg:block"
          >
            <Info className="size-3.5" aria-hidden="true" />
          </button>
        </TooltipTrigger>
        <TooltipContent className="max-w-72 text-xs leading-relaxed">{explain}</TooltipContent>
      </Tooltip>
      <Tabs
        value={value}
        onValueChange={(next) => onChange(next === "sessions" ? "sessions" : "requests")}
      >
        <TabsList
          aria-label="统计口径"
          title={explain}
          className="h-9 rounded-lg bg-muted/70 p-[3px]"
        >
          <TabsTrigger value="requests" className={TRIGGER_CLASS}>
            按请求时间
          </TabsTrigger>
          <TabsTrigger value="sessions" className={TRIGGER_CLASS}>
            按会话汇总
          </TabsTrigger>
        </TabsList>
      </Tabs>
    </div>
  );
}
