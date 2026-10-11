import { useCallback, useMemo, useState } from "react";

import { AppShell, type PageKey } from "@/components/AppShell";
import { useDataSourceStatus } from "@/hooks/use-data-source-status";
import { AnalyticsPage } from "@/pages/AnalyticsPage";
import { OverviewPage } from "@/pages/OverviewPage";
import { SessionsPage } from "@/pages/SessionsPage";
import { SourcesPage } from "@/pages/SourcesPage";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { resolveRange, type RangeKey } from "@/lib/ranges";

export default function App() {
  const [page, setPage] = useState<PageKey>("overview");
  const [rangeKey, setRangeKey] = useState<RangeKey>("30d");
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

  return (
    <TooltipProvider delayDuration={200}>
      <AppShell
        page={page}
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
      </AppShell>
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
