/** 侧边导航 + 顶部工具栏。 */

import type { ReactNode } from "react";
import { BarChart3, Database, LayoutDashboard, ListTree, ShieldCheck } from "lucide-react";

import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
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
import { FilterSelectContent } from "@/components/FilterSelectContent";
import { ErrorBlock } from "@/components/StateBlocks";
import type { DataSourceStatus } from "@/lib/usage-types";
import { formatDateTime, formatNumber } from "@/lib/format";
import { RANGE_OPTIONS, type RangeKey } from "@/lib/ranges";

export type PageKey = "overview" | "analytics" | "sessions" | "sources";

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
};

export function AppShell({
  page,
  rangeKey,
  children,
  status,
  syncBusy,
  onNavigate,
  onRangeChange,
  onSync,
  error,
  onRetry,
}: {
  page: PageKey;
  rangeKey: RangeKey;
  children: ReactNode;
  status: DataSourceStatus | null;
  syncBusy: boolean;
  onNavigate: (page: PageKey) => void;
  onRangeChange: (key: RangeKey) => void;
  onSync: (force?: boolean) => void;
  error: string | null;
  onRetry: () => void;
}) {
  const sync = status?.sync;
  return (
    <SidebarProvider>
      <Sidebar
        collapsible="none"
        className="sticky top-0 h-svh w-48 shrink-0 border-r border-sidebar-border lg:w-56"
      >
        <SidebarHeader className="px-5 py-7">
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
        <SidebarFooter className="gap-3 px-5 py-5">
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <ShieldCheck className="size-3.5" aria-hidden="true" />
            本地优先 · 隐私保护
          </div>
          <span className="font-mono text-xs text-muted-foreground/80">TokLens v0.1.0</span>
        </SidebarFooter>
      </Sidebar>

      <SidebarInset className="min-w-0 bg-background">
        <header className="flex flex-wrap items-center justify-between gap-3 border-b border-border/50 px-6 py-3">
          <span className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className="size-1.5 rounded-full bg-client-codex" aria-hidden="true" />
            本地 AI 工作空间
          </span>
          <SyncControl sync={sync} busy={syncBusy} onSync={onSync} compact />
        </header>
        <div className="mx-auto w-full max-w-[1600px] space-y-5 p-5 lg:p-7">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h1 className="text-xl font-semibold tracking-tight">{PAGE_TITLES[page]}</h1>
              <p className="mt-1 text-xs text-muted-foreground">本地 AI 编程助手使用情况统计</p>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <span className="hidden text-xs text-muted-foreground xl:inline">
                {sync?.report ? `已扫描 ${formatNumber(sync.report.filesSeen)} 个文件 · ` : ""}
                上次同步 {formatDateTime(sync?.finishedAtMs ?? null)}
              </span>
              <Select value={rangeKey} onValueChange={(value) => onRangeChange(value as RangeKey)}>
                <SelectTrigger size="sm" className="w-36" aria-label="选择日期范围">
                  <SelectValue />
                </SelectTrigger>
                <FilterSelectContent>
                  {RANGE_OPTIONS.map((option) => (
                    <SelectItem key={option.key} value={option.key}>
                      {option.label}
                    </SelectItem>
                  ))}
                </FilterSelectContent>
              </Select>
            </div>
          </div>
          {error && page !== "sources" ? <ErrorBlock message={error} onRetry={onRetry} /> : null}
          {children}
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
