/**
 * Model-brand classification used only for presentation.
 * The raw model string remains the source of truth and is never rewritten.
 */

export type ModelIconKind =
  | "anthropic"
  | "deepseek"
  | "gemini"
  | "gemma"
  | "glm"
  | "grok"
  | "kimi"
  | "meta"
  | "mimo"
  | "minimax"
  | "mistral"
  | "moonshot"
  | "openai"
  | "qwen"
  | "generic";

type ModelIconRule = {
  kind: Exclude<ModelIconKind, "generic">;
  pattern: RegExp;
};

/**
 * Match provider / model-family names rather than individual model versions.
 * This keeps new versions such as `gpt-5.6-luna` covered without a per-model
 * mapping, while the raw label shown in the UI stays unchanged.
 */
const MODEL_ICON_RULES: readonly ModelIconRule[] = [
  { kind: "anthropic", pattern: /^(anthropic|claude)(?=$|[-_. :]|\d)/i },
  { kind: "deepseek", pattern: /^deepseek(?=$|[-_. :]|\d)/i },
  { kind: "gemini", pattern: /^(gemini|google)(?=$|[-_. :]|\d)/i },
  { kind: "gemma", pattern: /^gemma(?=$|[-_. :]|\d)/i },
  { kind: "glm", pattern: /^(glm|chatglm|zhipu)(?=$|[-_. :]|\d)/i },
  { kind: "grok", pattern: /^grok(?=$|[-_. :]|\d)/i },
  { kind: "kimi", pattern: /^kimi(?=$|[-_. :]|\d)/i },
  { kind: "meta", pattern: /^(llama|meta)(?=$|[-_. :]|\d)/i },
  { kind: "mimo", pattern: /^(mimo|xiaomi)(?=$|[-_. :]|\d)/i },
  { kind: "minimax", pattern: /^(minimax|abab)(?=$|[-_. :]|\d)/i },
  { kind: "mistral", pattern: /^(mistral|mixtral|codestral|devstral|magistral)(?=$|[-_. :]|\d)/i },
  { kind: "moonshot", pattern: /^moonshot(?=$|[-_. :]|\d)/i },
  {
    kind: "openai",
    pattern: /^(openai|gpt|chatgpt|o[1-9]\d*)(?=$|[-_. :]|\d)/i,
  },
  { kind: "qwen", pattern: /^qwen(?=$|[-_. :]|\d)/i },
];

export function modelIconKind(model: string | null): ModelIconKind {
  const value = model?.trim();
  if (!value) return "generic";

  // Prefer the model over its provider, including multi-segment router names.
  for (const segment of value.split("/").reverse()) {
    const rule = MODEL_ICON_RULES.find(({ pattern }) => pattern.test(segment.trim()));
    if (rule) return rule.kind;
  }

  return "generic";
}
