import { useCallback, useEffect, useState } from "react";
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

  const act = async (operation: "check" | "download" | "install") => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      if (operation === "install") await installUpdate();
      else accept(await (operation === "check" ? checkUpdates() : downloadUpdate()));
    } catch (cause: unknown) {
      setError(toErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  const reload = async () => {
    try {
      accept(await getUpdateState());
      setError(null);
    } catch (cause: unknown) {
      setError(toErrorMessage(cause));
    }
  };
  return { state, error, busy, act, reload };
}
export type UpdatesController = ReturnType<typeof useUpdates>;
