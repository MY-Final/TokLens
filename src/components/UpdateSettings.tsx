import { SettingsSection, SettingsSwitch } from "@/components/SettingsFields";
import { UpdatePanel } from "@/components/UpdatePanel";
import type { SettingsController } from "@/hooks/use-settings";
import type { UpdatesController } from "@/hooks/use-updates";

export function UpdateSettings({
  controller,
  updates,
}: {
  controller: SettingsController;
  updates: UpdatesController;
}) {
  return (
    <SettingsSection title="更新">
      <div className="py-5">
        <UpdatePanel updates={updates} currentVersion={controller.appInfo?.version} />
      </div>
      {controller.settings && (
        <SettingsSwitch
          title="自动检查更新"
          description="每天最多访问一次 GitHub，仅检查。"
          checked={controller.settings.automaticUpdates}
          disabled={controller.busy}
          onChange={(automaticUpdates) => void controller.patch({ automaticUpdates })}
        />
      )}
    </SettingsSection>
  );
}
