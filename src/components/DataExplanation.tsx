import type { ReactNode } from "react";
import { Info } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

export function DataExplanation({ summary, children }: { summary: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
      <span>{summary}</span>
      <Popover>
        <PopoverTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1 px-2 text-xs"
            aria-label="查看统计口径与数据覆盖"
          >
            <Info className="size-3.5" />
            数据说明
          </Button>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          className="max-h-80 w-80 overflow-y-auto text-xs leading-relaxed"
        >
          <div className="space-y-2">{children}</div>
        </PopoverContent>
      </Popover>
    </div>
  );
}
