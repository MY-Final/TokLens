import {
  Claude,
  DeepSeek,
  Gemini,
  Gemma,
  Grok,
  Kimi,
  Meta,
  Minimax,
  Mistral,
  Moonshot,
  OpenAI,
  Qwen,
  XiaomiMiMo,
  Zhipu,
} from "@lobehub/icons";
import { Cpu } from "lucide-react";
import type { ComponentType } from "react";

import { modelIconKind, type ModelIconKind } from "@/lib/model-identity";
import { cn } from "@/lib/utils";

type ModelIcon = ComponentType<{ size: number }>;

const MODEL_ICONS: Record<ModelIconKind, ModelIcon> = {
  anthropic: Claude,
  deepseek: DeepSeek,
  gemini: Gemini,
  gemma: Gemma,
  glm: Zhipu,
  grok: Grok,
  kimi: Kimi,
  meta: Meta,
  mimo: XiaomiMiMo,
  minimax: Minimax,
  mistral: Mistral,
  moonshot: Moonshot,
  openai: OpenAI,
  qwen: Qwen,
  generic: Cpu,
};

export function ModelIdentity({
  model,
  compact = false,
  truncate = false,
  unknownLabel = "未知",
  className,
  labelClassName,
  iconClassName,
}: {
  model: string | null;
  compact?: boolean;
  truncate?: boolean;
  unknownLabel?: string;
  className?: string;
  labelClassName?: string;
  iconClassName?: string;
}) {
  const kind = modelIconKind(model);
  const Icon = MODEL_ICONS[kind];
  const isUnknown = model === null || model.trim().length === 0;
  const label = isUnknown ? unknownLabel : model;

  return (
    <span className={cn("inline-flex max-w-full min-w-0 items-center gap-2", className)}>
      <span
        aria-hidden="true"
        className={cn(
          "flex shrink-0 items-center justify-center rounded-md bg-muted/70 text-muted-foreground",
          compact ? "size-6" : "size-8",
          iconClassName,
        )}
      >
        <Icon size={compact ? 14 : 16} />
      </span>
      <span
        className={cn(
          "min-w-0 text-sm",
          truncate ? "truncate" : "break-all whitespace-normal",
          isUnknown && "text-muted-foreground",
          labelClassName,
        )}
        title={isUnknown ? "模型未记录" : model}
      >
        {label}
      </span>
    </span>
  );
}
