import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
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
import {
  clearStatistics,
  getStorageStatus,
  openAppDataDir,
  relocateStorage,
  resetSettings,
  toErrorMessage,
} from "@/lib/api";
import type { StorageStatus } from "@/lib/settings-types";

export function DataSettings({ controller }: { controller: SettingsController }) {
  const [action, setAction] = useState<"clear" | "reset" | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [storage, setStorage] = useState<StorageStatus | null>(null);
  const [storageError, setStorageError] = useState<string | null>(null);
  const [storageAttempt, setStorageAttempt] = useState(0);
  const [storageLoading, setStorageLoading] = useState(true);
  const [locationOpen, setLocationOpen] = useState(false);
  const [directory, setDirectory] = useState("");
  const [moving, setMoving] = useState(false);
  const [moveError, setMoveError] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    getStorageStatus()
      .then((next) => {
        if (active) setStorage(next);
      })
      .catch((cause: unknown) => {
        if (active) setStorageError(toErrorMessage(cause));
      })
      .finally(() => {
        if (active) setStorageLoading(false);
      });
    return () => {
      active = false;
    };
  }, [storageAttempt]);
  const move = async () => {
    if (moving || busy || controller.busy || !directory.trim()) return;
    setMoving(true);
    setMoveError(null);
    try {
      const next = await relocateStorage(directory.trim());
      setStorage(next);
      setLocationOpen(false);
      toast.success("存储位置已更改");
      if (next.warning) toast.warning(next.warning);
    } catch (cause: unknown) {
      setMoveError(toErrorMessage(cause));
    } finally {
      setMoving(false);
    }
  };
  const confirm = async () => {
    if (!action || busy || moving) return;
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
      <SettingsSection title="数据与隐私" description="只读采集，本地存储，不保存会话正文。">
        <div className="space-y-3 py-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-sm font-medium">存储位置</p>
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                disabled={!storage || storageLoading || busy || moving || controller.busy}
                onClick={() => {
                  void openAppDataDir().catch((cause: unknown) =>
                    toast.error(toErrorMessage(cause)),
                  );
                }}
              >
                打开目录
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={!storage || storageLoading || busy || moving || controller.busy}
                onClick={() => {
                  setDirectory(storage?.directory ?? "");
                  setMoveError(null);
                  setLocationOpen(true);
                }}
              >
                更改位置
              </Button>
            </div>
          </div>
          {storageLoading ? (
            <Skeleton className="h-5 w-full max-w-sm" aria-label="正在读取存储位置" />
          ) : storageError ? (
            <div className="flex flex-wrap items-center gap-3">
              <p role="alert" className="text-sm text-destructive">
                {storageError}
              </p>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  setStorageError(null);
                  setStorageLoading(true);
                  setStorageAttempt((attempt) => attempt + 1);
                }}
              >
                重试
              </Button>
            </div>
          ) : storage ? (
            <p className="font-mono text-xs break-all text-muted-foreground select-text">
              {storage.directory}
            </p>
          ) : (
            <p className="text-sm text-muted-foreground">尚未读取存储位置，请重试。</p>
          )}
          {storage?.warning && (
            <p role="status" className="text-xs text-muted-foreground">
              {storage.warning}
            </p>
          )}
        </div>
        <SettingsRow title="清除统计缓存" description="清除后暂停采集，保留原始记录和设置。">
          <Button
            size="sm"
            variant="outline"
            disabled={busy || moving || controller.busy}
            onClick={() => {
              setError(null);
              setAction("clear");
            }}
          >
            清除统计
          </Button>
        </SettingsRow>
        <SettingsRow title="恢复默认设置" description="保留统计和存储位置，恢复采集。">
          <Button
            size="sm"
            variant="outline"
            disabled={busy || moving || controller.busy}
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
        open={locationOpen}
        onOpenChange={(open) => {
          if (!moving) setLocationOpen(open);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>更改存储位置</DialogTitle>
            <DialogDescription>
              等待当前采集结束后，将统计数据与设置迁移到新目录并立即使用，无需重启。恢复默认设置不会还原存储位置。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-1">
              <p className="text-xs text-muted-foreground">当前位置</p>
              <p className="font-mono text-xs break-all select-text">{storage?.directory}</p>
            </div>
            <div className="space-y-2">
              <label htmlFor="storage-directory" className="text-sm font-medium">
                新目录（绝对路径）
              </label>
              <Input
                id="storage-directory"
                value={directory}
                disabled={moving}
                placeholder="D:\TokLensData"
                onChange={(event) => {
                  setDirectory(event.target.value);
                  setMoveError(null);
                }}
                className="font-mono text-sm"
                aria-describedby="storage-directory-note"
              />
              <p id="storage-directory-note" className="text-xs text-muted-foreground">
                系统应用目录仍保留小型位置记录，窗口状态和系统缓存由系统管理。
              </p>
            </div>
            {moving && (
              <p role="status" className="text-sm text-muted-foreground">
                正在等待采集结束并迁移，请勿退出应用…
              </p>
            )}
            {moveError && (
              <p role="alert" className="text-sm text-destructive">
                {moveError}
              </p>
            )}
          </div>
          <DialogFooter>
            <Button variant="ghost" disabled={moving} onClick={() => setLocationOpen(false)}>
              取消
            </Button>
            <Button
              disabled={
                moving ||
                busy ||
                controller.busy ||
                !directory.trim() ||
                directory.trim() === storage?.directory
              }
              onClick={() => {
                void move();
              }}
            >
              {moving ? "正在迁移…" : "迁移并使用"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
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
                : "窗口、主题、采集和更新偏好将恢复默认值，统计数据和存储位置保留；采集会恢复。"}
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
