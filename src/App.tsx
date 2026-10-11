import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { onDesktopError, onDesktopEvent } from "@/lib/api";
import { CloseWindowDialog } from "@/components/CloseWindowDialog";

import { AppShell, type PageKey } from "@/components/AppShell";
import { useDataSourceStatus } from "@/hooks/use-data-source-status";
import { AnalyticsPage } from "@/pages/AnalyticsPage";
import { OverviewPage } from "@/pages/OverviewPage";
import { SessionsPage } from "@/pages/SessionsPage";
import { SourcesPage } from "@/pages/SourcesPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { useSettings } from "@/hooks/use-settings";
import { useUpdates } from "@/hooks/use-updates";
import { useLocalClock } from "@/hooks/use-local-clock";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { resolveRange, type DateRange } from "@/lib/ranges";

export default function App() {
  const [page, setPage] = useState<PageKey>("overview");
  const [selectedRange, setSelectedRange] = useState<DateRange | null>(null);
  const nowMs = useLocalClock();
  const settings = useSettings();
  const updates = useUpdates();
  const { status, error, loading, refreshToken, syncBusy, syncNow, reload } = useDataSourceStatus();

  const range = useMemo(
    () =>
      selectedRange?.key === "custom"
        ? selectedRange
        : resolveRange(
            selectedRange?.key ?? settings.settings?.defaultRange ?? "30d",
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

  useEffect(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    const attach = (stop: () => void) => {
      if (disposed) stop();
      else stops.push(stop);
    };
    onDesktopEvent("navigate-settings", () => setPage("settings"))
      .then(attach)
      .catch(() => {});
    onDesktopError((message) => toast.error(message))
      .then(attach)
      .catch(() => {});
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, []);

  return (
    <TooltipProvider delayDuration={200}>
      <AppShell
        page={page}
        version={settings.appInfo?.version ?? null}
        collectionPaused={settings.settings?.collectionPaused ?? false}
        range={range}
        nowMs={nowMs}
        status={status}
        syncBusy={syncBusy}
        error={error}
        onRetry={() => {
          void reload();
        }}
        onNavigate={setPage}
        onRangeChange={setSelectedRange}
        onSync={handleSync}
      >
        {page === "overview" ? (
          <OverviewPage
            range={range}
            nowMs={nowMs}
            refreshToken={refreshToken}
            onNavigate={setPage}
          />
        ) : null}
        {page === "analytics" ? <AnalyticsPage range={range} refreshToken={refreshToken} /> : null}
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
        {page === "settings" ? <SettingsPage controller={settings} updates={updates} /> : null}
      </AppShell>
      <Toaster position="bottom-right" theme={settings.settings?.theme ?? "system"} />
      <CloseWindowDialog />
    </TooltipProvider>
  );
}
