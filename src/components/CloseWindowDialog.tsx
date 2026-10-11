import { useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  getClosePending,
  getDesktopStatus,
  onDesktopEvent,
  resolveClose,
  toErrorMessage,
} from "@/lib/api";
import type { CloseBehavior } from "@/lib/settings-types";

export function CloseWindowDialog() {
  const [open, setOpen] = useState(false);
  const [remember, setRemember] = useState(false);
  const [trayAvailable, setTrayAvailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const show = useCallback(() => {
    setRemember(false);
    setError(null);
    setOpen(true);
    getDesktopStatus()
      .then((status) => setTrayAvailable(status.trayAvailable))
      .catch((cause: unknown) => setError(toErrorMessage(cause)));
  }, []);

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    onDesktopEvent("close-requested", show)
      .then(async (unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        stop = unlisten;
        if ((await getClosePending()) && !disposed) show();
      })
      .catch(() => {
        /* No native lifecycle in browser preview. */
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [show]);

  const choose = async (action: CloseBehavior | null) => {
    if (busy) return;
    setBusy(true);
    try {
      await resolveClose(action, remember);
      setOpen(false);
    } catch (cause: unknown) {
      setError(toErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) void choose(null);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>关闭 TokLens</DialogTitle>
          <DialogDescription>
            缩小到托盘后仍按设置采集本地用量。退出将停止后台采集。
          </DialogDescription>
        </DialogHeader>
        {!trayAvailable && (
          <p className="text-sm text-muted-foreground">系统托盘当前不可用，可以退出或取消。</p>
        )}
        <label className="flex items-center gap-3 text-sm">
          <Switch checked={remember} onCheckedChange={setRemember} disabled={busy} />
          记住选择（可在设置中修改）
        </label>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <DialogFooter className="sm:justify-between">
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => {
              void choose(null);
            }}
          >
            取消
          </Button>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => {
                void choose("exit");
              }}
            >
              退出应用
            </Button>
            <Button
              disabled={busy || !trayAvailable}
              onClick={() => {
                void choose("tray");
              }}
            >
              缩小到托盘
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
