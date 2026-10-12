import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  SettingsRow,
  SettingsSection,
  SettingsSelect,
  SettingsSwitch,
} from "@/components/SettingsFields";
import type { SettingsController } from "@/hooks/use-settings";

export function CollectionSettings({ controller }: { controller: SettingsController }) {
  const { settings, busy, patch } = controller;
  if (!settings) return null;
  return (
    <SettingsSection title="采集">
      <SettingsRow title="采集状态" description="暂停保留历史，恢复后立即同步。">
        <div className="flex items-center gap-3">
          <Badge variant="outline">{settings.collectionPaused ? "已暂停" : "运行中"}</Badge>
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() => {
              void patch({ collectionPaused: !settings.collectionPaused });
            }}
          >
            {settings.collectionPaused ? "恢复采集" : "暂停采集"}
          </Button>
        </div>
      </SettingsRow>
      <SettingsSwitch
        title="启动时采集"
        checked={settings.syncOnStart}
        disabled={busy}
        onChange={(syncOnStart) => {
          void patch({ syncOnStart });
        }}
      />
      <SettingsSelect
        title="后台采集频率"
        value={String(settings.syncIntervalMinutes)}
        disabled={busy}
        options={[
          { value: "0", label: "关闭定时采集" },
          ...[1, 5, 15, 30].map((minutes) => ({
            value: String(minutes),
            label: `每 ${minutes} 分钟`,
          })),
        ]}
        onChange={(value) => {
          void patch({ syncIntervalMinutes: Number(value) });
        }}
      />
    </SettingsSection>
  );
}
