/** 客户端展示名（只做展示，不参与统计）。 */

import type { ClientKey } from "@/lib/usage-types";

export const CLIENT_LABELS: Record<ClientKey, string> = {
  codex: "Codex",
  claude_code: "Claude Code",
  opencode: "OpenCode",
};

export function clientLabel(client: ClientKey): string {
  return CLIENT_LABELS[client];
}
