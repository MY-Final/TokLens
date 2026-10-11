import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { releaseNotesSummary, releaseNotesText } from "@/lib/release-notes";
import type { UpdateState } from "@/lib/settings-types";

// Module lifetime matches an application run, including StrictMode effect replay.
const announcedVersions = new Set<string>();

export function UpdateAnnouncement({
  state,
  currentVersion,
  onOpenUpdates,
}: {
  state: UpdateState | null;
  currentVersion: string | null | undefined;
  onOpenUpdates: () => void;
}) {
  const [details, setDetails] = useState<{ version: string; notes: string | null } | null>(null);

  useEffect(() => {
    if (state?.phase !== "available" || !state.version || announcedVersions.has(state.version)) {
      return;
    }
    const version = state.version;
    const notes = state.notes;
    announcedVersions.add(version);
    const summary = releaseNotesSummary(notes);
    toast.info(`TokLens ${version} 可用`, {
      id: `update-available-${version}`,
      description: summary.length ? (
        <div className="space-y-1">
          {summary.map((line, index) => (
            <p key={index} className="line-clamp-1 break-all">
              {line}
            </p>
          ))}
        </div>
      ) : (
        "此版本未提供更新说明。"
      ),
      closeButton: true,
      duration: 12_000,
      action: { label: "查看详情", onClick: () => setDetails({ version, notes }) },
    });
  }, [state]);

  return (
    <Dialog open={details !== null} onOpenChange={(open) => !open && setDetails(null)}>
      <DialogContent className="max-h-[85vh]">
        <DialogHeader>
          <DialogTitle>发现新版本</DialogTitle>
          <DialogDescription>
            当前版本：{currentVersion ? `v${currentVersion}` : "—"} · 新版本：
            {details ? `v${details.version}` : "—"}
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-[50vh] overflow-y-auto rounded-md border p-4 text-sm break-words whitespace-pre-wrap">
          {releaseNotesText(details?.notes ?? null) || "此版本未提供更新说明。"}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => setDetails(null)}>
            关闭
          </Button>
          <Button
            onClick={() => {
              setDetails(null);
              onOpenUpdates();
            }}
          >
            前往更新
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
