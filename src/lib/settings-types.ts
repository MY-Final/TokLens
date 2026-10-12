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
