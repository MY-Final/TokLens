/** 会话明细：按区间与客户端筛选，展示元数据与用量（无正文）。 */

import { DashboardCard } from "@/components/DashboardCard";
import { FilterSelectContent } from "@/components/FilterSelectContent";
import { CardContent } from "@/components/ui/card";

import { useState } from "react";

import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { SessionTable } from "@/components/SessionTable";
import { SessionDetailPanel } from "@/components/SessionDetailPanel";
import { useAsyncData } from "@/hooks/use-async-data";
import { getSessionDetail, getSessions } from "@/lib/api";
import type { ClientKey, SessionSummary } from "@/lib/usage-types";
import { formatNumber } from "@/lib/format";
import type { DateRange } from "@/lib/ranges";

const SESSION_LIMIT = 200;

function SelectedSessionDetail({
  session,
  refreshToken,
  onClose,
}: {
  session: SessionSummary;
  refreshToken: number;
  onClose: () => void;
}) {
  const [offset, setOffset] = useState(0);
  const detail = useAsyncData(
    () =>
      getSessionDetail({
        client: session.client,
        clientSessionId: session.clientSessionId,
        limit: 100,
        offset,
      }),
    `session-detail:${session.client}:${session.clientSessionId}:${refreshToken}:${offset}`,
  );
  return (
    <SessionDetailPanel
      selectedSession={session}
      detail={detail}
      onClose={onClose}
      offset={offset}
      onOffsetChange={setOffset}
    />
  );
}

export function SessionsPage({ range, refreshToken }: { range: DateRange; refreshToken: number }) {
  const [clientValue, setClientValue] = useState("all");
  const [selectedSession, setSelectedSession] = useState<SessionSummary | null>(null);
  const client: ClientKey | null = clientValue === "all" ? null : (clientValue as ClientKey);
  const keySuffix = `${range.key}:${clientValue}:${range.fromMs ?? "all"}:${range.toMs}:${refreshToken}`;

  const sessions = useAsyncData(
    () =>
      getSessions({
        fromMs: range.fromMs,
        toMs: range.toMs,
        client,
        limit: SESSION_LIMIT,
      }),
    `session-list:${keySuffix}`,
  );

  const rows = sessions.data ?? [];
  const withoutUsage = rows.filter((session) => session.totalTokens === null).length;
  const partial = rows.filter((session) => session.coverage === "partial").length;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Select value={clientValue} onValueChange={setClientValue}>
          <SelectTrigger size="sm" className="w-44" aria-label="选择客户端">
            <SelectValue />
          </SelectTrigger>
          <FilterSelectContent>
            <SelectItem value="all">全部客户端</SelectItem>
            <SelectItem value="codex">Codex</SelectItem>
            <SelectItem value="claude_code">Claude Code</SelectItem>
            <SelectItem value="opencode">OpenCode</SelectItem>
            <SelectItem value="dsh">DSH</SelectItem>
            <SelectItem value="kimi_code">Kimi Code</SelectItem>
            <SelectItem value="mimocode">MiMoCode</SelectItem>
            <SelectItem value="pi_desktop">Pi Desktop</SelectItem>
          </FilterSelectContent>
        </Select>
        <p className="text-xs text-muted-foreground tabular-nums">
          显示 {formatNumber(rows.length)} / 最多 {formatNumber(SESSION_LIMIT)} 个会话
          {withoutUsage > 0 ? ` · ${formatNumber(withoutUsage)} 个无用量数据` : ""}
          {partial > 0 ? ` · ${formatNumber(partial)} 个明细不完整` : ""}
        </p>
      </div>

      <DashboardCard>
        <CardContent className="px-5">
          <SessionTable
            sessions={sessions.data}
            loading={sessions.loading}
            error={sessions.error}
            onRetry={sessions.reload}
            onOpenSession={setSelectedSession}
          />
        </CardContent>
      </DashboardCard>

      <p className="text-xs leading-relaxed text-muted-foreground">
        列表按会话开始时间倒序；Token 为会话累计值（父子集字段已扣除），"—"
        表示客户端没有记录该会话的用量。 会话内容、提示词与工具输出不读取、不保存。
      </p>
      {selectedSession ? (
        <SelectedSessionDetail
          key={`${selectedSession.client}:${selectedSession.clientSessionId}`}
          session={selectedSession}
          refreshToken={refreshToken}
          onClose={() => setSelectedSession(null)}
        />
      ) : null}
    </div>
  );
}
