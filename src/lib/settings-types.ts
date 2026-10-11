import type { ClientKey } from "@/lib/usage-types";
import type { RangeKey } from "@/lib/ranges";

export type CloseBehavior = "ask" | "tray" | "exit";
export type Theme = "system" | "light" | "dark";
export type AppSettings = {
  closeBehavior: CloseBehavior;
  autostart: boolean;
  autostartHidden: boolean;
  rememberWindow: boolean;
  theme: Theme;
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
