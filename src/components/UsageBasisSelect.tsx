import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";

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
  return (
    <Tabs
      value={value}
      onValueChange={(next) => onChange(next === "sessions" ? "sessions" : "requests")}
    >
      <TabsList aria-label="统计口径" className="h-9 rounded-lg bg-muted/70 p-[3px]">
        <TabsTrigger value="requests" className={TRIGGER_CLASS}>
          按请求时间
        </TabsTrigger>
        <TabsTrigger
          value="sessions"
          className={TRIGGER_CLASS}
          title="会话视角：会话数、会话排行与日趋势；用量按请求时间归集"
        >
          按会话汇总
        </TabsTrigger>
      </TabsList>
    </Tabs>
  );
}
