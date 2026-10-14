/**
 * AI 生成面板对话框：自然语言 → 声明式面板定义。
 *
 * 未配置时禁用生成并引导去设置页；生成结果必须过 `validatePanelSpec` 二次校验，
 * 接受后走既有 `onSave`（先校验再存）。AI 只产出「看什么」，数值仍来自本地查询。
 * 不打印、不持久化 prompt 内容。
 */

import { useEffect, useId, useState } from "react";
import { Sparkles } from "lucide-react";
import { toast } from "sonner";

import { LoadingInline } from "@/components/StateBlocks";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { generatePanelSpec, getLlmConfig, toErrorMessage } from "@/lib/api";
import { clientLabel } from "@/lib/client-labels";
import {
  createPanelId,
  isPanelAiConfigured,
  panelTitle,
  panelTypeLabel,
  resolveTimeBasis,
  validatePanelSpec,
  type PanelPresetKey,
  type PanelSpec,
} from "@/lib/panel-spec";
import { RANGE_PRESETS } from "@/lib/ranges";
import type { LlmConfig } from "@/lib/settings-types";

const PROMPT_PLACEHOLDER = "例如：看最近 7 天每个模型的 Token 排行";

const TIME_BASIS_LABELS: Record<string, string> = {
  session: "按会话开始日",
  request: "按请求时间",
};

const OPTION_LABELS: Record<string, string> = {
  metric: "指标",
  dimension: "维度",
  limit: "数量上限",
  sort: "排序",
  requestTime: "按请求时间",
  stack: "趋势模式",
  field: "指标字段",
  shortenKeys: "缩短项目路径",
  search: "搜索",
  model: "模型",
  projectPath: "项目路径",
  fields: "字段",
};

const OPTION_VALUE_LABELS: Record<string, Record<string, string>> = {
  metric: { tokens: "Token", requests: "请求" },
  dimension: { model: "模型", project: "项目" },
  stack: { client: "客户端对比", tokens: "Token 构成" },
  sort: { newest: "最新", oldest: "最早", tokens_desc: "Token 降序", requests_desc: "请求降序" },
  field: {
    rangeTotal: "区间 Token",
    todayTotal: "今日 Token",
    sessions: "会话数量",
    messages: "请求明细",
  },
};

const PRESET_LABELS = new Map<string, string>(
  RANGE_PRESETS.map((preset) => [preset.key, preset.label]),
);

function describeRange(spec: PanelSpec): string {
  const range = spec.query.range;
  if (range.kind === "followPage") return "跟随页面日期";
  if (range.kind === "preset") {
    return PRESET_LABELS.get(range.key as PanelPresetKey) ?? range.key;
  }
  const format = (ms: number) => new Date(ms).toLocaleString();
  return `${format(range.fromMs)} 至 ${format(range.toMs)}`;
}

function describeOptionValue(name: string, value: unknown): string {
  if (typeof value === "boolean") return value ? "是" : "否";
  if (typeof value === "string") return OPTION_VALUE_LABELS[name]?.[value] ?? value;
  if (typeof value === "number") return String(value);
  if (Array.isArray(value)) return value.join("、");
  return String(value);
}

function previewRows(spec: PanelSpec): { label: string; value: string }[] {
  const rows: { label: string; value: string }[] = [
    { label: "标题", value: panelTitle(spec) },
    { label: "类型", value: panelTypeLabel(spec.type) },
    { label: "时间范围", value: describeRange(spec) },
    { label: "口径", value: TIME_BASIS_LABELS[resolveTimeBasis(spec)] ?? resolveTimeBasis(spec) },
    { label: "客户端", value: spec.query.client ? clientLabel(spec.query.client) : "全部客户端" },
  ];
  for (const [name, value] of Object.entries(spec.typeOptions)) {
    if (value === undefined || value === null) continue;
    rows.push({ label: OPTION_LABELS[name] ?? name, value: describeOptionValue(name, value) });
  }
  return rows;
}

function formatErrors(errors: { path: string; message: string }[]): string[] {
  return errors.map((error) => `${error.path || "面板"}：${error.message}`);
}

