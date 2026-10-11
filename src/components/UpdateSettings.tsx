import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { SettingsRow, SettingsSection, SettingsSwitch } from "@/components/SettingsFields";
import type { SettingsController } from "@/hooks/use-settings";
import type { UpdatesController } from "@/hooks/use-updates";
import type { UpdatePhase } from "@/lib/settings-types";
import { openProjectPage, toErrorMessage } from "@/lib/api";
import { formatNumber } from "@/lib/format";

const LABELS: Record<UpdatePhase, string> = {
  disabled: "更新不可用",
  idle: "尚未检查更新",
  checking: "正在检查…",
  up_to_date: "已是最新版本",
  available: "发现新版本",
  downloading: "正在下载并验证…",
  downloaded: "更新包已通过签名验证",
  installing: "正在安装…",
  unavailable: "更新源暂不可用",
  failed: "更新失败",
};

export function UpdateSettings({
  controller,
  updates,
}: {
  controller: SettingsController;
  updates: UpdatesController;
}) {
  const [confirm, setConfirm] = useState(false);
  const state = updates.state;
  const busy =
    updates.busy ||
    state?.phase === "checking" ||
    state?.phase === "downloading" ||
    state?.phase === "installing";
  const percentage = state?.totalBytes
    ? Math.min(100, Math.floor((state.downloadedBytes / state.totalBytes) * 100))
    : null;
  const installLabel = state?.platform === "macos" ? "安装并重启" : "安装更新";
  return (
    <>
      <SettingsSection
        title="更新与关于"
        description="稳定版通过 GitHub Releases 分发，安装前验证更新签名。"
      >
        <SettingsRow title="当前版本">
          <span className="font-mono text-sm">
            {controller.appInfo ? `v${controller.appInfo.version}` : "—"}
          </span>
        </SettingsRow>
        {controller.settings && (
          <SettingsSwitch
            title="自动检查更新"
            description="开启后最多每天访问一次 GitHub 发布信息；不会自动下载或安装。默认关闭。"
            checked={controller.settings.automaticUpdates}
            disabled={controller.busy}
            onChange={(automaticUpdates) => {
              void controller.patch({ automaticUpdates });
            }}
          />
        )}
        <SettingsRow
          title={state ? LABELS[state.phase] : "读取更新状态…"}
          description={state?.version ? `可用版本 v${state.version}` : undefined}
        >
          <div className="flex flex-wrap gap-2">
            {!state && updates.error && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  void updates.reload();
                }}
              >
                重试
              </Button>
            )}
            <Button
              variant="outline"
              size="sm"
              disabled={busy || !state || state.phase === "disabled"}
              onClick={() => {
                void updates.act("check");
              }}
            >
              检查更新
            </Button>
            {state?.version && (state.phase === "available" || state.phase === "failed") && (
              <Button
                size="sm"
                disabled={busy}
                onClick={() => {
                  void updates.act("download");
                }}
              >
                下载更新
              </Button>
            )}
            {state?.phase === "downloaded" && (
              <Button size="sm" disabled={busy} onClick={() => setConfirm(true)}>
                {installLabel}
              </Button>
            )}
          </div>
        </SettingsRow>
        {state?.phase === "downloading" && (
          <div className="space-y-2 py-4" role="status">
            {percentage !== null && <Progress value={percentage} />}
            <p className="font-mono text-xs text-muted-foreground">
              已下载 {formatNumber(state.downloadedBytes)} 字节
              {percentage !== null ? ` · ${percentage}%` : ""}
            </p>
          </div>
        )}
        {(state?.error || updates.error) && (
          <p role="status" className="py-3 text-sm text-muted-foreground">
            {updates.error ?? state?.error}
          </p>
        )}
        {state?.notes && (
          <div className="py-4">
            <p className="mb-2 text-sm font-medium">更新说明</p>
            <p className="text-xs leading-relaxed whitespace-pre-wrap text-muted-foreground">
              {state.notes}
            </p>
          </div>
        )}
        <SettingsRow title="TokLens" description="本地优先的 AI 编程助手用量统计工具。">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              void openProjectPage().catch((cause: unknown) => toast.error(toErrorMessage(cause)));
            }}
          >
            GitHub 项目
          </Button>
        </SettingsRow>
      </SettingsSection>
      <Dialog open={confirm} onOpenChange={setConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{installLabel}？</DialogTitle>
            <DialogDescription>
              应用将等待当前采集结束并关闭。
              {state?.platform === "macos"
                ? "安装完成后自动重启。"
                : "安装程序将更新 TokLens 并重新打开应用。"}
              统计与设置会保留。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setConfirm(false)}>
              取消
            </Button>
            <Button
              disabled={busy}
              onClick={() => {
                setConfirm(false);
                void updates.act("install");
              }}
            >
              {installLabel}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
