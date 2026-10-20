/** 客户端展示名（只做展示，不参与统计）。 */

import type { ClientKey } from "@/lib/usage-types";

export const CLIENT_LABELS: Record<ClientKey, string> = {
  codex: "Codex",
  claude_code: "Claude Code",
  opencode: "OpenCode",
  dsh: "DSH",
  kimi_code: "Kimi Code",
  mimocode: "MiMoCode",
  pi_desktop: "Pi Desktop",
  workbuddy: "WorkBuddy",
  copilot: "GitHub Copilot",
  gemini_cli: "Gemini CLI",
  grok: "Grok CLI",
  mcode: "MCode",
  cline: "Cline",
};

export function clientLabel(client: ClientKey): string {
  return CLIENT_LABELS[client];
}

export function isClientKey(value: string | null): value is ClientKey {
  return value !== null && Object.prototype.hasOwnProperty.call(CLIENT_LABELS, value);
}

/** Single source of the client set; derived from the exhaustive label record. */
export const CLIENT_KEYS: readonly ClientKey[] = Object.keys(CLIENT_LABELS).filter(isClientKey);

export const GROK_USAGE_NOTE =
  "Grok CLI 按轮记录用量：请求数表示轮数，均量为每轮 Token，按轮结束时间归属，无法拆分为独立模型调用。";

export const CLINE_USAGE_NOTE =
  "Cline 按轮记录用量：一轮对应一条落盘的用量明细，不是单次 HTTP 请求；会话总量取客户端累计值，输入已包含缓存读写。";
