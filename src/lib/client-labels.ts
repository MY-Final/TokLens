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
};

export function clientLabel(client: ClientKey): string {
  return CLIENT_LABELS[client];
}

export const GROK_USAGE_NOTE =
  "Grok CLI 按轮记录用量：请求数表示轮数，均量为每轮 Token，按轮结束时间归属，无法拆分为独立模型调用。";

export function isClientKey(value: string | null): value is ClientKey {
  return value !== null && Object.prototype.hasOwnProperty.call(CLIENT_LABELS, value);
}
