import type { ClientKey } from "@/lib/usage-types";
import type { RangeKey } from "@/lib/ranges";

export type CloseBehavior = "ask" | "tray" | "exit";
export type Theme = "system" | "light" | "dark";
export type TokenDisplay = "auto" | "chinese" | "k" | "m" | "b" | "exact";
export type AppSettings = {
  closeBehavior: CloseBehavior;
  autostart: boolean;
  autostartHidden: boolean;
  rememberWindow: boolean;
  sidebarCollapsed: boolean;
  theme: Theme;
  tokenDisplay: TokenDisplay;
  defaultRange: RangeKey;
  syncOnStart: boolean;
  syncIntervalMinutes: number;
  collectionPaused: boolean;
  enabledClients: ClientKey[];
  automaticUpdates: boolean;
};
export type SettingsSnapshot = { settings: AppSettings; warning: string | null };
export type SettingsPatch = Partial<AppSettings>;
export type AppInfo = { name: string; version: string };
export type StorageStatus = {
  directory: string;
  databasePath: string;
  locatorPath: string;
  warning: string | null;
};

export type UpdatePhase =
  | "disabled"
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "downloading"
  | "downloaded"
  | "installing"
  | "unavailable"
  | "failed";
export type UpdateState = {
  phase: UpdatePhase;
  version: string | null;
  notes: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: string | null;
  platform: "windows" | "macos" | "unsupported";
};

/**
 * AI 面板生成（OpenAI 兼容协议）配置与调用 DTO。
 *
 * 默认离线：只有用户点击「测试连接 / 生成面板」才会联网。密钥只经 OS 凭据库，
 * 前端 write-only；`get_llm_config` 仅回读 `hasKey` 状态，永不回显密钥明文。
 */

/** `get_llm_config` 回读结果：密钥只暴露「是否已保存」。 */
export type LlmConfig = {
  baseUrl: string | null;
  model: string | null;
  hasKey: boolean;
};

/** `set_llm_config` 入参：仅本地配置，不含密钥。 */
export type LlmConfigInput = {
  baseUrl: string | null;
  model: string | null;
};

/** `test_llm_connection` 结果：失败信息可读，且绝不包含密钥。 */
export type LlmTestResult = {
  ok: boolean;
  message: string | null;
};

/** `generate_panel_spec` 的可选用量信息（仅用于提示，前端不做统计或展示金额）。 */
export type LlmUsage = {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
};

/** `generate_panel_spec` 结果：`spec` 为未收窄结构，必须再过 `validatePanelSpec`。 */
export type GeneratedPanelResult = {
  spec: unknown;
  usage?: LlmUsage;
};
