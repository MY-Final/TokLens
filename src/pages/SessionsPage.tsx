/** 会话明细：按区间与客户端筛选，展示元数据与用量（无正文）。 */

import { DashboardCard } from "@/components/DashboardCard";
import { DataExplanation } from "@/components/DataExplanation";
import { FilterSelectContent } from "@/components/FilterSelectContent";
import { CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

import { useState } from "react";
import { Input } from "@/components/ui/input";

import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { SessionTable } from "@/components/SessionTable";
import { SessionDetailPanel } from "@/components/SessionDetailPanel";
import { useAsyncData } from "@/hooks/use-async-data";
import { getSessionDetail, getSessions } from "@/lib/api";
import type {
  SessionBrowserState,
  SessionFilters,
  SessionSort,
  SessionSummary,
} from "@/lib/usage-types";
import { CLIENT_LABELS, clientLabel, isClientKey } from "@/lib/client-labels";
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

export function SessionsPage({
  range,
  refreshToken,
  state,
  onChange,
  onReturn,
  returnLabel,
}: {
  range: DateRange;
  refreshToken: number;
  state: SessionBrowserState;
  onChange: (state: SessionBrowserState) => void;
  onReturn?: () => void;
  returnLabel?: string;
}) {
  return (
    <SessionBrowser
      key={rangeScope(range)}
      range={range}
      refreshToken={refreshToken}
      state={state}
      onChange={onChange}
      onReturn={onReturn}
      returnLabel={returnLabel}
    />
  );
}

function SessionBrowser({
  range,
  refreshToken,
  state,
  onChange,
  onReturn,
  returnLabel,
}: {
  range: DateRange;
  refreshToken: number;
  state: SessionBrowserState;
  onChange: (state: SessionBrowserState) => void;
  onReturn?: () => void;
  returnLabel?: string;
}) {
  const [selectedSession, setSelectedSession] = useState<SessionSummary | null>(null);
  const { client, search, sort } = state;
  const clientValue = client ?? "all";
  const scope = rangeScope(range);
  const offset = state.rangeScope === scope ? state.offset : 0;
  const setOffset = (value: number) => onChange({ ...state, offset: value, rangeScope: scope });
  const changeFilters = (patch: Partial<SessionFilters>) => {
    setSelectedSession(null);
    onChange({ ...state, ...patch, offset: 0, rangeScope: scope });
  };
  const normalizedSearch = search.trim();
  const keySuffix = `${range.key}:${JSON.stringify(state)}:${range.fromMs ?? "all"}:${range.toMs}:${refreshToken}`;

  const sessions = useAsyncData(
    () =>
      getSessions({
        fromMs: range.fromMs,
        toMs: range.toMs,
        client,
        model: state.model,
        projectPath: state.projectPath,
        requestTime: state.requestTime,
        search: normalizedSearch || null,
        sort,
        limit: SESSION_LIMIT + 1,
        offset,
      }),
    `session-list:${keySuffix}:${offset}`,
    `${scope}:${JSON.stringify(state)}:${offset}`,
  );

  const rows = sessions.data?.slice(0, SESSION_LIMIT) ?? [];
  const hasNext = (sessions.data?.length ?? 0) > SESSION_LIMIT;
  const withoutUsage = rows.filter((session) => session.totalTokens === null).length;
  const partial = rows.filter((session) => session.coverage === "partial").length;

  return (
    <div className="space-y-4">
      {onReturn ? (
        <Button size="sm" variant="outline" onClick={onReturn}>
          {returnLabel ?? "返回统计"}
        </Button>
      ) : null}
      <div className="space-y-3">
        <div className="flex flex-wrap items-center gap-3">
          <Input
            value={search}
            onChange={(event) => changeFilters({ search: event.target.value })}
            placeholder="搜索项目、模型或会话 ID"
            aria-label="搜索项目、模型或会话 ID"
            className="h-9 w-72 max-w-full min-w-0 flex-1 basis-64"
          />
          <div className="flex flex-wrap items-center gap-3">
            <Select
              value={sort}
              onValueChange={(value) => changeFilters({ sort: value as SessionSort })}
            >
              <SelectTrigger className="h-9 w-40 shrink-0" aria-label="会话排序">
                <SelectValue />
              </SelectTrigger>
              <FilterSelectContent>
                <SelectItem value="newest">最近开始</SelectItem>
                <SelectItem value="oldest">最早开始</SelectItem>
                <SelectItem value="tokens_desc">Token 最高</SelectItem>
                <SelectItem value="requests_desc">请求最多</SelectItem>
              </FilterSelectContent>
            </Select>
            <Select
              value={clientValue}
              onValueChange={(value) => {
                changeFilters({ client: isClientKey(value) ? value : null });
                setSelectedSession(null);
              }}
            >
              <SelectTrigger className="h-9 w-44 shrink-0" aria-label="选择客户端">
                <SelectValue />
              </SelectTrigger>
              <FilterSelectContent>
                <SelectItem value="all">全部客户端</SelectItem>
                {Object.keys(CLIENT_LABELS)
                  .filter(isClientKey)
                  .map((entry) => (
                    <SelectItem key={entry} value={entry}>
                      {clientLabel(entry)}
                    </SelectItem>
                  ))}
              </FilterSelectContent>
            </Select>
          </div>
        </div>
        <div className="space-y-2">
          <p className="text-xs leading-relaxed text-muted-foreground tabular-nums">
            第 {formatNumber(offset / SESSION_LIMIT + 1)} 页 ·{" "}
            {sessions.loading ? "加载中…" : `本页 ${formatNumber(rows.length)} 个会话`}
            {withoutUsage > 0 ? ` · ${formatNumber(withoutUsage)} 个无用量数据` : ""}
            {partial > 0 ? ` · ${formatNumber(partial)} 个明细不完整` : ""}
          </p>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2 text-xs text-muted-foreground">
            <span>{state.requestTime ? "区间内有已记录请求的会话" : "按会话开始时间"}</span>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground"
              onClick={() => changeFilters({ requestTime: !state.requestTime })}
            >
              切换为{state.requestTime ? "会话开始时间" : "请求发生时间"}
            </Button>
            {state.model !== undefined ? (
              <Button
                size="sm"
                variant="outline"
                onClick={() => changeFilters({ model: undefined })}
              >
                模型：{state.model ?? "未记录"} ×
              </Button>
            ) : null}
            {state.projectPath !== undefined ? (
              <Button
                size="sm"
                variant="outline"
                className="max-w-full"
                onClick={() => changeFilters({ projectPath: undefined })}
                title={state.projectPath ?? "未记录项目"}
              >
                <span className="truncate">项目：{state.projectPath ?? "未记录"}</span> ×
              </Button>
            ) : null}
          </div>
        </div>
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

      <DataExplanation
        summary={`${state.requestTime ? "区间内有已记录请求" : "按会话开始时间"} · 会话累计`}
      >
        <p>
          {state.requestTime
            ? "筛选区间内有非估算、有时间请求记录的会话，包含区间前开始的会话。"
            : "按会话开始时间筛选。"}
          表中 Token 始终为整个会话累计，不等于区间内请求用量。
        </p>
        <p>
          搜索仅匹配项目路径、会话主模型和会话 ID
          的文字片段；模型与项目下钻为精确匹配，未知组仅匹配缺失字段。排序保持稳定，未知值置于已知值之后。
        </p>
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
