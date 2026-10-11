import { SettingsSection, SettingsSelect, SettingsSwitch } from "@/components/SettingsFields";
import { ErrorBlock, LoadingRows } from "@/components/StateBlocks";
import type { SettingsController } from "@/hooks/use-settings";
import type { CloseBehavior, Theme } from "@/lib/settings-types";
import { RANGE_OPTIONS } from "@/lib/ranges";
import { CollectionSettings } from "@/components/CollectionSettings";
import { DataSettings } from "@/components/DataSettings";

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

export function SettingsPage({ controller }: { controller: SettingsController }) {
  const { settings, busy, error, warning, patch, reload } = controller;
  if (!settings)
    return error ? (
      <ErrorBlock
        message={error}
        onRetry={() => {
          void reload();
        }}
      />
    ) : (
      <LoadingRows rows={6} />
    );
  return (
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
      <SettingsSection title="通用" description="窗口、启动与关闭行为。更改会自动保存到本机。">
        <SettingsSelect
          title="关闭主窗口时"
          value={settings.closeBehavior}
          options={CLOSE_OPTIONS}
          disabled={busy}
          onChange={(closeBehavior) => {
            void patch({ closeBehavior });
          }}
        />
        <SettingsSwitch
          title="开机自启"
          checked={settings.autostart}
          disabled={busy}
          onChange={(autostart) => {
            void patch({ autostart });
          }}
        />
        <SettingsSwitch
          title="开机启动时隐藏到托盘"
          description="仅对开机自启生效，手动打开应用始终显示窗口。"
          checked={settings.autostartHidden}
          disabled={busy || !settings.autostart}
          onChange={(autostartHidden) => {
            void patch({ autostartHidden });
          }}
        />
        <SettingsSwitch
          title="记住窗口尺寸与位置"
          checked={settings.rememberWindow}
          disabled={busy}
          onChange={(rememberWindow) => {
            void patch({ rememberWindow });
          }}
        />
      </SettingsSection>
      <SettingsSection title="外观" description="采用系统主题，或选择适合你的工作环境的配色。">
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
          options={RANGE_OPTIONS.map((option) => ({ value: option.key, label: option.label }))}
          disabled={busy}
          onChange={(defaultRange) => {
            void patch({ defaultRange });
          }}
        />
      </SettingsSection>
      <CollectionSettings controller={controller} />
      <DataSettings controller={controller} />
    </div>
  );
}
