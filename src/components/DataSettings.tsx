import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { SettingsRow, SettingsSection } from "@/components/SettingsFields";
import type { SettingsController } from "@/hooks/use-settings";
import { clearStatistics, openAppDataDir, resetSettings, toErrorMessage } from "@/lib/api";

export function DataSettings({ controller }: { controller: SettingsController }) {
  const [action, setAction] = useState<"clear" | "reset" | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const confirm = async () => {
    if (!action || busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await (action === "clear" ? clearStatistics() : resetSettings());
      controller.accept(next);
      toast.success(action === "clear" ? "统计已清除，采集已暂停" : "已恢复默认设置");
      setAction(null);
    } catch (cause: unknown) {
      setError(toErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <SettingsSection
        title="数据与隐私"
        description="统计与设置保存在本机，不上传会话正文或凭据。更新检查仅访问公开发布信息。"
      >
        <SettingsRow title="应用数据目录" description="仅打开 TokLens 自有目录。">
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              void openAppDataDir().catch((cause: unknown) => toast.error(toErrorMessage(cause)));
            }}
          >
            打开目录
          </Button>
        </SettingsRow>
        <SettingsRow
          title="清除统计缓存"
          description="清除统计和导入水位，并暂停采集；客户端原始记录与设置均保留。"
        >
          <Button
            size="sm"
            variant="outline"
            disabled={busy || controller.busy}
            onClick={() => {
              setError(null);
              setAction("clear");
            }}
          >
            清除统计
          </Button>
        </SettingsRow>
        <SettingsRow
          title="恢复默认设置"
          description="关闭开机自启和自动检查，恢复默认采集；保留统计数据。"
        >
          <Button
            size="sm"
            variant="outline"
            disabled={busy || controller.busy}
            onClick={() => {
              setError(null);
              setAction("reset");
            }}
          >
            恢复默认
          </Button>
        </SettingsRow>
      </SettingsSection>
      <Dialog
        open={action !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setAction(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{action === "clear" ? "清除本地统计？" : "恢复默认设置？"}</DialogTitle>
            <DialogDescription>
              {action === "clear"
                ? "等待正在执行的采集结束后清除缓存。采集将暂停，重启也不会自动导入；你可以之后恢复采集。客户端原文件不会被修改。"
                : "窗口、主题、采集和更新偏好将恢复默认值，统计数据保留；采集会恢复。"}
            </DialogDescription>
          </DialogHeader>
          {error && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}
          <DialogFooter>
            <Button variant="ghost" disabled={busy} onClick={() => setAction(null)}>
              取消
            </Button>
            <Button
              variant="destructive"
              disabled={busy}
              onClick={() => {
                void confirm();
              }}
            >
              {busy ? "正在处理…" : "确认"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
