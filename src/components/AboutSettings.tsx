import { toast } from "sonner";
import { SettingsRow, SettingsSection } from "@/components/SettingsFields";
import { Button } from "@/components/ui/button";
import type { SettingsController } from "@/hooks/use-settings";
import { openProjectPage, toErrorMessage } from "@/lib/api";

export function AboutSettings({ controller }: { controller: SettingsController }) {
  return (
    <SettingsSection title="关于 TokLens" description="本地优先的 AI 编程助手用量统计工具。">
      <SettingsRow title="当前版本">
        <span className="font-mono text-sm">
          {controller.appInfo ? `v${controller.appInfo.version}` : "—"}
        </span>
      </SettingsRow>
      <SettingsRow title="开源项目" description="查看项目源码、发布记录或反馈问题。">
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
      <SettingsRow
        title="本地优先 · 隐私保护"
        description="默认不联网、不上传、不做遥测；只读取统计所需元数据，不保存会话正文。"
      >
        <span className="text-xs text-muted-foreground">只读采集</span>
      </SettingsRow>
    </SettingsSection>
  );
}
