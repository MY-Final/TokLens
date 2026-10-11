/** Cross-platform DTOs. Keep fields aligned with Rust commands/dto.rs. */

/** 客户端标识（与 Rust `Client::as_str()` 一致）。 */
export type ClientKey =
  "codex" | "claude_code" | "opencode" | "dsh" | "kimi_code" | "mimocode" | "pi_desktop";

export type TokenTotals = {
  inputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  /** 已计入规范总量的缓存写入（按各客户端统计口径）。 */
  countedCacheWriteTokens: number;
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
  clients: { client: ClientKey; totalTokens: number }[];
  dayStartMs: number;
  sessions: number;
  messages: number;
  totals: TokenTotals;
};

export type ClientUsage = {
  /** 后端计算的 Token 占比，10000 表示 100%。 */
  shareBps: number;
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
  clients: { client: ClientKey; error: string | null; report: SyncReport | null }[];
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

export type RangeArgs = {
  fromMs: number | null;
  toMs: number | null;
};
