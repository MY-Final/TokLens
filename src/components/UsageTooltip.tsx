import type { ReactNode } from "react";
import { formatNumber } from "@/lib/format";

export function UsageTooltip({
  title,
  total,
  requests,
  sessions,
  unknown = 0,
  partial = 0,
  basis,
  children,
}: {
  title: string;
  total: number | null;
  requests?: number;
  sessions?: number;
  unknown?: number;
  partial?: number;
  basis: "requests" | "sessions";
  children?: ReactNode;
}) {
  return (
    <div className="min-w-56 space-y-2 text-xs">
      <p className="font-medium">{title}</p>
      <div className="border-b pb-2">
        <p className="text-muted-foreground">
          {title.includes("所选时段") ? "所选时段已知 Token" : "当日已知 Token"}
          {unknown > 0 && total !== null ? " · 已知子集" : ""}
        </p>
        <p className="font-mono text-lg font-semibold tabular-nums">
          {total === null ? "—（未知）" : formatNumber(total)}{" "}
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
      {basis === "requests" && requests === 0 ? (
        <p className="text-muted-foreground">未记录请求，不代表无活动。</p>
      ) : null}
      {children}
      {unknown > 0 ? (
        <p className="text-muted-foreground">
          {formatNumber(unknown)} {basis === "requests" ? "条请求" : "个会话"}
          总量未知，未知不等于零。
        </p>
      ) : null}
      {partial > 0 ? (
        <p className="text-muted-foreground">
          {formatNumber(partial)} 个会话明细不完整，已知总量仍保留。
        </p>
      ) : null}
      <p className="text-muted-foreground">
        {basis === "requests"
          ? "请求时间 · 实测 / 换算 · 不含估算"
          : "会话开始日 · 会话累计 · 不含估算"}
      </p>
    </div>
  );
}
