/**
 * 面板编辑表单：由 `get_panel_catalog()` 驱动的合法控件与范围（目录不可用时用内置白名单）。
 * 保存前过 `validatePanelSpec`，非法定义不提交（见 docs/generative-panel-spec.md ⑦）。
 */

import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { FilterSelectContent } from "@/components/FilterSelectContent";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { toErrorMessage } from "@/lib/api";
import { CLIENT_KEYS, clientLabel } from "@/lib/client-labels";
import { formatDateTimeInput, parseDateTimeInput, RANGE_PRESETS } from "@/lib/ranges";
import {
  createDefaultPanelSpec,
  defaultTimeBasis,
  defaultTypeOptions,
  FALLBACK_CATALOG,
  PANEL_COL_SPANS,
  PANEL_TYPES,
  panelTypeLabel,
  validatePanelSpec,
  type PanelCatalog,
  type PanelCatalogParam,
  type PanelColSpan,
  type PanelPresetKey,
  type PanelSpec,
  type PanelType,
  type PanelTypeOptions,
} from "@/lib/panel-spec";

const ALL_CLIENTS = "__all__";

const PARAM_LABELS: Record<string, string> = {
  field: "指标字段",
  stack: "趋势模式",
  metric: "指标",
  dimension: "维度",
  limit: "数量上限",
  shortenKeys: "缩短项目路径",
  sort: "排序",
  requestTime: "按请求时间",
  search: "搜索",
  model: "模型",
  projectPath: "项目路径",
};

const VALUE_LABELS: Record<string, Record<string, string>> = {
  field: {
    rangeTotal: "区间 Token",
    todayTotal: "今日 Token",
    sessions: "会话数量",
    messages: "请求明细",
  },
  stack: { client: "客户端对比", tokens: "Token 构成" },
  metric: { tokens: "Token", requests: "请求" },
  dimension: { model: "模型", project: "项目" },
  sort: { newest: "最新", oldest: "最早", tokens_desc: "Token 降序", requests_desc: "请求降序" },
};

const TIME_BASIS_LABELS: Record<string, string> = {
  session: "按会话开始日",
  request: "按请求时间",
};

const COL_SPAN_LABELS: Record<number, string> = {
  3: "1/4 宽",
  4: "1/3 宽",
  6: "半宽",
  12: "整行",
};

function paramLabel(param: PanelCatalogParam): string {
  return PARAM_LABELS[param.name] ?? param.name;
}

function valueLabel(name: string, value: string): string {
  return VALUE_LABELS[name]?.[value] ?? value;
}

function Field({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor?: string;
  children: ReactNode;
}) {
  return (
    <div className="min-w-0 space-y-1.5">
      <label htmlFor={htmlFor} className="text-xs font-medium text-muted-foreground">
        {label}
      </label>
      {children}
    </div>
  );
}

function SelectField({
  id,
  label,
  value,
  options,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  options: ReadonlyArray<{ value: string; label: string }>;
  disabled?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <Field label={label} htmlFor={id}>
      <Select value={value} onValueChange={onChange} disabled={disabled}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue />
        </SelectTrigger>
        <FilterSelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </FilterSelectContent>
      </Select>
    </Field>
  );
}

