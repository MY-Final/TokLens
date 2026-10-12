import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { onDesktopError, onDesktopEvent, onTrayNavigate } from "@/lib/api";
import { CloseWindowDialog } from "@/components/CloseWindowDialog";
import { UpdateAnnouncement } from "@/components/UpdateAnnouncement";

import { AppShell, type PageKey } from "@/components/AppShell";
import { useDataSourceStatus } from "@/hooks/use-data-source-status";
import { AnalyticsPage } from "@/pages/AnalyticsPage";
import { OverviewPage } from "@/pages/OverviewPage";
import { SessionsPage } from "@/pages/SessionsPage";
import { SourcesPage } from "@/pages/SourcesPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { useSettings } from "@/hooks/use-settings";
import { TokenDisplayContext } from "@/hooks/use-token-format";
import { useUpdates } from "@/hooks/use-updates";
import { useLocalClock } from "@/hooks/use-local-clock";
import { useTrayUsage } from "@/hooks/use-tray-usage";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { resolveRange, type DateRange } from "@/lib/ranges";

export default function App() {
  const [page, setPage] = useState<PageKey>("overview");
  const [updateFocusToken, setUpdateFocusToken] = useState(0);
  const [selectedRange, setSelectedRange] = useState<DateRange | null>(null);
  const nowMs = useLocalClock();
  const settings = useSettings();
  const updates = useUpdates();
  const latestUpdates = useRef(updates);
  const trayCheckPending = useRef(false);
  const { status, error, loading, refreshToken, syncBusy, syncNow, reload } = useDataSourceStatus();
  useTrayUsage(nowMs, refreshToken);

  const handleNavigate = useCallback((nextPage: PageKey) => {
    setUpdateFocusToken(0);
    setPage(nextPage);
  }, []);

  useEffect(() => {
    latestUpdates.current = updates;
  });

  useEffect(() => {
    if (!trayCheckPending.current || updates.busy) return;
    trayCheckPending.current = false;
    const message = updates.error ?? updates.state?.error;
    if (message) toast.error(`检查更新失败：${message}`);
    else if (updates.state?.phase === "up_to_date") toast.success("当前已是最新版本");
    else if (updates.state?.phase === "disabled")
      toast.info("开发模式不提供应用更新，请使用正式安装版本检查更新");
    else if (updates.state?.phase === "unavailable")
      toast.info("当前环境无法检查更新，请在设置中查看更新状态");
  }, [updates.busy, updates.error, updates.state]);

  const range = useMemo(
    () =>
      selectedRange?.key === "custom"
        ? selectedRange
        : resolveRange(
            selectedRange?.key ?? settings.settings?.defaultRange ?? "today",
            status?.firstEventMs ?? null,
            new Date(nowMs),
          ),
    [selectedRange, settings.settings?.defaultRange, status?.firstEventMs, nowMs],
  );

  const handleSync = useCallback(
    (force = false) => {
      void syncNow(force);
    },
    [syncNow],
  );

  const handleOpenUpdates = useCallback(() => {
    setPage("settings");
    setUpdateFocusToken((token) => token + 1);
  }, []);

  useEffect(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    const attach = (stop: () => void) => {
      if (disposed) stop();
      else stops.push(stop);
    };
    onDesktopEvent("navigate-settings", () => handleNavigate("settings"))
      .then(attach)
      .catch(() => {});
    onTrayNavigate(handleNavigate)
      .then(attach)
      .catch(() => {});
    onDesktopEvent("navigate-updates", () => {
      setPage("settings");
      setUpdateFocusToken((token) => token + 1);
      const current = latestUpdates.current;
      if (
        trayCheckPending.current ||
        current.busy ||
        current.state?.phase === "checking" ||
        current.state?.phase === "downloading" ||
        current.state?.phase === "installing"
      )
        return;
      trayCheckPending.current = true;
      void current.act("check");
    })
      .then(attach)
      .catch(() => {});
    onDesktopError((message) => toast.error(message))
      .then(attach)
      .catch(() => {});
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, [handleNavigate]);

  return (
    <TokenDisplayContext value={settings.settings?.tokenDisplay ?? "auto"}>
      <TooltipProvider delayDuration={200}>
        <AppShell
          page={page}
          sidebarCollapsed={settings.settings?.sidebarCollapsed ?? false}
          sidebarBusy={settings.busy || !settings.settings}
          onSidebarCollapsedChange={(sidebarCollapsed) => {
            void settings.patch({ sidebarCollapsed });
          }}
          collectionPaused={settings.settings?.collectionPaused ?? false}
          range={range}
          nowMs={nowMs}
          status={status}
          syncBusy={syncBusy}
          error={error ?? settings.error}
          onRetry={() => {
            void Promise.all([reload(), settings.reload()]);
          }}
          onNavigate={handleNavigate}
          onRangeChange={setSelectedRange}
          onSync={handleSync}
        >
          {page === "overview" ? (
            <OverviewPage
              range={range}
              nowMs={nowMs}
              refreshToken={refreshToken}
              onNavigate={handleNavigate}
            />
          ) : null}
          {page === "analytics" ? (
            <AnalyticsPage range={range} refreshToken={refreshToken} />
          ) : null}
          {page === "sessions" ? <SessionsPage range={range} refreshToken={refreshToken} /> : null}
          {page === "sources" ? (
            <SourcesPage
              settingsController={settings}
              status={status}
              loading={loading}
              error={error}
              syncBusy={syncBusy}
              refreshToken={refreshToken}
              onRetry={() => {
                void reload();
              }}
              onSync={handleSync}
            />
          ) : null}
          {page === "settings" ? (
            <SettingsPage
              controller={settings}
              updates={updates}
              updateFocusToken={updateFocusToken}
            />
          ) : null}
        </AppShell>
        <UpdateAnnouncement
          state={updates.state}
          currentVersion={settings.appInfo?.version ?? null}
          onOpenUpdates={handleOpenUpdates}
        />
        <Toaster position="bottom-right" theme={settings.settings?.theme ?? "system"} />
        <CloseWindowDialog />
      </TooltipProvider>
    </TokenDisplayContext>
  );
}
