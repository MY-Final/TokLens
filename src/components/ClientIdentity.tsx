import {
  ClaudeCode,
  Codex,
  GithubCopilot,
  DeepSeek,
  Kimi,
  OpenCode,
  XiaomiMiMo,
} from "@lobehub/icons";
import { Bot, TerminalSquare } from "lucide-react";
import type { ComponentType } from "react";

import type { ClientKey } from "@/lib/usage-types";
import { clientLabel } from "@/lib/client-labels";
import { cn } from "@/lib/utils";

const CLIENT_STYLES = {
  codex: { icon: Codex, className: "bg-client-codex/10 text-client-codex" },
  claude_code: { icon: ClaudeCode, className: "bg-client-claude/10 text-client-claude" },
  opencode: { icon: OpenCode, className: "bg-client-opencode/10 text-client-opencode" },
  dsh: { icon: DeepSeek, className: "bg-client-dsh/10 text-client-dsh" },
  kimi_code: { icon: Kimi, className: "bg-client-kimi/10 text-client-kimi" },
  mimocode: { icon: XiaomiMiMo, className: "bg-client-mimo/10 text-client-mimo" },
  pi_desktop: { icon: TerminalSquare, className: "bg-client-pi/10 text-client-pi" },
  workbuddy: { icon: Bot, className: "bg-client-workbuddy/10 text-client-workbuddy" },
  copilot: { icon: GithubCopilot, className: "bg-client-copilot/10 text-client-copilot" },
} satisfies Record<ClientKey, { icon: ComponentType<{ size: number }>; className: string }>;

export function ClientIdentity({
  client,
  compact = false,
}: {
  client: ClientKey;
  compact?: boolean;
}) {
  const { icon: Icon, className } = CLIENT_STYLES[client];
  return (
    <span className="inline-flex min-w-0 items-center gap-2.5">
      <span
        aria-hidden="true"
        className={cn(
          "flex shrink-0 items-center justify-center rounded-lg",
          compact ? "size-6" : "size-9",
          className,
        )}
      >
        <Icon size={compact ? 14 : 16} />
      </span>
      <span className="truncate text-sm font-medium">{clientLabel(client)}</span>
    </span>
  );
}
