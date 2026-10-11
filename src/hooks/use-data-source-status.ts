/** 数据源状态与同步控制：启动时读取一次，同步结束事件到达后通知页面刷新。 */

import { useCallback, useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { getDataSourceStatus, onSyncEvent, syncUsage, toErrorMessage } from "@/lib/api";
import type { DataSourceStatus, SyncState } from "@/lib/usage-types";

const RUNNING_POLL_MS = 2_000;

export type SyncController = {
  status: DataSourceStatus | null;
  error: string | null;
  loading: boolean;
  /** 每次同步结束后自增，页面把它拼进查询 key 即可自动重新拉取。 */
  refreshToken: number;
  syncBusy: boolean;
  syncNow: (force?: boolean) => Promise<void>;
  reload: () => Promise<void>;
};

export function useDataSourceStatus(): SyncController {
  const [status, setStatus] = useState<DataSourceStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshToken, setRefreshToken] = useState(0);
  const [syncBusy, setSyncBusy] = useState(false);
  // 首次加载：还没有任何结果（派生值，避免在 effect 中同步 setState）。
  const loading = status === null && error === null;

  const applySync = useCallback((sync: SyncState) => {
    setStatus((previous) => (previous ? { ...previous, sync } : previous));
    setSyncBusy(sync.running);
    if (!sync.running) {
      setRefreshToken((token) => token + 1);
    }
  }, []);

  const reload = useCallback(async () => {
    try {
      const next = await getDataSourceStatus();
      setStatus(next);
      setSyncBusy(next.sync.running);
      setError(null);
    } catch (cause: unknown) {
      setError(toErrorMessage(cause));
    }
  }, []);

  useEffect(() => {
    // 首次加载直接走 promise 回调（不在 effect 体内同步 setState）。
    let active = true;
    getDataSourceStatus()
      .then((next) => {
        if (active) {
          setStatus(next);
          setSyncBusy(next.sync.running);
        }
      })
      .catch((cause: unknown) => {
        if (active) {
          setError(toErrorMessage(cause));
        }
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    onSyncEvent(applySync)
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unlisten = stop;
        }
      })
      .catch(() => {
        // 事件订阅不可用时忽略：状态仍可通过 reload 获取。
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [applySync]);

  // 同步进行中：轻量轮询状态，让"已导入会话数"等计数在首次导入时逐步增长。
  useEffect(() => {
    if (!syncBusy) {
      return;
    }
    const timer = window.setInterval(() => {
      void reload();
    }, RUNNING_POLL_MS);
    return () => window.clearInterval(timer);
  }, [syncBusy, reload]);

  const syncNow = useCallback(
    async (force = false) => {
      setSyncBusy(true);
      try {
        const state = await syncUsage(force);
        applySync(state);
      } catch (cause: unknown) {
        setError(toErrorMessage(cause));
        setSyncBusy(false);
      }
    },
    [applySync],
  );

  return { status, error, loading, refreshToken, syncBusy, syncNow, reload };
}
