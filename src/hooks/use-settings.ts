import { useCallback, useEffect, useRef, useState } from "react";
import { useAppTheme } from "@/hooks/use-app-theme";
import { getAppInfo, getSettings, onDesktopEvent, toErrorMessage, updateSettings } from "@/lib/api";
import type { AppInfo, SettingsPatch, SettingsSnapshot } from "@/lib/settings-types";

export function useSettings() {
  const [snapshot, setSnapshot] = useState<SettingsSnapshot | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const saving = useRef(false);
  useAppTheme(snapshot?.settings.theme ?? "system");

  const accept = useCallback((next: SettingsSnapshot) => {
    setSnapshot(next);
    setError(null);
  }, []);

  const reload = useCallback(async () => {
    try {
      accept(await getSettings());
    } catch (cause: unknown) {
      setError(toErrorMessage(cause));
    }
  }, [accept]);

  useEffect(() => {
    let active = true;
    getSettings()
      .then((next) => {
        if (active) accept(next);
      })
      .catch((cause: unknown) => {
        if (active) setError(toErrorMessage(cause));
      });
    getAppInfo()
      .then((info) => {
        if (active) setAppInfo(info);
      })
      .catch(() => {
        /* The settings error explains a missing native runtime. */
      });
    return () => {
      active = false;
    };
  }, [accept]);

  const patch = useCallback(
    async (change: SettingsPatch) => {
      if (saving.current) return;
      saving.current = true;
      setBusy(true);
      try {
        accept(await updateSettings(change));
      } catch (cause: unknown) {
        setError(toErrorMessage(cause));
      } finally {
        saving.current = false;
        setBusy(false);
      }
    },
    [accept],
  );

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    onDesktopEvent("settings-changed", () => {
      void reload();
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(() => {
        /* Native settings are unavailable in browser preview. */
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [reload]);

  return {
    settings: snapshot?.settings ?? null,
    warning: snapshot?.warning ?? null,
    appInfo,
    error,
    busy,
    patch,
    reload,
    accept,
  };
}

export type SettingsController = ReturnType<typeof useSettings>;