/** 对话框内容：每次打开时重新挂载，天然获得干净的初始状态。 */
function AiPanelDialogBody({
  onOpenChange,
  onSave,
  onOpenSettings,
}: {
  onOpenChange: (open: boolean) => void;
  onSave: (spec: PanelSpec) => Promise<void>;
  onOpenSettings: () => void;
}) {
  const [config, setConfig] = useState<LlmConfig | null>(null);
  const [configLoading, setConfigLoading] = useState(true);
  const [configError, setConfigError] = useState<string | null>(null);
  const [prompt, setPrompt] = useState("");
  const [generating, setGenerating] = useState(false);
  const [result, setResult] = useState<PanelSpec | null>(null);
  const [genError, setGenError] = useState<string | null>(null);
  const [validationErrors, setValidationErrors] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const promptId = useId();

  useEffect(() => {
    let active = true;
    getLlmConfig()
      .then((next) => {
        if (active) setConfig(next);
      })
      .catch((cause: unknown) => {
        if (active) setConfigError(toErrorMessage(cause));
      })
      .finally(() => {
        if (active) setConfigLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const configured = isPanelAiConfigured(config);
  const busy = generating || saving;

  const generate = async () => {
    const text = prompt.trim();
    if (!text || busy || !configured) return;
    setGenerating(true);
    setGenError(null);
    setValidationErrors([]);
    setResult(null);
    try {
      const { spec } = await generatePanelSpec(text);
      const checked = validatePanelSpec(spec);
      if (checked.ok) setResult(checked.spec);
      else setValidationErrors(formatErrors(checked.errors));
    } catch (cause: unknown) {
      setGenError(toErrorMessage(cause));
    } finally {
      setGenerating(false);
    }
  };

  const accept = async () => {
    if (!result || busy) return;
    setSaving(true);
    setGenError(null);
    setValidationErrors([]);
    try {
      const candidate: PanelSpec = { ...result, id: createPanelId(), source: { origin: "ai" } };
      const checked = validatePanelSpec(candidate);
      if (!checked.ok) {
        setValidationErrors(formatErrors(checked.errors));
        return;
      }
      await onSave(checked.spec);
      toast.success("AI 面板已添加到自定义面板");
      onOpenChange(false);
    } catch (cause: unknown) {
      setGenError(toErrorMessage(cause));
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>AI 生成面板</DialogTitle>
        <DialogDescription>
          用自然语言描述想看的维度，AI
          只生成「看什么」的面板定义，数值仍全部来自本地查询；不会读取会话正文。
        </DialogDescription>
      </DialogHeader>

      {configLoading ? (
        <LoadingInline label="正在读取配置…" />
      ) : configError ? (
        <p role="alert" className="text-sm text-destructive">
          {configError}
        </p>
      ) : !configured ? (
        <div className="space-y-3 rounded-lg border border-border bg-muted/40 p-4">
          <p className="text-sm">
            AI 面板生成默认关闭，尚未配置 Base URL、模型或 API Key，因此不会联网。
          </p>
          <p className="text-xs leading-relaxed text-muted-foreground">
            请在设置中填写 OpenAI 兼容接口地址、模型与 API Key（密钥存于系统凭据库），之后即可生成。
          </p>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              onOpenChange(false);
              onOpenSettings();
            }}
          >
            前往设置
          </Button>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="space-y-1.5">
            <label htmlFor={promptId} className="text-xs font-medium text-muted-foreground">
              面板描述
            </label>
            <textarea
              id={promptId}
              value={prompt}
              disabled={busy}
              rows={3}
              maxLength={500}
              placeholder={PROMPT_PLACEHOLDER}
              onChange={(event) => {
                setPrompt(event.target.value);
                setGenError(null);
                setValidationErrors([]);
              }}
              className="w-full min-w-0 resize-y rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs transition-[color,box-shadow] outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-input/30"
            />
          </div>

          {generating ? <LoadingInline label="正在生成面板…" /> : null}

          {genError ? (
            <div className="space-y-2 rounded-md border border-destructive/40 bg-destructive/5 p-3">
              <p role="alert" className="text-sm text-destructive">
                {genError}
              </p>
              <Button size="sm" variant="outline" disabled={busy} onClick={() => void generate()}>
                重新生成
              </Button>
            </div>
          ) : null}

          {validationErrors.length > 0 ? (
            <div className="space-y-2 rounded-md border border-destructive/40 bg-destructive/5 p-3">
              <p className="text-sm text-destructive">AI 返回的面板定义未通过校验，已被拒绝：</p>
              <ul className="list-inside list-disc space-y-1 text-xs text-destructive">
                {validationErrors.map((error) => (
                  <li key={error}>{error}</li>
                ))}
              </ul>
              <Button size="sm" variant="outline" disabled={busy} onClick={() => void generate()}>
                重新生成
              </Button>
            </div>
          ) : null}

          {result ? (
            <div className="space-y-3">
              <div className="rounded-lg border border-border bg-card p-4">
                <div className="mb-2 flex items-center gap-2 text-xs font-medium text-muted-foreground">
                  <Sparkles className="size-3.5" aria-hidden="true" />
                  预览
                </div>
                <dl className="grid grid-cols-1 gap-x-6 gap-y-2 sm:grid-cols-2">
                  {previewRows(result).map((row) => (
                    <div key={row.label} className="flex items-baseline justify-between gap-3">
                      <dt className="shrink-0 text-xs text-muted-foreground">{row.label}</dt>
                      <dd className="min-w-0 truncate text-right text-sm" title={row.value}>
                        {row.value}
                      </dd>
                    </div>
                  ))}
                </dl>
              </div>
              <p className="text-xs leading-relaxed text-muted-foreground">
                数值仍来自本地查询，AI 不参与取数；接受后可随时在面板中编辑或删除。
              </p>
            </div>
          ) : null}
        </div>
      )}

      <DialogFooter>
        <Button variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>
          取消
        </Button>
        {configured ? (
          result ? (
            <>
              <Button variant="outline" disabled={busy} onClick={() => void generate()}>
                重新生成
              </Button>
              <Button disabled={busy} onClick={() => void accept()}>
                {saving ? "添加中…" : "生成面板"}
              </Button>
            </>
          ) : (
            <Button disabled={busy || prompt.trim().length === 0} onClick={() => void generate()}>
              {generating ? "生成中…" : "生成"}
            </Button>
          )
        ) : null}
      </DialogFooter>
    </>
  );
}

export function AiPanelDialog({
  open,
  onOpenChange,
  onSave,
  onOpenSettings,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: (spec: PanelSpec) => Promise<void>;
  onOpenSettings: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <AiPanelDialogBody
          onOpenChange={onOpenChange}
          onSave={onSave}
          onOpenSettings={onOpenSettings}
        />
      </DialogContent>
    </Dialog>
  );
}
