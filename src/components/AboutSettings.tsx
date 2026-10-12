import { toast } from "sonner";
import { SettingsRow, SettingsSection } from "@/components/SettingsFields";
import { Button } from "@/components/ui/button";
import type { SettingsController } from "@/hooks/use-settings";
import { openProjectPage, toErrorMessage } from "@/lib/api";

export function AboutSettings({ controller }: { controller: SettingsController }) {
  return (
    <SettingsSection title="关于 TokLens">
      <SettingsRow title="当前版本">
        <span className="font-mono text-sm">
          {controller.appInfo ? `v${controller.appInfo.version}` : "—"}
        </span>
      </SettingsRow>
      <SettingsRow title="开源项目">
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            void openProjectPage().catch((cause: unknown) => toast.error(toErrorMessage(cause)));
          }}
        >
          GitHub 项目
        </Button>
      </SettingsRow>
      <SettingsRow title="沟通群聊">
        <span className="text-sm select-text">
          QQ 群：<span className="font-mono tabular-nums">668526188</span>
        </span>
      </SettingsRow>
    </SettingsSection>
  );
}
