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
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { resolveRange, type RangeKey } from "@/lib/ranges";

export default function App() {
  const [page, setPage] = useState<PageKey>("overview");
  const [selectedRange, setRangeKey] = useState<RangeKey | null>(null);
  const settings = useSettings();
  const rangeKey = selectedRange ?? settings.settings?.defaultRange ?? "30d";
  const { status, error, loading, refreshToken, syncBusy, syncNow, reload } = useDataSourceStatus();

  const range = useMemo(
    () => resolveRange(rangeKey, status?.firstEventMs ?? null),
    [rangeKey, status?.firstEventMs],
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
        rangeKey={rangeKey}
        status={status}
        syncBusy={syncBusy}
        error={error}
        onRetry={() => {
          void reload();
        }}
        onNavigate={setPage}
        onRangeChange={setRangeKey}
        onSync={handleSync}
      >
        {page === "overview" ? (
          <OverviewPage range={range} refreshToken={refreshToken} onNavigate={setPage} />
        ) : null}
        {page === "analytics" ? <AnalyticsPage range={range} refreshToken={refreshToken} /> : null}
        {page === "sessions" ? <SessionsPage range={range} refreshToken={refreshToken} /> : null}
        {page === "sources" ? (
          <SourcesPage
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
        {page === "settings" ? <SettingsPage controller={settings} /> : null}
      </AppShell>
      <Toaster position="bottom-right" />
      <CloseWindowDialog />
    </TooltipProvider>
  );
}
