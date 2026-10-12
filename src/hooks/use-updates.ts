import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import {
  checkUpdates,
  downloadUpdate,
  getUpdateState,
  installUpdate,
  onUpdateState,
  toErrorMessage,
} from "@/lib/api";
import type { UpdateState } from "@/lib/settings-types";

export function useUpdates() {
  const [state, setState] = useState<UpdateState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [operation, setOperation] = useState<"check" | "download" | "install" | null>(null);
  const pending = useRef(false);
  const [checkSequence, setCheckSequence] = useState(0);
  const accept = useCallback((next: UpdateState) => {
    setState(next);
  }, []);
  useEffect(() => {
    let disposed = false;
    let receivedEvent = false;
    let stop: (() => void) | undefined;
    onUpdateState((next) => {
      receivedEvent = true;
      if (!disposed) accept(next);
    })
      .then(async (unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        stop = unlisten;
        const next = await getUpdateState();
        if (!disposed && !receivedEvent) accept(next);
      })
      .catch((cause: unknown) => {
        if (!disposed) setError(toErrorMessage(cause));
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [accept]);

  const act = useCallback(
    async (operation: "check" | "download" | "install") => {
      if (pending.current) return;
      pending.current = true;
      setBusy(true);
      setOperation(operation);
      setError(null);
      try {
        if (operation === "install") await installUpdate();
        else {
          const next = await (operation === "check" ? checkUpdates() : downloadUpdate());
          accept(next);
          if (operation === "check") {
            setCheckSequence((sequence) => sequence + 1);
            if (next.phase === "up_to_date") toast.success("当前已是最新版本");
            else if (next.phase === "disabled")
              toast.info(next.error ?? "当前环境不支持应用内更新");
            else if (next.phase === "failed" || next.phase === "unavailable")
              toast.error(next.error ?? "检查更新失败，请重试");
          }
        }
      } catch (cause: unknown) {
        setError(toErrorMessage(cause));
        if (operation === "check") toast.error(`检查更新失败：${toErrorMessage(cause)}`);
      } finally {
        pending.current = false;
        setBusy(false);
        setOperation(null);
      }
    },
    [accept],
  );
  const reload = async () => {
    try {
      accept(await getUpdateState());
      setError(null);
    } catch (cause: unknown) {
      setError(toErrorMessage(cause));
    }
  };
  return { state, error, busy, operation, checkSequence, act, reload };
}
export type UpdatesController = ReturnType<typeof useUpdates>;
