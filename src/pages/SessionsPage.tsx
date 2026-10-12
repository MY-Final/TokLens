/** 会话明细：按区间与客户端筛选，展示元数据与用量（无正文）。 */

import { DashboardCard } from "@/components/DashboardCard";
import { DataExplanation } from "@/components/DataExplanation";
import { FilterSelectContent } from "@/components/FilterSelectContent";
import { CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

import { useState } from "react";

import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { SessionTable } from "@/components/SessionTable";
import { SessionDetailPanel } from "@/components/SessionDetailPanel";
import { useAsyncData } from "@/hooks/use-async-data";
import { getSessionDetail, getSessions } from "@/lib/api";
import type { ClientKey, SessionSummary } from "@/lib/usage-types";
import { formatNumber } from "@/lib/format";
import { rangeScope, type DateRange } from "@/lib/ranges";

const SESSION_LIMIT = 50;

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
    `${session.client}:${session.clientSessionId}:${offset}`,
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
  return (
    <SessionBrowser
      key={rangeScope(range)}
      range={range}
      refreshToken={refreshToken}
      clientValue={clientValue}
      onClientChange={setClientValue}
    />
  );
}

function SessionBrowser({
  range,
  refreshToken,
  clientValue,
  onClientChange,
}: {
  range: DateRange;
  refreshToken: number;
  clientValue: string;
  onClientChange: (value: string) => void;
}) {
  const [offset, setOffset] = useState(0);
  const [selectedSession, setSelectedSession] = useState<SessionSummary | null>(null);
  const client: ClientKey | null = clientValue === "all" ? null : (clientValue as ClientKey);
  const keySuffix = `${range.key}:${clientValue}:${range.fromMs ?? "all"}:${range.toMs}:${refreshToken}`;

  const sessions = useAsyncData(
    () =>
      getSessions({
        fromMs: range.fromMs,
        toMs: range.toMs,
        client,
        limit: SESSION_LIMIT + 1,
        offset,
      }),
    `session-list:${keySuffix}:${offset}`,
    `${rangeScope(range)}:${clientValue}:${offset}`,
  );

  const rows = sessions.data?.slice(0, SESSION_LIMIT) ?? [];
  const hasNext = (sessions.data?.length ?? 0) > SESSION_LIMIT;
  const withoutUsage = rows.filter((session) => session.totalTokens === null).length;
  const partial = rows.filter((session) => session.coverage === "partial").length;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Select
          value={clientValue}
          onValueChange={(value) => {
            onClientChange(value);
            setOffset(0);
            setSelectedSession(null);
          }}
        >
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
            <SelectItem value="workbuddy">WorkBuddy</SelectItem>
          </FilterSelectContent>
        </Select>
        <p className="text-xs text-muted-foreground tabular-nums">
          第 {formatNumber(offset / SESSION_LIMIT + 1)} 页 ·{" "}
          {sessions.loading ? "加载中…" : `本页 ${formatNumber(rows.length)} 个会话`}
          {withoutUsage > 0 ? ` · ${formatNumber(withoutUsage)} 个无用量数据` : ""}
          {partial > 0 ? ` · ${formatNumber(partial)} 个明细不完整` : ""}
        </p>
      </div>

      <DashboardCard>
        <CardContent className="px-5">
          <SessionTable
            sessions={sessions.data === null ? null : rows}
            loading={sessions.loading}
            error={sessions.error}
            onRetry={sessions.reload}
            onOpenSession={setSelectedSession}
          />
        </CardContent>
      </DashboardCard>
      <nav aria-label="会话分页" className="flex items-center justify-end gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={offset === 0 || sessions.loading}
          onClick={() => setOffset(0)}
        >
          首页
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={offset === 0 || sessions.loading}
          onClick={() => setOffset(Math.max(0, offset - SESSION_LIMIT))}
        >
          上一页
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={!hasNext || sessions.loading || sessions.error !== null}
          onClick={() => setOffset(offset + SESSION_LIMIT)}
        >
          下一页
        </Button>
      </nav>

      <DataExplanation summary="按会话开始时间 · 会话累计">
        <p>按开始时间筛选并倒序排列。Token 为会话累计，重叠子集已扣除。</p>
        <p>— 表示尚未导入可用总量，可在数据源管理检查读取路径和采集状态。</p>
      </DataExplanation>
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
