/**
 * Tauri Command 的前端封装与 DTO 类型。
 *
 * 字段名与 `src-tauri/src/commands/dto.rs` 逐字对应（Rust 侧 `rename_all = "camelCase"`）。
 * 前端不直接访问文件系统或 SQLite：所有数据都来自这些 command。
 */

import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 客户端标识（与 Rust `Client::as_str()` 一致）。 */
export type ClientKey = "codex" | "claude_code" | "opencode";

export type TokenTotals = {
  inputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalTokens: number;
};

export type StatsBucket = {
  sessions: number;
  messages: number;
  /** 完全没有用量数据的会话数（总量未知，未计入求和）。 */
  sessionsWithoutUsage: number;
  /** 请求明细不完整的会话数。 */
  sessionsPartial: number;
  /** 客户端自报总量与其分项之和不一致的会话数（按客户端值计入并标记）。 */
  sessionsTotalMismatch: number;
  totals: TokenTotals;
};

export type OverviewStats = {
  range: StatsBucket;
  today: StatsBucket;
  firstEventMs: number | null;
  lastEventMs: number | null;
};

export type DailyTokenBucket = {
  dayStartMs: number;
  sessions: number;
  messages: number;
  totals: TokenTotals;
};

export type ClientUsage = {
  client: ClientKey;
  /** false 表示该客户端的采集器尚未实现（UI 显示"等待采集"，数值为 0 而非模拟数据）。 */
  supported: boolean;
  sessions: number;
  messages: number;
  totals: TokenTotals;
};

export type RankingEntry = {
  /** null 表示客户端未记录该字段（例如未标注模型）。 */
  key: string | null;
  sessions: number;
  messages: number;
  totalTokens: number;
};

export type Rankings = {
  models: RankingEntry[];
  projects: RankingEntry[];
};

export type MessageCoverage = "complete" | "partial" | "none";
export type TotalBasis = "client_cumulative" | "client_reported" | "sum_of_messages";

export type SessionSummary = {
  client: ClientKey;
  clientSessionId: string;
  model: string | null;
  projectPath: string | null;
  startedAtMs: number | null;
  endedAtMs: number | null;
  inputTokens: number | null;
  outputTokens: number | null;
  reasoningTokens: number | null;
  cacheReadTokens: number | null;
  cacheWriteTokens: number | null;
  /** null 表示该会话没有用量数据（未知，不是 0）。 */
  totalTokens: number | null;
  messages: number;
  coverage: MessageCoverage;
  totalBasis: TotalBasis;
  /** 无法确认的字段关系，例如 `cache_write.relation_to_input`。 */
  unknownRelations: string[];
};

export type ClientSourceStatus = {
  client: ClientKey;
  supported: boolean;
  sessions: number;
  messages: number;
  filesTracked: number;
  unparsedRecords: number;
  lastScanMs: number | null;
};

export type SyncReport = {
  rootMissing: boolean;
  filesSeen: number;
  filesParsed: number;
  filesSkippedUnchanged: number;
  filesWithoutIdentity: number;
  filesFailed: number;
  unparsedLines: number;
  sessionsImported: number;
  sessionsSkippedStale: number;
  messagesInserted: number;
  messagesUpdated: number;
  messagesPruned: number;
};

export type SyncState = {
  running: boolean;
  startedAtMs: number | null;
  finishedAtMs: number | null;
  durationMs: number | null;
  error: string | null;
  report: SyncReport | null;
};

export type DataSourceStatus = {
  clients: ClientSourceStatus[];
  sync: SyncState;
  databasePath: string;
  firstEventMs: number | null;
  lastEventMs: number | null;
  /** 是否有可靠的成本来源；当前恒为 false，UI 不展示金额。 */
  costAvailable: boolean;
};

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

export type RangeArgs = {
  fromMs: number | null;
  toMs: number | null;
};

export function getOverviewStats(
  args: RangeArgs & { todayFromMs: number },
): Promise<OverviewStats> {
  return invoke<OverviewStats>("get_overview_stats", args);
}

export function getDailyTokenTrend(args: {
  fromMs: number;
  dayCount: number;
  client?: ClientKey | null;
}): Promise<DailyTokenBucket[]> {
  return invoke<DailyTokenBucket[]>("get_daily_token_trend", {
    fromMs: args.fromMs,
    dayCount: args.dayCount,
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

export function syncCodex(force = false): Promise<SyncState> {
  return invoke<SyncState>("sync_codex", { force });
}

/** 订阅同步状态变化（开始/结束）。返回取消订阅函数。 */
export function onSyncEvent(handler: (state: SyncState) => void): Promise<UnlistenFn> {
  return listen<SyncState>(SYNC_EVENT, (event) => handler(event.payload));
}
