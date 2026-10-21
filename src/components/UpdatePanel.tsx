import type { ReactNode } from "react";
import { Download, LoaderCircle, RefreshCw } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { ReleaseNotes } from "@/components/ReleaseNotes";
import type { UpdatesController } from "@/hooks/use-updates";
import type { UpdatePhase, UpdateState } from "@/lib/settings-types";
import { resolveUpdatePhase, updateErrorText, updatePhaseLabel } from "@/lib/update-phase";

function formatBytes(bytes: number) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** 版本摘要：新版本存在时突出显示版本号，否则显示当前版本。 */
function UpdateSummary({
  state,
  phase,
  currentVersion,
  showStatus,
}: {
  state: UpdateState | null;
  phase: UpdatePhase | null;
  currentVersion: string | null | undefined;
  showStatus: boolean;
}) {
  const version = state?.version;
  const current = currentVersion ? `v${currentVersion}` : "—";
  if (!version) {
    return (
      <div className="space-y-1">
        <p className="text-lg font-semibold tabular-nums">v{current}</p>
        {showStatus && <p className="text-xs text-muted-foreground">{updatePhaseLabel(phase)}</p>}
      </div>
    );
  }
  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        <p className="text-lg font-semibold tabular-nums">v{version}</p>
        {phase === "available" && (
          <Badge variant="outline" className="border-primary/50 text-primary">
            新版本
          </Badge>
        )}
      </div>
      <p className="text-xs text-muted-foreground">当前版本 {current}</p>
    </div>
  );
}

/** 底部操作栏：左侧说明或下载进度，右侧次要操作与主操作。 */
function UpdateActions({
  updates,
  phase,
  busy,
}: {
  updates: UpdatesController;
  phase: UpdatePhase | null;
  busy: boolean;
}) {
  const { state } = updates;
  const percentage = state?.totalBytes
    ? Math.min(100, Math.floor((state.downloadedBytes / state.totalBytes) * 100))
    : null;
  const error = updateErrorText(updates);
  const canDownload = !!state?.version && (phase === "available" || phase === "failed");
  const canInstall = phase === "downloaded";

  let hint: ReactNode = "";
  if (phase === "downloading") {
    hint = (
      <div className="space-y-1.5">
        {percentage !== null && <Progress value={percentage} aria-label="更新下载进度" />}
        <p className="text-xs text-muted-foreground tabular-nums">
          {formatBytes(state?.downloadedBytes ?? 0)}
          {state?.totalBytes ? ` / ${formatBytes(state.totalBytes)}` : ""}
          {percentage !== null ? ` · ${percentage}%` : ""}
        </p>
      </div>
    );
  } else if (error && !busy) {
    hint = (
      <p role="alert" className="text-xs break-words text-destructive">
        {error}
      </p>
    );
  } else if (phase === "available") {
    hint = <p className="text-xs text-muted-foreground">查看更新内容后再决定</p>;
  } else if (canInstall) {
    hint = <p className="text-xs text-muted-foreground">安装会关闭应用，统计与设置保留</p>;
  } else if (phase === "idle") {
    hint = <p className="text-xs text-muted-foreground">自动检查更新默认关闭，可手动检查</p>;
  }

  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className="min-w-0 flex-1">{hint}</div>
      <div className="flex shrink-0 flex-wrap items-center gap-2">
        {canDownload && (
          <Button variant="outline" disabled={busy} onClick={() => void updates.act("check")}>
            <RefreshCw aria-hidden="true" />
            重新检查
          </Button>
        )}
        {canDownload ? (
          <Button disabled={busy} onClick={() => void updates.act("download")}>
            <Download aria-hidden="true" />
            {phase === "failed" ? "重新下载" : "下载更新"}
          </Button>
        ) : canInstall ? (
          <Button disabled={busy} onClick={() => void updates.act("install")}>
            {state?.platform === "macos" ? "安装并重启" : "安装更新"}
          </Button>
        ) : (
          <Button
            disabled={busy || !state || phase === "disabled"}
            onClick={() => void updates.act("check")}
          >
            {busy ? (
              <LoaderCircle className="animate-spin" aria-hidden="true" />
            ) : (
              <RefreshCw aria-hidden="true" />
            )}
            {phase === "checking"
              ? "正在检查…"
              : phase === "downloading"
                ? "正在下载…"
                : phase === "installing"
                  ? "正在安装…"
                  : phase === "failed" || phase === "unavailable"
                    ? "重试检查"
                    : "检查更新"}
          </Button>
        )}
      </div>
    </div>
  );
}

/** Shared update flow for settings and the notification dialog. */
export function UpdatePanel({
  updates,
  currentVersion,
  layout = "inline",
}: {
  updates: UpdatesController;
  currentVersion: string | null | undefined;
  layout?: "inline" | "dialog";
}) {
  const { state } = updates;
  const phase = resolveUpdatePhase(updates);
  const busy =
    updates.busy || phase === "checking" || phase === "downloading" || phase === "installing";
  const notes = state?.notes ?? null;
  const summary = (
    <UpdateSummary
      state={state}
      phase={phase}
      currentVersion={currentVersion}
      showStatus={layout === "inline"}
    />
  );
  const changelog =
    state?.version && notes && phase !== "checking" ? (
      <div className="space-y-3">
        <p className="text-sm font-semibold">更新日志</p>
        <ReleaseNotes notes={notes} omitVersion={state.version} />
      </div>
    ) : null;

  if (layout === "dialog") {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="shrink-0 border-b px-6 py-4">
          {summary}
          <p className="sr-only" role="status" aria-live="polite">
            {updatePhaseLabel(phase)}
          </p>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">{changelog}</div>
        <div className="shrink-0 border-t px-6 py-4">
          <UpdateActions updates={updates} phase={phase} busy={busy} />
        </div>
      </div>
    );
  }

  return (
    <div className="min-w-0 space-y-4">
      {summary}
      <p className="sr-only" role="status" aria-live="polite">
        {updatePhaseLabel(phase)}
      </p>
      {changelog}
      <UpdateActions updates={updates} phase={phase} busy={busy} />
    </div>
  );
}
