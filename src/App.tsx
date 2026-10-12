import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { onDesktopError, onDesktopEvent, onTrayNavigate } from "@/lib/api";
import { CloseWindowDialog } from "@/components/CloseWindowDialog";
import { UpdateAnnouncement } from "@/components/UpdateAnnouncement";
import { ArrowUpCircle } from "lucide-react";
import { Button } from "@/components/ui/button";

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
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [selectedRange, setSelectedRange] = useState<DateRange | null>(null);
  const nowMs = useLocalClock();
  const settings = useSettings();
  const updates = useUpdates();
  const latestUpdates = useRef(updates);
  const { status, error, loading, refreshToken, syncBusy, syncNow, reload } = useDataSourceStatus();
  useTrayUsage(nowMs, refreshToken);

  const handleNavigate = useCallback((nextPage: PageKey) => {
    setUpdateFocusToken(0);
    setPage(nextPage);
  }, []);

  useEffect(() => {
    latestUpdates.current = updates;
  });

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
        current.busy ||
        current.state?.phase === "checking" ||
        current.state?.phase === "downloading" ||
        current.state?.phase === "installing"
      )
        return;
      if (current.state?.phase === "downloaded") return;
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

  const knownUpdate =
    !!updates.state?.version &&
    ["available", "downloading", "downloaded", "installing", "failed"].includes(
      updates.state.phase,
    );
  const updateLabel =
    updates.operation === "check" || updates.state?.phase === "checking"
      ? "正在检查更新"
      : updates.state?.phase === "downloaded"
        ? "更新已就绪"
        : updates.state?.phase === "downloading"
          ? "正在下载更新"
          : updates.state?.phase === "installing"
            ? "正在安装更新"
            : knownUpdate
              ? `更新 v${updates.state?.version}`
              : "检查更新";

  return (
    <TokenDisplayContext value={settings.settings?.tokenDisplay ?? "auto"}>
      <TooltipProvider delayDuration={200}>
        <AppShell
          page={page}
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
          updateControl={
            <Button
              variant="ghost"
              className="h-10 w-full justify-start gap-3 px-3 text-primary"
              aria-label={updateLabel}
              title={updateLabel}
              onClick={() => {
                setUpdateDialogOpen(true);
                if (
                  updates.state &&
                  !knownUpdate &&
                  !updates.busy &&
                  updates.state?.phase !== "disabled" &&
                  updates.state?.phase !== "checking"
                )
                  void updates.act("check");
              }}
            >
              <ArrowUpCircle className="size-4 shrink-0" aria-hidden="true" />
              <span className="truncate">{updateLabel}</span>
            </Button>
          }
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
          updates={updates}
          currentVersion={settings.appInfo?.version ?? null}
          open={updateDialogOpen}
          onOpenChange={setUpdateDialogOpen}
        />
        <Toaster position="bottom-right" theme={settings.settings?.theme ?? "system"} />
        <CloseWindowDialog />
      </TooltipProvider>
    </TokenDisplayContext>
  );
}
