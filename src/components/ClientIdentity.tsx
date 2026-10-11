import { Box, Code2, Terminal } from "lucide-react";

import type { ClientKey } from "@/lib/usage-types";
import { clientLabel } from "@/lib/client-labels";
import { cn } from "@/lib/utils";

const CLIENT_STYLES = {
  codex: { icon: Terminal, className: "bg-client-codex/10 text-client-codex" },
  claude_code: { icon: Code2, className: "bg-client-claude/10 text-client-claude" },
  opencode: { icon: Box, className: "bg-client-opencode/10 text-client-opencode" },
} satisfies Record<ClientKey, { icon: typeof Terminal; className: string }>;

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
        className={cn(
          "flex shrink-0 items-center justify-center rounded-lg",
          compact ? "size-6" : "size-9",
          className,
        )}
      >
        <Icon className={compact ? "size-3.5" : "size-4"} aria-hidden="true" />
      </span>
      <span className="truncate text-sm font-medium">{clientLabel(client)}</span>
    </span>
  );
}
