/**
 * AI 面板生成设置：OpenAI 兼容协议（Base URL / 模型 / API Key）。
 *
 * 默认离线，只有用户点击「测试连接」才会立即联网；API Key 只写不读，
 * 存于系统凭据库，前端只显示「已保存 / 未保存」状态，永不回显明文。
 */

import { useEffect, useId, useState, type ReactNode } from "react";
import { toast } from "sonner";

import { SettingsSection } from "@/components/SettingsFields";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  clearLlmCredential,
  getLlmConfig,
  setLlmConfig,
  setLlmCredential,
  testLlmConnection,
  toErrorMessage,
} from "@/lib/api";
import type { LlmConfig, LlmTestResult } from "@/lib/settings-types";

function Field({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <label htmlFor={htmlFor} className="text-xs font-medium text-muted-foreground">
        {label}
      </label>
      {children}
      {hint ? <p className="text-xs leading-relaxed text-muted-foreground">{hint}</p> : null}
    </div>
  );
}

export function AiPanelSettings() {
  const [config, setConfig] = useState<LlmConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [keyInput, setKeyInput] = useState("");
  const [savingConfig, setSavingConfig] = useState(false);
  const [configMessage, setConfigMessage] = useState<string | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [savingKey, setSavingKey] = useState(false);
  const [clearingKey, setClearingKey] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<LlmTestResult | null>(null);
  const baseUrlId = useId();
  const modelId = useId();
  const keyId = useId();

  const applyConfig = (next: LlmConfig) => {
    setConfig(next);
    setBaseUrl(next.baseUrl ?? "");
    setModel(next.model ?? "");
  };

  useEffect(() => {
    let active = true;
    getLlmConfig()
      .then((next) => {
        if (!active) return;
        applyConfig(next);
        setLoadError(null);
      })
      .catch((cause: unknown) => {
        if (active) setLoadError(toErrorMessage(cause));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const busy = savingConfig || savingKey || clearingKey || testing;

  const saveConfig = async () => {
    if (busy) return;
    setSavingConfig(true);
    setConfigMessage(null);
    setConfigError(null);
    try {
      await setLlmConfig({ baseUrl: baseUrl.trim() || null, model: model.trim() || null });
      applyConfig(await getLlmConfig());
      setConfigMessage("配置已保存");
    } catch (cause: unknown) {
      setConfigError(toErrorMessage(cause));
    } finally {
      setSavingConfig(false);
    }
  };

  const saveKey = async () => {
    if (busy) return;
    const key = keyInput.trim();
    if (!key) {
      setKeyError("请输入 API Key");
      return;
    }
    setSavingKey(true);
    setKeyError(null);
    try {
      await setLlmCredential(key);
      setKeyInput("");
      applyConfig(await getLlmConfig());
      setTestResult(null);
      toast.success("API Key 已保存到系统凭据库");
    } catch (cause: unknown) {
      setKeyError(toErrorMessage(cause));
    } finally {
      setSavingKey(false);
    }
  };

  const clearKey = async () => {
    if (busy) return;
    setClearingKey(true);
    setKeyError(null);
    try {
      await clearLlmCredential();
      applyConfig(await getLlmConfig());
      setTestResult(null);
      toast.success("API Key 已清除");
    } catch (cause: unknown) {
      setKeyError(toErrorMessage(cause));
    } finally {
      setClearingKey(false);
    }
  };

  const test = async () => {
    if (busy) return;
    setTesting(true);
    setTestResult(null);
    try {
      setTestResult(await testLlmConnection());
    } catch (cause: unknown) {
      setTestResult({ ok: false, message: toErrorMessage(cause) });
    } finally {
      setTesting(false);
    }
  };

  const hasKey = config?.hasKey ?? false;
  const canTest =
    !loading && !busy && hasKey && baseUrl.trim().length > 0 && model.trim().length > 0;

  return (
    <SettingsSection
      title="AI 面板生成"
      description="默认关闭。只有你在自定义页点击「AI 生成」或在此点击「测试连接」时才会联网，使用 OpenAI 兼容接口（如 DeepSeek、中转站）。AI 只生成「看什么」的面板定义，数值仍全部来自本地查询。"
    >
      <div className="space-y-5 py-5">
        {loading ? (
          <p role="status" className="text-sm text-muted-foreground">
            正在读取配置…
          </p>
        ) : loadError ? (
          <p role="alert" className="text-sm text-destructive">
            {loadError}
          </p>
        ) : (
          <>
            <Field
              label="Base URL"
              htmlFor={baseUrlId}
              hint="OpenAI 兼容接口地址，需包含版本路径，例如 https://api.deepseek.com/v1。"
            >
              <Input
                id={baseUrlId}
                type="url"
                inputMode="url"
                autoComplete="off"
                spellCheck={false}
                value={baseUrl}
                disabled={busy}
                placeholder="https://api.deepseek.com/v1"
                onChange={(event) => {
                  setBaseUrl(event.target.value);
                  setConfigMessage(null);
                  setConfigError(null);
                }}
              />
            </Field>

            <Field label="模型" htmlFor={modelId} hint="填写接口使用的模型名，例如 deepseek-chat。">
              <Input
                id={modelId}
                type="text"
                autoComplete="off"
                spellCheck={false}
                value={model}
                disabled={busy}
                placeholder="deepseek-chat"
                onChange={(event) => {
                  setModel(event.target.value);
                  setConfigMessage(null);
                  setConfigError(null);
                }}
              />
            </Field>

            <Field
              label="API Key"
              htmlFor={keyId}
              hint="保存到系统凭据库（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service），保存后无法再显示，可随时一键清除。"
            >
              <div className="flex flex-wrap items-center gap-2">
                <Input
                  id={keyId}
                  type="password"
                  autoComplete="off"
                  spellCheck={false}
                  value={keyInput}
                  disabled={busy}
                  placeholder={hasKey ? "已保存（如需更换请重新粘贴）" : "粘贴 API Key"}
                  onChange={(event) => {
                    setKeyInput(event.target.value);
                    setKeyError(null);
                  }}
                  className="min-w-[12rem] flex-1"
                />
                <Badge variant="outline" aria-live="polite">
                  {hasKey ? "已保存" : "未保存"}
                </Badge>
                <Button size="sm" variant="outline" disabled={busy} onClick={() => void saveKey()}>
                  {savingKey ? "保存中…" : "保存"}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={busy || !hasKey}
                  onClick={() => void clearKey()}
                >
                  {clearingKey ? "清除中…" : "清除"}
                </Button>
              </div>
            </Field>

            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" disabled={busy} onClick={() => void saveConfig()}>
                {savingConfig ? "保存中…" : "保存配置"}
              </Button>
              <Button size="sm" variant="outline" disabled={!canTest} onClick={() => void test()}>
                {testing ? "测试中…" : "测试连接"}
              </Button>
              {!canTest && !busy && hasKey ? (
                <span className="text-xs text-muted-foreground">
                  填写 Base URL 与模型后可测试连接
                </span>
              ) : null}
            </div>

            {configMessage ? (
              <p role="status" className="text-sm">
                {configMessage}
              </p>
            ) : null}
            {configError ? (
              <p role="alert" className="text-sm text-destructive">
                {configError}
              </p>
            ) : null}
            {keyError ? (
              <p role="alert" className="text-sm text-destructive">
                {keyError}
              </p>
            ) : null}
            {testResult ? (
              testResult.ok ? (
                <p role="status" className="text-sm">
                  连接成功{testResult.message ? `：${testResult.message}` : ""}
                </p>
              ) : (
                <p role="alert" className="text-sm text-destructive">
                  连接失败
                  {testResult.message ? `：${testResult.message}` : "，请检查配置与网络。"}
                </p>
              )
            ) : null}
          </>
        )}
      </div>
    </SettingsSection>
  );
}
