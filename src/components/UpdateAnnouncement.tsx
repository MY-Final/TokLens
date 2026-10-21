import { useEffect, useRef } from "react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { UpdatePanel } from "@/components/UpdatePanel";
import { releaseNotesSummary } from "@/lib/release-notes";
import { resolveUpdatePhase, updateErrorText, updatePhaseLabel } from "@/lib/update-phase";
import type { UpdatesController } from "@/hooks/use-updates";

const announcedVersions = new Set<string>();

export function UpdateAnnouncement({
  updates,
  currentVersion,
  open,
  onOpenChange,
}: {
  updates: UpdatesController;
  currentVersion: string | null | undefined;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const state = updates.state;
  const phase = resolveUpdatePhase(updates);
  const busy =
    updates.busy || phase === "checking" || phase === "downloading" || phase === "installing";
  const hasError = !busy && !!updateErrorText(updates);
  // 失败原因已在操作栏说明，标题区只保留应用名，避免同一信息出现两次。
  const subtitle = hasError ? "TokLens" : `TokLens · ${updatePhaseLabel(phase)}`;
  const lastManualCheck = useRef(0);
  useEffect(() => {
    const manualCheck = lastManualCheck.current !== updates.checkSequence;
    lastManualCheck.current = updates.checkSequence;
    if (
      state?.phase !== "available" ||
      !state.version ||
      (!manualCheck && announcedVersions.has(state.version))
    )
      return;
    const version = state.version;
    announcedVersions.add(version);
    if (open) return;
    const summary = releaseNotesSummary(state.notes);
    toast.info(`TokLens v${version} 可用`, {
      id: `update-available-${version}`,
      description: summary.length ? summary.slice(0, 2).join("；") : undefined,
      closeButton: true,
      duration: 12_000,
      action: { label: "查看更新", onClick: () => onOpenChange(true) },
    });
  }, [state, onOpenChange, open, updates.checkSequence]);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl">
        <DialogHeader className="shrink-0 gap-1 border-b px-6 py-4">
          <DialogTitle>应用更新</DialogTitle>
          <DialogDescription className="sr-only">
            查看更新内容、下载进度并安装更新。
          </DialogDescription>
          <p className="text-sm text-muted-foreground">{subtitle}</p>
        </DialogHeader>
        <UpdatePanel updates={updates} currentVersion={currentVersion} layout="dialog" />
      </DialogContent>
    </Dialog>
  );
}
