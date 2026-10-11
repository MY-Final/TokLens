import { ClientIdentity } from "@/components/ClientIdentity";
import { DashboardCard } from "@/components/DashboardCard";
import { HeatmapCell, HeatmapLegend } from "@/components/HeatmapCell";
import { EmptyBlock, ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { clientLabel } from "@/lib/client-labels";
import { formatDateTime, formatMonthDay, formatNumber } from "@/lib/format";
import type { ClientKey, DailyTokenBucket } from "@/lib/usage-types";

const CLIENTS: ClientKey[] = [
  "codex",
  "claude_code",
  "opencode",
  "dsh",
  "kimi_code",
  "mimocode",
  "pi_desktop",
];

export function ClientDateHeatmap({
  buckets,
  loading,
  error,
  onRetry,
  rangeLabel,
  client,
}: {
  buckets: DailyTokenBucket[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  rangeLabel: string;
  client?: ClientKey | null;
}) {
  const days = buckets ?? [];
  const clients = client ? [client] : CLIENTS;
  const rows = clients.map((key) => ({
    client: key,
    cells: days.map((day) => {
      const entry = day.clients.find((item) => item.client === key);
      const sessions = entry?.sessions ?? 0;
      const missing = entry?.sessionsWithoutUsage ?? 0;
      const partial = entry?.sessionsPartial ?? 0;
      return {
        dayStartMs: day.dayStartMs,
        sessions,
        missing,
        partial,
        value: sessions > 0 && sessions === missing ? null : (entry?.totalTokens ?? 0),
      };
    }),
  }));
  const maxValue = Math.max(0, ...rows.flatMap((row) => row.cells.map((cell) => cell.value ?? 0)));
  const hasData = rows.some((row) => row.cells.some((cell) => cell.sessions > 0));
  const labelInterval = days.length > 31 ? 7 : days.length > 14 ? 3 : 2;
  const firstDay = days[0];
  const lastDay = days[days.length - 1];

  return (
    <DashboardCard className="min-w-0">
      <CardHeader className="px-5">
        <CardTitle className="text-sm">客户端 × 日期 Token 热力图</CardTitle>
        <CardDescription className="text-xs">
          {rangeLabel} · 本地时区 · 单位 Token · 所有客户端使用同一色阶
        </CardDescription>
      </CardHeader>
      <CardContent className="min-w-0 space-y-4 px-5">
        {error ? (
          <ErrorBlock message={error} onRetry={onRetry} />
        ) : loading ? (
          <LoadingRows rows={7} />
        ) : !hasData ? (
          <EmptyBlock
            title="所选区间没有会话记录"
            description="换个时间范围或客户端，或点击刷新同步本地客户端数据。"
          />
        ) : (
          <div className="max-w-full overflow-x-auto pb-2">
            <table
              className="w-max min-w-full border-separate border-spacing-0 text-xs"
              aria-label={`${rangeLabel} 客户端每日 Token，按本地会话开始日期归属`}
            >
              <thead>
                <tr>
                  <th
                    scope="col"
                    className="sticky left-0 z-10 min-w-40 bg-card pr-4 pb-3 text-left font-normal text-muted-foreground"
                  >
                    客户端 / 日期
                  </th>
                  {days.map((day, index) => (
                    <th
                      key={day.dayStartMs}
                      scope="col"
                      className="h-8 min-w-8 pb-3 text-left font-normal whitespace-nowrap text-muted-foreground"
                      aria-label={formatDateTime(day.dayStartMs).split(" ")[0]}
                    >
                      <span className="block w-8 overflow-visible">
                        {index % labelInterval === 0 ? formatMonthDay(day.dayStartMs) : null}
                      </span>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.client}>
                    <th scope="row" className="sticky left-0 z-10 bg-card py-1 pr-4 text-left">
                      <ClientIdentity client={row.client} compact />
                    </th>
                    {row.cells.map((cell) => {
                      const date = formatDateTime(cell.dayStartMs).split(" ")[0];
                      const tokenLabel =
                        cell.value === null ? "—，用量未知" : `${formatNumber(cell.value)} Token`;
                      const isPartial = cell.missing > 0 || cell.partial > 0;
                      return (
                        <td key={cell.dayStartMs} className="py-1 pr-1">
                          <HeatmapCell
                            value={cell.value}
                            maxValue={maxValue}
                            partial={isPartial}
                            className="size-7"
                            label={`${clientLabel(row.client)} · ${date} · ${tokenLabel}${isPartial ? " · 数据不完整" : ""}`}
                          >
                            <div className="space-y-1 text-xs">
                              <p className="font-medium">
                                {clientLabel(row.client)} · {date}
                              </p>
                              <p className="tabular-nums">{tokenLabel}</p>
                              <p>{formatNumber(cell.sessions)} 个会话</p>
                              {cell.value === null ? (
                                <p>所有会话均缺少用量记录，未知不等于 0。</p>
                              ) : cell.missing > 0 ? (
                                <p>
                                  仅含已知总量；{formatNumber(cell.missing)}{" "}
                                  个会话用量未知，未计入。
                                </p>
                              ) : (
                                <p>客户端记录的已知总量 · 无估算</p>
                              )}
                              {cell.partial > 0 ? (
                                <p>
                                  {formatNumber(cell.partial)}{" "}
                                  个会话请求明细不完整；已知累计总量仍按客户端记录展示。
                                </p>
                              ) : null}
                              {cell.sessions === 0 ? <p>当天没有该客户端的会话。</p> : null}
                              <p>本地会话开始日期归属累计用量，不代表当天请求增量。</p>
                            </div>
                          </HeatmapCell>
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {!loading && !error && hasData ? <HeatmapLegend maxValue={maxValue} unit="Token" /> : null}
        {!loading && !error && firstDay && lastDay ? (
          <p className="font-mono text-xs text-muted-foreground">
            展示日期：{formatDateTime(firstDay.dayStartMs).split(" ")[0]} —{" "}
            {formatDateTime(lastDay.dayStartMs).split(" ")[0]}
          </p>
        ) : null}
        <p className="text-xs leading-relaxed text-muted-foreground">
          按本地会话开始日期归属累计用量，不代表当天请求增量。色阶比较各客户端已知 Token
          总量，缓存按各客户端口径计入，缺失值不推算。
        </p>
      </CardContent>
    </DashboardCard>
  );
}
