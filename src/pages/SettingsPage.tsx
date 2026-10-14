import { useEffect, useRef, useState } from "react";
import { Download, Info, Palette, ScanLine, Settings2, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { AboutSettings } from "@/components/AboutSettings";
import { cn } from "@/lib/utils";
import { SettingsSection, SettingsSelect, SettingsSwitch } from "@/components/SettingsFields";
import { ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import type { SettingsController } from "@/hooks/use-settings";
import type { CloseBehavior, Theme } from "@/lib/settings-types";
import { RANGE_OPTIONS } from "@/lib/ranges";
import { TOKEN_DISPLAY_OPTIONS } from "@/lib/format";
import { CollectionSettings } from "@/components/CollectionSettings";
import { DataSettings } from "@/components/DataSettings";
import { UpdateSettings } from "@/components/UpdateSettings";
import type { UpdatesController } from "@/hooks/use-updates";

const CLOSE_OPTIONS: { value: CloseBehavior; label: string }[] = [
  { value: "ask", label: "每次询问" },
  { value: "tray", label: "缩小到托盘" },
  { value: "exit", label: "退出应用" },
];
const THEME_OPTIONS: { value: Theme; label: string }[] = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];

const CATEGORIES = [
  { key: "general", label: "通用", icon: Settings2, group: "应用" },
  { key: "appearance", label: "外观与统计", icon: Palette, group: "应用" },
  { key: "collection", label: "采集", icon: ScanLine, group: "应用" },
  { key: "updates", label: "更新", icon: Download, group: "管理" },
  { key: "data", label: "数据与隐私", icon: ShieldCheck, group: "管理" },
  { key: "about", label: "关于", icon: Info, group: "管理" },
] as const;
type Category = (typeof CATEGORIES)[number]["key"];

export function SettingsPage({
  controller,
  updates,
  updateFocusToken = 0,
}: {
  controller: SettingsController;
  updates: UpdatesController;
  updateFocusToken?: number;
}) {
  const { settings, busy, error, warning, patch, reload } = controller;
  const [selection, setSelection] = useState<{ category: Category; token: number }>({
    category: "general",
    token: 0,
  });
  const category =
    updateFocusToken !== 0 && selection.token !== updateFocusToken ? "updates" : selection.category;
  const selectCategory = (category: Category) =>
    setSelection({ category, token: updateFocusToken });
  const updateSection = useRef<HTMLDivElement>(null);
  const focusedToken = useRef(0);
  useEffect(() => {
    if (updateFocusToken === 0) {
      focusedToken.current = 0;
      return;
    }
    if (!settings || focusedToken.current === updateFocusToken) return;
    const section = updateSection.current;
    if (!section) return;
    focusedToken.current = updateFocusToken;
    section.scrollIntoView({ block: "start" });
    section.focus({ preventScroll: true });
  }, [settings, updateFocusToken, category]);
  if (!settings)
    return (
      <div className="p-5 lg:p-8">
        {error ? (
          <ErrorBlock
            message={error}
            onRetry={() => {
              void reload();
            }}
          />
        ) : (
          <LoadingRows rows={6} />
        )}
      </div>
    );
  return (
    <div className="flex min-h-[calc(100svh-4rem)] items-start">
      <nav
        aria-label="设置分类"
        className="sticky top-16 min-h-[calc(100svh-4rem)] w-44 shrink-0 border-r border-border/60 bg-sidebar/50 px-3 py-6 xl:w-52 xl:px-4"
      >
        <h2 className="mb-6 px-3 text-base font-semibold">偏好设置</h2>
        {["应用", "管理"].map((group) => (
          <div key={group} className="mb-6 space-y-1 last:mb-0">
            <p className="mb-2 px-3 text-xs font-medium text-muted-foreground">{group}</p>
            {CATEGORIES.filter((item) => item.group === group).map((item) => (
              <Button
                key={item.key}
                variant="ghost"
                aria-current={category === item.key ? "page" : undefined}
                onClick={() => selectCategory(item.key)}
                className={cn(
                  "h-10 w-full justify-start gap-3 rounded-lg px-3 text-muted-foreground",
                  category === item.key && "bg-accent font-medium text-accent-foreground",
                )}
              >
                <item.icon className="size-4 shrink-0" aria-hidden="true" />
                {item.label}
              </Button>
            ))}
          </div>
        ))}
      </nav>
      <div className="min-w-0 flex-1 px-5 py-6 lg:px-8 xl:px-10">
        <div className="mx-auto max-w-3xl space-y-4">
          {error && (
            <ErrorBlock
              message={error}
              onRetry={() => {
                void reload();
              }}
            />
          )}
          {warning && (
            <p role="status" className="rounded-lg border border-border bg-muted/50 p-3 text-sm">
              {warning}
            </p>
          )}
          {category === "general" && (
            <SettingsSection title="通用">
              <SettingsSelect
                title="关闭主窗口时"
                description="选择继续在托盘运行、退出应用或每次询问。"
                value={settings.closeBehavior}
                options={CLOSE_OPTIONS}
                disabled={busy}
                onChange={(closeBehavior) => {
                  void patch({ closeBehavior });
                }}
              />
              <SettingsSwitch
                title="开机自启"
                description="登录系统后自动启动 TokLens。"
                checked={settings.autostart}
                disabled={busy}
                onChange={(autostart) => {
                  void patch({ autostart });
                }}
              />
              <SettingsSwitch
                title="开机启动时隐藏到托盘"
                description="自启时在托盘运行，不打开主窗口。"
                checked={settings.autostartHidden}
                disabled={busy || !settings.autostart}
                onChange={(autostartHidden) => {
                  void patch({ autostartHidden });
                }}
              />
              <SettingsSwitch
                title="记住窗口尺寸与位置"
                description="下次打开时恢复上次的窗口大小和位置。"
                checked={settings.rememberWindow}
                disabled={busy}
                onChange={(rememberWindow) => {
                  void patch({ rememberWindow });
                }}
              />
            </SettingsSection>
          )}
          {category === "appearance" && (
            <SettingsSection title="外观与统计">
              <SettingsSelect
                title="主题"
                value={settings.theme}
                options={THEME_OPTIONS}
                disabled={busy}
                onChange={(theme) => {
                  void patch({ theme });
                }}
              />
              <SettingsSelect
                title="默认统计范围"
                value={settings.defaultRange}
                options={RANGE_OPTIONS.map((option) => ({
                  value: option.key,
                  label: option.label,
                }))}
                disabled={busy}
                onChange={(defaultRange) => {
                  void patch({ defaultRange });
                }}
              />
              <SettingsSelect
                title="Token 显示单位"
                value={settings.tokenDisplay}
                options={TOKEN_DISPLAY_OPTIONS}
                disabled={busy}
                onChange={(tokenDisplay) => {
                  void patch({ tokenDisplay });
                }}
              />
            </SettingsSection>
          )}
          {category === "collection" && <CollectionSettings controller={controller} />}
          {category === "updates" && (
            <div ref={updateSection} tabIndex={-1} aria-label="更新">
              <UpdateSettings controller={controller} updates={updates} />
            </div>
          )}
          {category === "data" && <DataSettings controller={controller} />}
          {category === "about" && <AboutSettings controller={controller} />}
        </div>
      </div>
    </div>
  );
}
