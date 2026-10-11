/** 数据源管理：客户端接入状态、本地库路径、同步操作与上次同步明细。 */

import { DashboardCard } from "@/components/DashboardCard";

import { Database, FolderSearch, ShieldCheck } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { ClientIdentity } from "@/components/ClientIdentity";
import { SyncControl } from "@/components/SyncControl";
import { ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import type { DataSourceStatus, SyncState } from "@/lib/api";
import { formatDateTime, formatNumber, formatRelative } from "@/lib/format";

const CLIENT_HINTS: Record<string, string> = {
  codex: "读取 %USERPROFILE%\\.codex\\sessions 下的 rollout-*.jsonl（只读）。",
  claude_code: "采集器尚未实现：接入后会读取 ~/.claude/projects 下的会话转录。",
  opencode: "采集器尚未实现：接入后会读取本地 opencode.db（只读）。",
};

export function SourcesPage({
  status,
  loading,
  error,
  syncBusy,
  refreshToken,
  onRetry,
  onSync,
}: {
  status: DataSourceStatus | null;
  loading: boolean;
  error: string | null;
  syncBusy: boolean;
  refreshToken: number;
  onRetry: () => void;
  onSync: (force?: boolean) => void;
}) {
  const sync: SyncState | null = status?.sync ?? null;
  const report = sync?.report ?? null;

  return (
    <div className="space-y-4">
      {error ? <ErrorBlock message={error} onRetry={onRetry} /> : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <DashboardCard>
          <CardHeader>
            <CardTitle>同步</CardTitle>
            <CardDescription>
              启动时会自动执行一次增量同步；只解析发生变化的文件，其余直接跳过。
            </CardDescription>
          </CardHeader>
          <CardContent>
            <SyncControl sync={sync} busy={syncBusy} onSync={onSync} size="default" />
            {report ? (
              <div className="mt-4 space-y-2">
                <Separator />
                <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
                  <dt className="text-muted-foreground">扫描文件</dt>
                  <dd className="text-right tabular-nums">{formatNumber(report.filesSeen)}</dd>
                  <dt className="text-muted-foreground">本次解析</dt>
                  <dd className="text-right tabular-nums">{formatNumber(report.filesParsed)}</dd>
                  <dt className="text-muted-foreground">未变化跳过</dt>
                  <dd className="text-right tabular-nums">
                    {formatNumber(report.filesSkippedUnchanged)}
                  </dd>
                  <dt className="text-muted-foreground">导入会话 / 明细</dt>
                  <dd className="text-right tabular-nums">
                    {formatNumber(report.sessionsImported)} /{" "}
                    {formatNumber(report.messagesInserted)}
                  </dd>
                  <dt className="text-muted-foreground">清理旧明细</dt>
                  <dd className="text-right tabular-nums">{formatNumber(report.messagesPruned)}</dd>
                  <dt className="text-muted-foreground">无法解析的行</dt>
                  <dd className="text-right tabular-nums">{formatNumber(report.unparsedLines)}</dd>
                </dl>
                {report.rootMissing ? (
                  <p className="text-xs text-destructive">
                    未找到 Codex 数据目录：%USERPROFILE%\.codex\sessions 不存在。
                  </p>
                ) : null}
                {report.filesFailed > 0 ? (
                  <p className="text-xs text-destructive">
                    {formatNumber(report.filesFailed)} 个文件读取失败，将在下次同步重试。
                  </p>
                ) : null}
              </div>
            ) : null}
          </CardContent>
        </DashboardCard>

        <DashboardCard>
          <CardHeader>
            <CardTitle>本地数据库</CardTitle>
            <CardDescription>
              只写入 TokLens 专属应用数据目录；客户端原始数据始终只读。
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {loading && !status ? (
              <LoadingRows rows={3} />
            ) : (
              <>
                <div className="flex items-start gap-2">
                  <Database className="mt-0.5 size-4 text-muted-foreground" aria-hidden="true" />
                  <code className="rounded-sm bg-muted/60 px-2 py-1 text-xs break-all text-muted-foreground">
                    {status?.databasePath ?? "—"}
                  </code>
                </div>
                <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
                  <dt className="text-muted-foreground">最早会话</dt>
                  <dd className="text-right tabular-nums">
                    {formatDateTime(status?.firstEventMs ?? null)}
                  </dd>
                  <dt className="text-muted-foreground">最近会话</dt>
                  <dd className="text-right tabular-nums">
                    {formatDateTime(status?.lastEventMs ?? null)}
                  </dd>
                  <dt className="text-muted-foreground">费用金额</dt>
                  <dd className="text-right text-muted-foreground">不展示（无可靠来源）</dd>
                </dl>
                <p className="flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
                  <ShieldCheck className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
                  仅保存统计元数据（时间、模型、项目路径、Token
                  计数），不保存会话正文、提示词或凭据。
                  {refreshToken > 0 ? " 数据已随最近一次同步刷新。" : ""}
                </p>
              </>
            )}
          </CardContent>
        </DashboardCard>
      </div>

      <div className="grid gap-4 lg:grid-cols-3">
        {(status?.clients ?? []).map((client) => (
          <DashboardCard
            key={client.client}
            className={client.supported ? "" : "border-dashed opacity-50"}
          >
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <ClientIdentity client={client.client} compact />
                {client.supported ? (
                  <Badge variant="secondary" className="font-normal">
                    已接入
                  </Badge>
                ) : (
                  <Badge variant="outline" className="font-normal text-muted-foreground">
                    等待采集
                  </Badge>
                )}
              </CardTitle>
              <CardDescription>{CLIENT_HINTS[client.client]}</CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              {client.supported ? (
                <>
                  <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
                    <dt className="text-muted-foreground">会话</dt>
                    <dd className="text-right tabular-nums">{formatNumber(client.sessions)}</dd>
                    <dt className="text-muted-foreground">请求明细</dt>
                    <dd className="text-right tabular-nums">{formatNumber(client.messages)}</dd>
                    <dt className="text-muted-foreground">已跟踪文件</dt>
                    <dd className="text-right tabular-nums">{formatNumber(client.filesTracked)}</dd>
                    <dt className="text-muted-foreground">无法解析的行</dt>
                    <dd className="text-right tabular-nums">
                      {formatNumber(client.unparsedRecords)}
                    </dd>
                    <dt className="text-muted-foreground">最近扫描</dt>
                    <dd className="text-right tabular-nums">{formatRelative(client.lastScanMs)}</dd>
                  </dl>
                  {client.sessions === 0 && !loading ? (
                    <p className="flex items-center gap-2 text-xs text-muted-foreground">
                      <FolderSearch className="size-3.5" aria-hidden="true" />
                      还没有导入数据，先执行一次同步。
                    </p>
                  ) : null}
                </>
              ) : (
                <p className="text-xs leading-relaxed text-muted-foreground">
                  采集器尚未接入，暂不参与用量统计。
                </p>
              )}
            </CardContent>
          </DashboardCard>
        ))}
        {!status && !loading ? (
          <DashboardCard>
            <CardContent className="py-4">
              <p className="text-xs text-muted-foreground">数据源状态不可用。</p>
              <Button size="sm" variant="outline" className="mt-2" onClick={onRetry}>
                重新读取
              </Button>
            </CardContent>
          </DashboardCard>
        ) : null}
      </div>

      <p className="text-xs leading-relaxed text-muted-foreground">
        口径未知项：Codex
        的“缓存写入”与输入的关系无法从本机数据判定，因此不计入总量，仅在会话与分桶中单独展示。
      </p>
    </div>
  );
}
