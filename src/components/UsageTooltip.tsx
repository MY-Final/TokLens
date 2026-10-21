import type { ReactNode } from "react";
import { formatNumber } from "@/lib/format";
import { useTokenFormat } from "@/hooks/use-token-format";
import { useStatisticMode } from "@/hooks/use-statistic-mode";

export function UsageTooltip({
  title,
  totalLabel,
  total,
  requests,
  sessions,
  unknown = 0,
  partial = 0,
  basis,
  children,
  breakdown = [],
  breakdownUnit = "Token",
}: {
  title: string;
  totalLabel?: string;
  total: number | null;
  requests?: number;
  sessions?: number;
  unknown?: number;
  partial?: number;
  basis: "requests" | "sessions";
  children?: ReactNode;
  breakdown?: { key: string; label: string; value: number | null; note?: string }[];
  breakdownUnit?: "Token" | "条请求";
}) {
  const { formatTokens } = useTokenFormat();
  const { tooltipBasis } = useStatisticMode();
  const sorted = [...breakdown].sort((a, b) =>
    a.value === null ? (b.value === null ? 0 : 1) : b.value === null ? -1 : b.value - a.value,
  );
  const exact = total !== null && formatTokens(total) !== formatNumber(total);
  return (
    <div className="min-w-56 space-y-2 text-xs">
      <p className="font-medium">{title}</p>
      <div className="border-b pb-2">
        <p className="text-muted-foreground">{totalLabel ?? "已知 Token"}</p>
        <p className="font-mono text-lg font-semibold tabular-nums">
          {total === null ? "—（未知）" : formatTokens(total)}{" "}
          <span className="font-sans text-xs font-normal">Token</span>
        </p>
      </div>
      <dl className="space-y-1">
        {requests !== undefined ? (
          <div className="flex justify-between gap-6">
            <dt>{basis === "requests" ? "请求数" : "已采集请求明细"}</dt>
            <dd className="font-mono tabular-nums">{formatNumber(requests)}</dd>
          </div>
        ) : null}
        {sessions !== undefined ? (
          <div className="flex justify-between gap-6">
            <dt>{basis === "requests" ? "有请求会话" : "会话数"}</dt>
            <dd className="font-mono tabular-nums">{formatNumber(sessions)}</dd>
          </div>
        ) : null}
      </dl>
      {children}
      {sorted.length > 0 ? (
        <dl className="space-y-1 border-t pt-2">
          {sorted.map((entry) => (
            <div key={entry.key} className="flex justify-between gap-4">
              <dt className="text-muted-foreground">
                {entry.label}
                {entry.note ? <span className="block text-xs">{entry.note}</span> : null}
              </dt>
              <dd
                className="shrink-0 font-mono tabular-nums"
                title={
                  entry.value === null
                    ? "用量未知"
                    : `${formatNumber(entry.value)} ${breakdownUnit}`
                }
              >
                {entry.value === null
                  ? "—"
                  : breakdownUnit === "Token"
                    ? formatTokens(entry.value)
                    : formatNumber(entry.value)}{" "}
                {breakdownUnit}
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
      {/* 口径与精度信息默认折叠，避免浮框被说明文字淹没。 */}
      <details className="group">
        <summary className="cursor-pointer list-none text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring">
          <span className="group-open:hidden">展开口径</span>
          <span className="hidden group-open:inline">收起口径</span>
        </summary>
        <div className="mt-1 space-y-1 border-t pt-2 text-muted-foreground">
          {exact ? <p className="tabular-nums">精确值 {formatNumber(total)} Token</p> : null}
          {unknown > 0 ? (
            <p>
              {total !== null ? "已知子集 · " : ""}
              {formatNumber(unknown)} {basis === "requests" ? "条请求" : "个会话"}用量未知
            </p>
          ) : null}
          {partial > 0 ? <p>{formatNumber(partial)} 个会话明细不完整</p> : null}
          {basis === "requests" && requests === 0 ? <p>未记录请求，不代表没有活动。</p> : null}
          <p>{tooltipBasis} · 不含估算</p>
        </div>
      </details>
    </div>
  );
}
