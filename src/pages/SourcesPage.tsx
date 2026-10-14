/** 数据源管理：客户端接入状态、本地库路径、同步操作与上次同步明细。 */

import { DashboardCard } from "@/components/DashboardCard";
import { DataExplanation } from "@/components/DataExplanation";

import { Copy, Database, FolderSearch, RefreshCw, ShieldCheck } from "lucide-react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import type { SettingsController } from "@/hooks/use-settings";
import { ClientIdentity } from "@/components/ClientIdentity";
import { SyncControl } from "@/components/SyncControl";
import { ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import { useSyncing } from "@/hooks/use-sync-status";
import type { ClientKey, DataSourceStatus, SyncState } from "@/lib/usage-types";
import { formatDateTime, formatNumber, formatRelative } from "@/lib/format";
import { GROK_USAGE_NOTE, clientLabel } from "@/lib/client-labels";
import { sourceDiagnosticSummary, sourceStatusLabel } from "@/lib/source-diagnostics";

const CLIENT_HINTS: Record<string, string> = {
  codex: "Codex：CODEX_HOME（VS Code 与 TokLens 需使用相同用户和环境）",
  claude_code: "Claude Code：CLAUDE_CONFIG_DIR",
  dsh: "DSH：DSH_SESSIONS_DIR",
  mimocode: "MiMoCode：MIMOCODE_DB / XDG_DATA_HOME",
  pi_desktop: "Pi Desktop：PI_DESKTOP_SESSIONS_DIR",
  workbuddy: "WorkBuddy：WORKBUDDY_CONFIG_DIR",
  copilot: "Copilot：CLI 与 VS Code Agent Host（自动发现本机默认目录）",
  gemini_cli: "Gemini CLI：tmp 项目会话（排除 Antigravity）",
  grok: "Grok CLI：GROK_HOME 下的 sessions 用量快照",
  mcode: "MCode：MINIMAX_DATA_DIR / MAVIS_DATA_DIR 下的 v2 用量数据库",
  kimi_code: "Kimi Code：KIMI_CODE_HOME",
  opencode: "OpenCode：XDG_DATA_HOME",
};

export function SourcesPage({
  status,
  loading,
  error,
  syncBusy,
  onRetry,
  onSync,
  onSyncClient,
  settingsController,
}: {
  status: DataSourceStatus | null;
  loading: boolean;
  error: string | null;
  syncBusy: boolean;
  onRetry: () => void;
  onSync: (force?: boolean) => void;
  onSyncClient: (client: ClientKey) => void;
  settingsController: SettingsController;
}) {
  const sync: SyncState | null = status?.sync ?? null;
  const report = sync?.report ?? null;
  const syncing = useSyncing();

  return (
    <div className="space-y-4">
      {error ? <ErrorBlock message={error} onRetry={onRetry} /> : null}
      {settingsController.error ? (
        <ErrorBlock
          message={settingsController.error}
          onRetry={() => {
            void settingsController.reload();
          }}
        />
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <DashboardCard>
          <CardHeader>
            <CardTitle>同步</CardTitle>
          </CardHeader>
          <CardContent>
            <SyncControl
              sync={sync}
              busy={syncBusy}
              onSync={onSync}
              paused={settingsController.settings?.collectionPaused ?? false}
              size="default"
            />
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
                    未发现客户端数据源。请确认客户端已产生本地会话记录。
                  </p>
                ) : null}
                {report.filesFailed + report.directoriesFailed > 0 ? (
                  <p className="text-xs text-destructive">
                    {formatNumber(report.filesFailed)} 个文件、
                    {formatNumber(report.directoriesFailed)} 个目录读取失败，将在下次同步重试。
                  </p>
                ) : null}
              </div>
            ) : null}
          </CardContent>
        </DashboardCard>

        <DashboardCard>
          <CardHeader>
            <CardTitle>本地数据库</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            {(loading || syncing) && !status ? (
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
                </dl>
                <p className="flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
                  <ShieldCheck className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
                  仅存统计，不存正文
                </p>
              </>
            )}
          </CardContent>
        </DashboardCard>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        {(status?.clients ?? []).map((client) => {
          const result = client.lastResult;
          const scan = result?.report;
          const enabled =
            settingsController.settings?.enabledClients.includes(client.client) ?? false;
          const diagnosticSummary = sourceDiagnosticSummary(
            client,
            settingsController.appInfo?.version ?? null,
          );
          return (
            <DashboardCard
              key={client.client}
              className={client.supported ? "" : "border-dashed opacity-50"}
            >
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <ClientIdentity client={client.client} compact />
                  <Switch
                    className="ml-auto"
                    aria-label={`采集 ${clientLabel(client.client)}`}
                    checked={
                      settingsController.settings?.enabledClients.includes(client.client) ?? false
                    }
                    disabled={settingsController.busy || !settingsController.settings}
                    onCheckedChange={(enabled) => {
                      const previous = settingsController.settings?.enabledClients ?? [];
                      void settingsController.patch({
                        enabledClients: enabled
                          ? [...previous, client.client]
                          : previous.filter((key) => key !== client.client),
                      });
                    }}
                  />
                  {client.supported ? (
                    <Badge variant="secondary" className="font-normal">
                      {settingsController.settings &&
                      !settingsController.settings.enabledClients.includes(client.client)
                        ? "已停用"
                        : sourceStatusLabel(client)}
                    </Badge>
                  ) : (
                    <Badge variant="outline" className="font-normal text-muted-foreground">
                      等待采集
                    </Badge>
                  )}
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-2">
                <p className="text-xs text-muted-foreground">实际读取位置</p>
                {client.client === "grok" ? (
                  <p className="text-xs text-muted-foreground">{GROK_USAGE_NOTE}</p>
                ) : client.client === "mcode" ? (
                  <p className="text-xs text-muted-foreground">
                    缺失模型保持未知；推理关系未确认时，总量显示 —。
                  </p>
                ) : client.client === "gemini_cli" ? (
                  <p className="text-xs text-muted-foreground">
                    工具用量关系尚未确认，分项可能不等于客户端自报总量。
                  </p>
                ) : null}
                {client.sourcePaths.length > 0 ? (
                  client.sourcePaths.map((source) => (
                    <div key={source.path} className="space-y-1">
                      <code className="block text-xs break-all select-text">{source.path}</code>
                      <p className="text-xs text-muted-foreground">
                        {source.state === "available"
                          ? "根路径可访问（不代表全部文件可读）"
                          : source.state === "missing"
                            ? "来源不存在"
                            : "无法读取，请检查权限或文件占用"}
                      </p>
                    </div>
                  ))
                ) : (
                  <p className="text-xs text-muted-foreground">无法确定用户目录</p>
                )}
                {client.lastCheckedMs !== null ? (
                  <p className="text-xs text-muted-foreground">
                    最近检查（本次运行）：{formatDateTime(client.lastCheckedMs)}
                  </p>
                ) : null}
                {result?.error ? <p className="text-xs text-destructive">{result?.error}</p> : null}
                {scan ? (
                  <p className="text-xs text-muted-foreground">
                    本次发现 {formatNumber(scan.filesSeen)} 个文件 · 解析{" "}
                    {formatNumber(scan.filesParsed)} · 未变化{" "}
                    {formatNumber(scan.filesSkippedUnchanged)} · 失败{" "}
                    {formatNumber(scan.filesFailed)}
                    {scan.directoriesFailed > 0
                      ? ` · 目录失败 ${formatNumber(scan.directoriesFailed)}`
                      : ""}
                  </p>
                ) : null}
                {(scan?.filesFailed ?? 0) + (scan?.directoriesFailed ?? 0) > 0 ? (
                  <p className="text-xs text-destructive">读取失败，保留历史，下次同步重试。</p>
                ) : null}
                {(scan?.unparsedLines ?? 0) + (scan?.filesWithoutIdentity ?? 0) > 0 ? (
                  <p className="text-xs text-muted-foreground">
                    部分记录无法解析或缺少会话标识；已知数据仍可查看，缺失部分不补零。
                  </p>
                ) : null}
                {client.supported ? (
                  <>
                    <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
                      <dt className="text-muted-foreground">会话</dt>
                      <dd className="text-right tabular-nums">{formatNumber(client.sessions)}</dd>
                      <dt className="text-muted-foreground">请求明细</dt>
                      <dd className="text-right tabular-nums">{formatNumber(client.messages)}</dd>
                      <dt className="text-muted-foreground">已跟踪文件</dt>
                      <dd className="text-right tabular-nums">
                        {formatNumber(client.filesTracked)}
                      </dd>
                      <dt className="text-muted-foreground">无法解析的行</dt>
                      <dd className="text-right tabular-nums">
                        {formatNumber(client.unparsedRecords)}
                      </dd>
                      <dt className="text-muted-foreground">最近导入</dt>
                      <dd className="text-right tabular-nums">
                        {formatRelative(client.lastScanMs)}
                      </dd>
                      <dt className="text-muted-foreground">最近用量记录</dt>
                      <dd className="text-right tabular-nums">
                        {formatDateTime(client.lastRequestMs)}
                      </dd>
                      <dt className="text-muted-foreground">Token 总量未知的会话</dt>
                      <dd className="text-right tabular-nums">
                        {formatNumber(client.sessionsWithoutUsage)}
                      </dd>
                    </dl>
                    {client.sessionsWithoutUsage > 0 ? (
                      <p className="text-xs text-muted-foreground">
                        {client.sessionsWithoutUsage === client.sessions
                          ? "已有会话，但未记录可确认的 Token 总量。"
                          : "部分会话 Token 总量未知，只统计已知子集。"}
                        未知不代表 0；重扫不会估算缺失用量。
                      </p>
                    ) : null}
                    <div className="flex flex-wrap gap-2 pt-2">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={
                          syncBusy ||
                          !enabled ||
                          (settingsController.settings?.collectionPaused ?? false)
                        }
                        onClick={() => onSyncClient(client.client)}
                      >
                        <RefreshCw aria-hidden="true" />
                        重扫此客户端
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                          void navigator.clipboard.writeText(diagnosticSummary).then(
                            () => toast.success("已复制脱敏诊断摘要"),
                            () => toast.error("复制失败，请检查剪贴板权限"),
                          );
                        }}
                      >
                        <Copy aria-hidden="true" />
                        复制脱敏诊断
                      </Button>
                    </div>
                    <details className="text-xs text-muted-foreground">
                      <summary className="cursor-pointer py-1">预览脱敏诊断摘要</summary>
                      <pre className="mt-2 max-h-64 overflow-auto rounded-md bg-muted/60 p-3 text-xs break-all whitespace-pre-wrap select-text">
                        {diagnosticSummary}
                      </pre>
                    </details>
                    {client.sessions === 0 && !loading ? (
                      <p className="flex items-center gap-2 text-xs text-muted-foreground">
                        <FolderSearch className="size-3.5" aria-hidden="true" />
                        暂无数据，点击刷新导入。
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
          );
        })}
        {!status && !loading && !syncing ? (
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

      <DataExplanation summary="只读采集">
        <p>暂停或停用客户端保留已有统计。自定义目录配置：</p>
        <p>
          最近用量记录仅使用已导入的非估算请求原生时间；Grok
          为轮结束时间，无时间记录不补造。最近导入来自成功解析水位，未变化跳过和失败检查不会推进；每客户端检查结果仅保留在本次应用运行中。
        </p>
        <p>
          根路径可访问不代表所有会话文件可读。脱敏诊断不含路径、会话标识和错误原文，仅在点击复制时写入本机剪贴板。
        </p>
        {Object.entries(CLIENT_HINTS).map(([client, hint]) => (
          <p key={client}>{hint}</p>
        ))}
        <p>Codex / OpenCode / WorkBuddy 的缓存写入关系未知，不计入总量。</p>
      </DataExplanation>
    </div>
  );
}
