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
          <TabsTrigger
            value="sessions"
            title="会话视角：会话数、会话排行与日趋势；用量按请求时间归集"
          >
            按会话汇总
          </TabsTrigger>
        </TabsList>
      </Tabs>
    </div>
  );
}
