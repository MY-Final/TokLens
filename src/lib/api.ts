/**
 * Tauri Command 的调用、错误转换与事件订阅入口。
 *
 * DTO 定义位于 `usage-types.ts`，与 `src-tauri/src/commands/dto.rs` 逐字对应（Rust 侧 `rename_all = "camelCase"`）。
 * 前端不直接访问文件系统或 SQLite：所有数据都来自这些 command。
 */

import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { UpdateState } from "@/lib/settings-types";
import { openUrl } from "@tauri-apps/plugin-opener";

export function getUpdateState(): Promise<UpdateState> {
  return invoke("get_update_state");
}
export function checkUpdates(): Promise<UpdateState> {
  return invoke("check_updates");
}
export function downloadUpdate(): Promise<UpdateState> {
  return invoke("download_update");
}
export function installUpdate(): Promise<void> {
  return invoke("install_update");
}
export function onUpdateState(callback: (state: UpdateState) => void): Promise<UnlistenFn> {
  return listen<UpdateState>("update-state", (event) => callback(event.payload));
}
export function openProjectPage(): Promise<void> {
  return openUrl("https://github.com/MY-Final/TokLens");
}
import type { AppInfo, CloseBehavior, SettingsPatch, SettingsSnapshot } from "@/lib/settings-types";

export function getDesktopStatus(): Promise<{ trayAvailable: boolean }> {
  return invoke("get_desktop_status");
}
export function getClosePending(): Promise<boolean> {
  return invoke("get_close_pending");
}
export function resolveClose(action: CloseBehavior | null, remember: boolean): Promise<void> {
  return invoke("resolve_close", { action, remember });
}
export function onDesktopEvent(
  event: "close-requested" | "navigate-settings" | "settings-changed",
  callback: () => void,
): Promise<UnlistenFn> {
  return listen(event, callback);
}
export function onDesktopError(callback: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("desktop-error", (event) => callback(event.payload));
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

export function getSettings(): Promise<SettingsSnapshot> {
  return invoke<SettingsSnapshot>("get_settings");
}
export function clearStatistics(): Promise<SettingsSnapshot> {
  return invoke("clear_statistics");
}
export function resetSettings(): Promise<SettingsSnapshot> {
  return invoke("reset_settings");
}
export function openAppDataDir(): Promise<void> {
  return invoke("open_app_data_dir");
}

export function updateSettings(patch: SettingsPatch): Promise<SettingsSnapshot> {
  return invoke<SettingsSnapshot>("update_settings", { patch });
}

import type {
  ClientKey,
  ClientUsage,
  DataSourceStatus,
  DailyTokenBucket,
  OverviewStats,
  RangeArgs,
  Rankings,
  SessionSummary,
  SyncState,
} from "@/lib/usage-types";
export type {
  ClientKey,
  TokenTotals,
  StatsBucket,
  OverviewStats,
  DailyTokenBucket,
  ClientUsage,
  RankingEntry,
  Rankings,
  MessageCoverage,
  TotalBasis,
  SessionSummary,
  ClientSourceStatus,
  SyncReport,
  SyncState,
  DataSourceStatus,
  RangeArgs,
} from "@/lib/usage-types";

/** 同步状态广播事件名（与 Rust `commands::SYNC_EVENT` 一致）。 */
export const SYNC_EVENT = "usage-sync";

/** 把 invoke 抛出的错误统一成可展示的消息。 */
export function toErrorMessage(error: unknown): string {
  if (!isTauri()) {
    return "浏览器预览无法读取本地用量，请使用 pnpm tauri dev 启动桌面应用。";
  }
  if (typeof error === "string") {
    return error;
  }
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") {
      return message;
    }
  }
  return "无法连接本地后端（若在浏览器中预览，请使用 pnpm tauri dev 启动桌面应用）。";
}

export function getOverviewStats(
  args: RangeArgs & { todayFromMs: number; client?: ClientKey | null },
): Promise<OverviewStats> {
  return invoke<OverviewStats>("get_overview_stats", { ...args, client: args.client ?? null });
}

export function getDailyTokenTrend(args: {
  fromMs: number;
  dayCount: number;
  dayStartMs?: number;
  toMs?: number;
  client?: ClientKey | null;
}): Promise<DailyTokenBucket[]> {
  return invoke<DailyTokenBucket[]>("get_daily_token_trend", {
    fromMs: args.fromMs,
    dayCount: args.dayCount,
    dayStartMs: args.dayStartMs ?? null,
    toMs: args.toMs ?? null,
    client: args.client ?? null,
  });
}

export function getClientUsage(args: RangeArgs): Promise<ClientUsage[]> {
  return invoke<ClientUsage[]>("get_client_usage", args);
}

export function getRankings(
  args: RangeArgs & { client?: ClientKey | null; limit?: number },
): Promise<Rankings> {
  return invoke<Rankings>("get_rankings", {
    fromMs: args.fromMs,
    toMs: args.toMs,
    client: args.client ?? null,
    limit: args.limit ?? null,
  });
}

export function getSessions(
  args: RangeArgs & { client?: ClientKey | null; limit?: number; offset?: number },
): Promise<SessionSummary[]> {
  return invoke<SessionSummary[]>("get_sessions", {
    fromMs: args.fromMs,
    toMs: args.toMs,
    client: args.client ?? null,
    limit: args.limit ?? null,
    offset: args.offset ?? null,
  });
}

export function getDataSourceStatus(): Promise<DataSourceStatus> {
  return invoke<DataSourceStatus>("get_data_source_status");
}

export function syncUsage(force = false): Promise<SyncState> {
  return invoke<SyncState>("sync_usage", { force });
}

export function syncCodex(force = false): Promise<SyncState> {
  return invoke<SyncState>("sync_codex", { force });
}

/** 订阅同步状态变化（开始/结束）。返回取消订阅函数。 */
export function onSyncEvent(handler: (state: SyncState) => void): Promise<UnlistenFn> {
  return listen<SyncState>(SYNC_EVENT, (event) => handler(event.payload));
}
