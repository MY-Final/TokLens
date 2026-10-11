import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";

export type UsageBasis = "requests" | "sessions";

export function UsageBasisSelect({
  value,
  onChange,
}: {
  value: UsageBasis;
  onChange: (value: UsageBasis) => void;
}) {
  return (
    <div className="space-y-2">
      <Tabs
        value={value}
        onValueChange={(next) => onChange(next === "sessions" ? "sessions" : "requests")}
      >
        <TabsList aria-label="统计口径">
          <TabsTrigger value="requests">按请求时间</TabsTrigger>
          <TabsTrigger value="sessions">会话累计</TabsTrigger>
        </TabsList>
      </Tabs>
      <p className="text-xs leading-relaxed text-muted-foreground">
        {value === "requests"
          ? "按每条已记录请求的发生时间筛选，跨天会话的请求分别归入对应日期。"
          : "按会话开始时间筛选，统计整个会话累计用量；与按请求时间统计的范围和覆盖不同。"}
      </p>
    </div>
  );
}
