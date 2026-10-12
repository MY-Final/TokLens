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
};

export function clientLabel(client: ClientKey): string {
  return CLIENT_LABELS[client];
}

export function isClientKey(value: string | null): value is ClientKey {
  return value !== null && Object.prototype.hasOwnProperty.call(CLIENT_LABELS, value);
}
