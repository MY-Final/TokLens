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
          <TabsTrigger value="sessions" title="按会话开始日期筛选，展示整个会话累计用量">
            按会话开始日
          </TabsTrigger>
        </TabsList>
      </Tabs>
    </div>
  );
}
