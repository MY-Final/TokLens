/** 侧边导航 + 顶部工具栏。 */

import { useEffect, type CSSProperties, type ReactNode } from "react";
import { BarChart3, Database, LayoutDashboard, ListTree, Settings } from "lucide-react";

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
} from "@/components/ui/sidebar";
import { SyncControl } from "@/components/SyncControl";
import { DateRangePicker } from "@/components/DateRangePicker";
import { ErrorBlock } from "@/components/StateBlocks";
import type { DataSourceStatus } from "@/lib/usage-types";
import type { DateRange } from "@/lib/ranges";
import { cn } from "@/lib/utils";

export type PageKey = "overview" | "analytics" | "sessions" | "sources" | "settings";

const NAV_ITEMS: ReadonlyArray<{ key: PageKey; label: string; icon: typeof LayoutDashboard }> = [
  { key: "overview", label: "总览", icon: LayoutDashboard },
  { key: "analytics", label: "用量分析", icon: BarChart3 },
  { key: "sessions", label: "会话明细", icon: ListTree },
  { key: "sources", label: "数据源管理", icon: Database },
];

const PAGE_TITLES: Record<PageKey, string> = {
  overview: "总览",
  analytics: "用量分析",
  sessions: "会话明细",
  sources: "数据源管理",
  settings: "设置",
};

export function AppShell({
  page,
  collectionPaused,
  range,
  nowMs,
  children,
  status,
  syncBusy,
  onNavigate,
  onRangeChange,
  onSync,
  error,
  onRetry,
  updateControl,
}: {
  page: PageKey;
  collectionPaused: boolean;
  range: DateRange;
  nowMs: number;
  children: ReactNode;
  status: DataSourceStatus | null;
  syncBusy: boolean;
  onNavigate: (page: PageKey) => void;
  onRangeChange: (range: DateRange) => void;
  onSync: (force?: boolean) => void;
  error: string | null;
  onRetry: () => void;
  updateControl?: ReactNode;
}) {
  const sync = status?.sync;
  useEffect(() => {
    const preventSidebarToggle = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "b" || (!event.ctrlKey && !event.metaKey)) return;
      // The navigation stays expanded; prevent the vendor shortcut and cookie write.
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    window.addEventListener("keydown", preventSidebarToggle, true);
    return () => window.removeEventListener("keydown", preventSidebarToggle, true);
  }, []);
  return (
    <SidebarProvider open style={{ "--sidebar-width": "208px" } as CSSProperties}>
      <Sidebar
        collapsible="none"
        className="sticky top-0 h-svh shrink-0 border-r border-sidebar-border"
      >
        <SidebarHeader className="h-16 justify-center px-4">
          <div className="flex items-center gap-2.5">
            <div className="flex size-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <BarChart3 className="size-5" aria-hidden="true" />
            </div>
            <span className="text-lg font-semibold tracking-tight text-foreground">TokLens</span>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup className="px-3">
            <SidebarGroupContent>
              <SidebarMenu>
                {NAV_ITEMS.map((item) => (
                  <SidebarMenuItem key={item.key}>
                    <SidebarMenuButton
                      isActive={page === item.key}
                      onClick={() => onNavigate(item.key)}
                      tooltip={item.label}
                      aria-current={page === item.key ? "page" : undefined}
                      className="h-10 gap-3 rounded-lg px-3 text-muted-foreground transition-colors data-[active=true]:font-medium data-[active=true]:text-sidebar-accent-foreground"
                    >
                      <item.icon className="size-4" aria-hidden="true" />
                      <span>{item.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter className="px-3 py-4">
          {updateControl}
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                tooltip="设置"
                aria-current={page === "settings" ? "page" : undefined}
                className="h-10 gap-3 rounded-lg px-3 text-muted-foreground data-[active=true]:font-medium data-[active=true]:text-sidebar-accent-foreground"
                isActive={page === "settings"}
                onClick={() => onNavigate("settings")}
              >
                <Settings className="size-4" aria-hidden="true" />
                <span>设置</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
      </Sidebar>

      <SidebarInset className="min-w-0 bg-background">
        <header className="grid min-h-16 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 border-b border-border/50 px-3 py-2 lg:gap-3 lg:px-5">
          <div className="flex shrink-0 items-center gap-2">
            <h1 className="truncate text-base font-semibold tracking-tight">{PAGE_TITLES[page]}</h1>
          </div>
          {page !== "settings" && page !== "sources" ? (
            <DateRangePicker
              range={range}
              firstEventMs={status?.firstEventMs ?? null}
              nowMs={nowMs}
              onChange={onRangeChange}
            />
          ) : (
            <div />
          )}
          <div className="flex shrink-0 items-center gap-2">
            <SyncControl
              sync={sync}
              busy={syncBusy}
              onSync={onSync}
              paused={collectionPaused}
              onOpenSettings={() => onNavigate("settings")}
              compact
            />
          </div>
        </header>
        <div
          className={cn(
            "w-full",
            page !== "settings" && "mx-auto max-w-[1600px] space-y-5 p-5 lg:p-7",
          )}
        >
          {error && page !== "sources" && page !== "settings" ? (
            <ErrorBlock message={error} onRetry={onRetry} />
          ) : null}
          {children}
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