export function PanelEditor({
  open,
  onOpenChange,
  catalog,
  initial,
  onSave,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  catalog: PanelCatalog | null;
  initial: PanelSpec | null;
  onSave: (spec: PanelSpec) => Promise<void>;
}) {
  const effectiveCatalog = catalog ?? FALLBACK_CATALOG;
  const catalogRef = useRef(effectiveCatalog);
  const [draft, setDraft] = useState<PanelSpec | null>(null);
  const [customFrom, setCustomFrom] = useState("");
  const [customTo, setCustomTo] = useState("");
  const [errors, setErrors] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const basisId = useId();

  useEffect(() => {
    catalogRef.current = effectiveCatalog;
  });

  useEffect(() => {
    if (!open) return;
    const next = initial ?? createDefaultPanelSpec(catalogRef.current, "stat", 0);
    setDraft(next);
    setErrors([]);
    setSaveError(null);
    if (next.query.range.kind === "custom") {
      setCustomFrom(formatDateTimeInput(next.query.range.fromMs));
      setCustomTo(formatDateTimeInput(next.query.range.toMs));
    }
  }, [open, initial]);

  if (!open || !draft) return null;

  const typeInfo = effectiveCatalog.panelTypes.find((entry) => entry.type === draft.type);
  const fields = typeInfo?.fields ?? [];
  // 只渲染属于 typeOptions 的参数（目录适配层已过滤，此处为纵深防御）。
  const params = (typeInfo?.params ?? []).filter((param) => fields.includes(param.name));
  const timeBases = typeInfo?.timeBases ?? [defaultTimeBasis(draft.type)];
  const colSpans = effectiveCatalog.globalLimits.colSpans.filter((span): span is PanelColSpan =>
    (PANEL_COL_SPANS as readonly number[]).includes(span),
  );
  const basis = draft.query.timeBasis ?? defaultTimeBasis(draft.type);
  const client = draft.query.client ?? ALL_CLIENTS;
  const options = draft.typeOptions as Record<string, unknown>;

  const patchDraft = (patch: Partial<PanelSpec>) =>
    setDraft((current) => (current ? { ...current, ...patch } : current));
  const patchQuery = (patch: Partial<PanelSpec["query"]>) =>
    setDraft((current) =>
      current ? { ...current, query: { ...current.query, ...patch } } : current,
    );
  const patchLayout = (patch: Partial<PanelSpec["layout"]>) =>
    setDraft((current) =>
      current ? { ...current, layout: { ...current.layout, ...patch } } : current,
    );
  const patchOption = (name: string, value: unknown) =>
    setDraft((current) =>
      current
        ? {
            ...current,
            typeOptions: {
              ...(current.typeOptions as Record<string, unknown>),
              [name]: value,
            } as PanelTypeOptions,
          }
        : current,
    );

  const changeType = (type: PanelType) => {
    const info = effectiveCatalog.panelTypes.find((entry) => entry.type === type);
    setDraft((current) =>
      current
        ? {
            ...current,
            type,
            typeOptions: defaultTypeOptions(type),
            query: {
              ...current.query,
              timeBasis: info?.defaultTimeBasis ?? defaultTimeBasis(type),
            },
          }
        : current,
    );
  };

  const changeRangeKind = (kind: string) => {
    if (kind === "followPage") patchQuery({ range: { kind: "followPage" } });
    else if (kind === "preset") patchQuery({ range: { kind: "preset", key: "7d" } });
    else {
      const now = Date.now();
      const from = now - 7 * 24 * 60 * 60 * 1000;
      setCustomFrom(formatDateTimeInput(from));
      setCustomTo(formatDateTimeInput(now));
      patchQuery({ range: { kind: "custom", fromMs: from, toMs: now } });
    }
  };

  const submit = async () => {
    setSaveError(null);
    const next: PanelSpec = { ...draft };
    if (next.query.range.kind === "custom") {
      const fromMs = parseDateTimeInput(customFrom);
      const toMs = parseDateTimeInput(customTo);
      if (fromMs === null || toMs === null) {
        setErrors(["请输入有效的自定义起止时间"]);
        return;
      }
      next.query = { ...next.query, range: { kind: "custom", fromMs, toMs } };
    }
    const result = validatePanelSpec(next);
    if (!result.ok) {
      setErrors(result.errors.map((error) => `${error.path || "面板"}：${error.message}`));
      return;
    }
    setSaving(true);
    try {
      await onSave(result.spec);
      onOpenChange(false);
    } catch (cause: unknown) {
      setSaveError(toErrorMessage(cause));
    } finally {
      setSaving(false);
    }
  };

  const renderParam = (param: PanelCatalogParam) => {
    const id = `panel-param-${param.name}`;
    const value = options[param.name];
    if (param.kind === "select") {
      const values = param.values ?? [];
      return (
        <SelectField
          key={param.name}
          id={id}
          label={paramLabel(param)}
          value={typeof value === "string" ? value : (values[0] ?? "")}
          options={values.map((entry) => ({ value: entry, label: valueLabel(param.name, entry) }))}
          onChange={(next) => patchOption(param.name, next)}
        />
      );
    }
    if (param.kind === "boolean") {
      return (
        <Field key={param.name} label={paramLabel(param)} htmlFor={id}>
          <div className="flex h-9 items-center">
            <Switch
              id={id}
              aria-label={paramLabel(param)}
              checked={value === true}
              onCheckedChange={(checked) => patchOption(param.name, checked)}
            />
          </div>
        </Field>
      );
    }
    if (param.kind === "number") {
      return (
        <Field key={param.name} label={paramLabel(param)} htmlFor={id}>
          <Input
            id={id}
            type="number"
            min={param.min ?? undefined}
            max={param.max ?? undefined}
            value={typeof value === "number" ? String(value) : ""}
            onChange={(event) => {
              const raw = event.target.value;
              patchOption(param.name, raw === "" ? undefined : Number(raw));
            }}
          />
        </Field>
      );
    }
    return (
      <Field key={param.name} label={paramLabel(param)} htmlFor={id}>
        <Input
          id={id}
          type="text"
          maxLength={param.max ?? undefined}
          value={typeof value === "string" ? value : ""}
          placeholder="留空表示不限"
          onChange={(event) => {
            const raw = event.target.value;
            patchOption(param.name, raw === "" ? null : raw);
          }}
        />
      </Field>
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{initial ? "编辑面板" : "新增面板"}</DialogTitle>
          <DialogDescription>
            面板只保存"看什么"的定义，数值仍全部来自本地查询；不写入会话正文。
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <SelectField
              id="panel-type"
              label="类型"
              value={draft.type}
              options={PANEL_TYPES.map((type) => ({ value: type, label: panelTypeLabel(type) }))}
              onChange={(value) => changeType(value as PanelType)}
            />
            <Field label="宽度" htmlFor="panel-span">
              <Select
                value={String(draft.layout.colSpan)}
                onValueChange={(value) => patchLayout({ colSpan: Number(value) as PanelColSpan })}
              >
                <SelectTrigger id="panel-span" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <FilterSelectContent>
                  {colSpans.map((span) => (
                    <SelectItem key={span} value={String(span)}>
                      {COL_SPAN_LABELS[span] ?? String(span)}
                    </SelectItem>
                  ))}
                </FilterSelectContent>
              </Select>
            </Field>
          </div>

          <Field label="标题" htmlFor="panel-title">
            <Input
              id="panel-title"
              type="text"
              maxLength={effectiveCatalog.globalLimits.titleMaxLen}
              value={draft.title ?? ""}
              placeholder="留空使用类型默认名"
              onChange={(event) => patchDraft({ title: event.target.value })}
            />
          </Field>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <SelectField
              id="panel-range-kind"
              label="时间范围"
              value={draft.query.range.kind}
              options={[
                { value: "followPage", label: "跟随页面" },
                { value: "preset", label: "预设" },
                { value: "custom", label: "自定义" },
              ]}
              onChange={changeRangeKind}
            />
            {draft.query.range.kind === "preset" ? (
              <SelectField
                id="panel-preset"
                label="预设范围"
                value={draft.query.range.key}
                options={RANGE_PRESETS.map((preset) => ({
                  value: preset.key,
                  label: preset.label,
                }))}
                onChange={(value) =>
                  patchQuery({ range: { kind: "preset", key: value as PanelPresetKey } })
                }
              />
            ) : null}
            {timeBases.length > 1 ? (
              <SelectField
                id={basisId}
                label="时间口径"
                value={basis}
                options={timeBases.map((entry) => ({
                  value: entry,
                  label: TIME_BASIS_LABELS[entry] ?? entry,
                }))}
                onChange={(value) => patchQuery({ timeBasis: value as "session" | "request" })}
              />
            ) : null}
            <SelectField
              id="panel-client"
              label="客户端"
              value={client}
              options={[
                { value: ALL_CLIENTS, label: "全部客户端" },
                ...CLIENT_KEYS.map((key) => ({ value: key, label: clientLabel(key) })),
              ]}
              onChange={(value) =>
                patchQuery({
                  client: value === ALL_CLIENTS ? null : (value as PanelSpec["query"]["client"]),
                })
              }
            />
          </div>

          {draft.query.range.kind === "custom" ? (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field label="起始时间" htmlFor="panel-from">
                <Input
                  id="panel-from"
                  type="datetime-local"
                  step={60}
                  value={customFrom}
                  onChange={(event) => setCustomFrom(event.target.value)}
                />
              </Field>
              <Field label="结束时间" htmlFor="panel-to">
                <Input
                  id="panel-to"
                  type="datetime-local"
                  step={60}
                  value={customTo}
                  onChange={(event) => setCustomTo(event.target.value)}
                />
              </Field>
            </div>
          ) : null}

          {params.length > 0 ? (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">{params.map(renderParam)}</div>
          ) : null}

          {catalog === null ? (
            <p className="text-xs text-muted-foreground">
              目录暂不可用，已使用内置白名单；保存时仍会通过前端校验。
            </p>
          ) : null}
          {errors.length > 0 ? (
            <ul className="list-inside list-disc space-y-1 text-xs text-destructive" role="alert">
              {errors.map((error) => (
                <li key={error}>{error}</li>
              ))}
            </ul>
          ) : null}
          {saveError ? <p className="text-xs text-destructive">{saveError}</p> : null}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            取消
          </Button>
          <Button onClick={() => void submit()} disabled={saving}>
            保存
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
