/** 同步结束后刷新来源状态与统计；事件丢失时通过轮询恢复。 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getDataSourceStatus, onSyncEvent, syncClient, syncUsage, toErrorMessage } from "@/lib/api";
import type { ClientKey, DataSourceStatus, SyncState } from "@/lib/usage-types";

export type SyncController = {
  status: DataSourceStatus | null;
  error: string | null;
  loading: boolean;
  refreshToken: number;
  syncBusy: boolean;
  syncNow: (force?: boolean, client?: ClientKey) => Promise<void>;
  reload: () => Promise<void>;
};

export function useDataSourceStatus(): SyncController {
  const [status, setStatus] = useState<DataSourceStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshToken, setRefreshToken] = useState(0);
  const [syncBusy, setSyncBusy] = useState(false);
  const generation = useRef(0);
  const mounted = useRef(false);
  const completion = useRef<string | null>(null);

  const applySync = useCallback((sync: SyncState, cleared = false) => {
    setSyncBusy(sync.running);
    setStatus((previous) => (previous ? { ...previous, sync } : previous));
    if (!sync.running) {
      const key = `${sync.startedAtMs}:${sync.finishedAtMs}`;
      if (cleared || completion.current !== key) {
        completion.current = key;
        setRefreshToken((value) => value + 1);
      }
    }
  }, []);

  const reload = useCallback(async () => {
    const request = ++generation.current;
    try {
      const next = await getDataSourceStatus();
      if (!mounted.current || request !== generation.current) return;
      setStatus(next);
      applySync(next.sync);
      setError(null);
    } catch (cause: unknown) {
      if (mounted.current && request === generation.current) setError(toErrorMessage(cause));
    }
  }, [applySync]);

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    // Subscribe before the initial read so fast startup syncs cannot fall into a gap.
    void onSyncEvent((sync) => {
      if (disposed) return;
      generation.current += 1;
      applySync(sync, !sync.running && sync.startedAtMs === null && sync.finishedAtMs === null);
      // Progress events only change in-memory state; query the database on completion.
      if (!sync.running) void reload();
    })
      .then((stop) => {
        if (disposed) stop();
        else {
          unlisten = stop;
          void reload();
        }
      })
      .catch(() => {
        if (!disposed) void reload();
      });
    return () => {
      disposed = true;
      mounted.current = false;
      generation.current += 1;
      unlisten?.();
    };
  }, [applySync, reload]);

  useEffect(() => {
    const timer = window.setInterval(
      () => {
        void reload();
      },
      syncBusy ? 2_000 : 30_000,
    );
    const onFocus = () => {
      void reload();
    };
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [syncBusy, reload]);

  const syncNow = useCallback(
    async (force = false, client?: ClientKey) => {
      setSyncBusy(true);
      try {
        if (client) await syncClient(client, force);
        else await syncUsage(force);
        // The command reply can be older than a fast completion event.
        await reload();
      } catch (cause: unknown) {
        if (mounted.current) {
          setError(toErrorMessage(cause));
          setSyncBusy(false);
        }
      }
    },
    [reload],
  );

  return {
    status,
    error,
    loading: status === null && error === null,
    refreshToken,
    syncBusy,
    syncNow,
    reload,
  };
}
