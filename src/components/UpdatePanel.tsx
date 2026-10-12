import { Download, LoaderCircle, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import type { UpdatesController } from "@/hooks/use-updates";
import type { UpdatePhase } from "@/lib/settings-types";
import { releaseNotesText } from "@/lib/release-notes";

const LABELS: Record<UpdatePhase, string> = {
  disabled: "更新不可用",
  idle: "检查是否有新版本",
  checking: "正在检查更新…",
  up_to_date: "已是最新版本",
  available: "发现新版本",
  downloading: "正在下载更新…",
  downloaded: "更新已准备好",
  installing: "正在安装更新…",
  unavailable: "暂时无法检查更新",
  failed: "更新未完成",
};
function formatBytes(bytes: number) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Shared update flow for settings and the notification dialog. */
export function UpdatePanel({
  updates,
  currentVersion,
}: {
  updates: UpdatesController;
  currentVersion: string | null | undefined;
}) {
  const { state, operation } = updates;
  const phase =
    operation === "check"
      ? "checking"
      : operation === "download"
        ? "downloading"
        : operation === "install"
          ? "installing"
          : state?.phase;
  const busy =
    updates.busy || phase === "checking" || phase === "downloading" || phase === "installing";
  const canDownload = !!state?.version && (phase === "available" || phase === "failed");
  const canInstall = phase === "downloaded";
  const percentage = state?.totalBytes
    ? Math.min(100, Math.floor((state.downloadedBytes / state.totalBytes) * 100))
    : null;
  const error = updates.error ?? state?.error;
  const notes = releaseNotesText(state?.notes ?? null);
  return (
    <div className="min-w-0 space-y-4">
      <div aria-live="polite" role="status" className="space-y-1">
        <p className="text-base font-medium">
          {phase ? LABELS[phase] : "读取更新状态…"}
          {state?.version && phase !== "checking" ? ` v${state.version}` : ""}
        </p>
        <p className="text-xs text-muted-foreground">
          当前版本 {currentVersion ? `v${currentVersion}` : "—"}
        </p>
      </div>
      {phase === "downloading" && (
        <div className="space-y-2" role="status">
          {percentage !== null && <Progress value={percentage} aria-label="更新下载进度" />}
          <p className="text-xs text-muted-foreground tabular-nums">
            {formatBytes(state?.downloadedBytes ?? 0)}
            {state?.totalBytes ? ` / ${formatBytes(state.totalBytes)}` : ""}
            {percentage !== null ? ` · ${percentage}%` : ""}
          </p>
        </div>
      )}
      {error && !busy && (
        <p role="alert" className="text-sm break-words text-muted-foreground">
          {error}
        </p>
      )}
      {canInstall && (
        <p className="text-xs text-muted-foreground">安装时会关闭应用，统计与设置保留。</p>
      )}
      <div className="flex flex-wrap items-center gap-2">
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
        {canDownload && (
          <Button variant="ghost" disabled={busy} onClick={() => void updates.act("check")}>
            重新检查
          </Button>
        )}
        {!state && updates.error && (
          <Button variant="outline" disabled={busy} onClick={() => void updates.reload()}>
            重试
          </Button>
        )}
      </div>
      {state?.version && notes && phase !== "checking" && (
        <div className="space-y-2 border-t pt-4">
          <p className="text-sm font-medium">更新内容</p>
          <p className="max-h-[35vh] overflow-y-auto text-sm leading-relaxed break-words whitespace-pre-wrap text-muted-foreground">
            {notes}
          </p>
        </div>
      )}
    </div>
  );
}
