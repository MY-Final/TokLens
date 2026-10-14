/** 自定义面板页：12 列响应式网格 + 增删复制/上下移 + 目录驱动的编辑表单。 */

import { useState } from "react";
import { toast } from "sonner";
import { ArrowDown, ArrowUp, Copy, LayoutGrid, Pencil, Plus, Trash2 } from "lucide-react";

import { DashboardCard } from "@/components/DashboardCard";
import { EmptyBlock, ErrorBlock } from "@/components/StateBlocks";
import { PanelEditor } from "@/components/panels/PanelEditor";
import { PanelRenderer } from "@/components/panels/PanelRenderer";
import { PANEL_GRID_CLASSES, PANEL_SPAN_CLASSES } from "@/components/panels/panel-layout";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { useAsyncData } from "@/hooks/use-async-data";
import { usePanels } from "@/hooks/use-panels/use-panels";
import { getPanelCatalog, toErrorMessage } from "@/lib/api";
import {
  createPanelId,
  type PanelRenderContext,
  type PanelSpec,
  type UnsupportedPanel,
} from "@/lib/panel-spec";
import type { DateRange } from "@/lib/ranges";
import type { SessionFilters } from "@/lib/usage-types";

function PanelControls({
  index,
  total,
  onEdit,
  onDuplicate,
  onMove,
  onDelete,
}: {
  index: number;
  total: number;
  onEdit: () => void;
  onDuplicate: () => void;
  onMove: (delta: number) => void;
  onDelete: () => void;
}) {
  return (
    <>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="上移"
        disabled={index === 0}
        onClick={() => onMove(-1)}
      >
        <ArrowUp className="size-3.5" aria-hidden="true" />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="下移"
        disabled={index === total - 1}
        onClick={() => onMove(1)}
      >
        <ArrowDown className="size-3.5" aria-hidden="true" />
      </Button>
      <Button variant="ghost" size="icon-sm" aria-label="编辑" onClick={onEdit}>
        <Pencil className="size-3.5" aria-hidden="true" />
      </Button>
      <Button variant="ghost" size="icon-sm" aria-label="复制" onClick={onDuplicate}>
        <Copy className="size-3.5" aria-hidden="true" />
      </Button>
      <Button variant="ghost" size="icon-sm" aria-label="删除" onClick={onDelete}>
        <Trash2 className="size-3.5" aria-hidden="true" />
      </Button>
    </>
  );
}

function UnsupportedPanelCard({ panel }: { panel: UnsupportedPanel }) {
  return (
    <DashboardCard className="py-5">
      <CardContent className="space-y-2 px-5">
        <h3 className="truncate text-sm font-medium" title={panel.title}>
          {panel.title}
        </h3>
        <p className="text-xs leading-relaxed text-muted-foreground">
          此面板由更高版本（v{panel.specVersion}）创建，已保留数据但暂不支持渲染或编辑。
        </p>
      </CardContent>
    </DashboardCard>
  );
}

export function CustomPanelsPage({
  range,
  nowMs,
  firstEventMs,
  refreshToken,
  onOpenSessions,
}: {
  range: DateRange;
  nowMs: number;
  firstEventMs: number | null;
  refreshToken: number;
  onOpenSessions: (filters?: Partial<SessionFilters>, range?: DateRange) => void;
}) {
  const panels = usePanels();
  const catalog = useAsyncData(
    () => getPanelCatalog(),
    `panel-catalog:${refreshToken}`,
    "panel-catalog",
  );
  const [editorOpen, setEditorOpen] = useState(false);
  const [editing, setEditing] = useState<PanelSpec | null>(null);
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  const context: PanelRenderContext = {
    range,
    firstEventMs,
    nowMs,
    refreshToken,
    onOpenSessions,
  };

  const openCreate = () => {
    setEditing(null);
    setEditorOpen(true);
  };
  const openEdit = (spec: PanelSpec) => {
    setEditing(spec);
    setEditorOpen(true);
  };

  const handleSave = async (spec: PanelSpec) => {
    const next = editing
      ? spec
      : { ...spec, layout: { ...spec.layout, order: panels.panels.length } };
    await panels.save(next);
  };

  const confirmDelete = () => {
    const id = pendingDelete;
    setPendingDelete(null);
    if (!id) return;
    void panels.remove(id).catch((cause: unknown) => toast.error(toErrorMessage(cause)));
  };

  const duplicate = (spec: PanelSpec) => {
    const copy: PanelSpec = {
      ...spec,
      id: createPanelId(),
      layout: { ...spec.layout, order: panels.panels.length },
    };
    void panels.save(copy).catch((cause: unknown) => toast.error(toErrorMessage(cause)));
  };

  const move = (index: number, delta: number) => {
    const target = index + delta;
    if (target < 0 || target >= panels.panels.length) return;
    const ids = panels.panels.map((panel) => panel.id);
    const current = ids[index];
    const other = ids[target];
    if (current === undefined || other === undefined) return;
    ids[index] = other;
    ids[target] = current;
    void panels.reorder(ids).catch((cause: unknown) => toast.error(toErrorMessage(cause)));
  };

  const hasPanels = panels.panels.length > 0 || panels.unsupported.length > 0;

  return (
    <div className="space-y-4">
      {panels.error ? (
        <ErrorBlock message={panels.error} onRetry={() => void panels.reload()} />
      ) : null}
      {panels.invalidCount > 0 ? (
        <p className="text-xs leading-relaxed text-muted-foreground">
          有 {panels.invalidCount} 个面板定义无效，已整体隔离（不会半渲染）。
        </p>
      ) : null}

      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-xs text-muted-foreground tabular-nums">
          {panels.loading ? "加载中…" : `${panels.panels.length} 个面板`}
        </p>
        <Button size="sm" onClick={openCreate}>
          <Plus className="size-4" aria-hidden="true" />
          新增面板
        </Button>
      </div>

      {!panels.loading && !hasPanels ? (
        <DashboardCard>
          <CardContent className="px-5">
            <EmptyBlock
              title="还没有自定义面板"
              description="从白名单选择面板类型与参数，全部本地离线，数值取自本机统计。"
              action={
                <Button size="sm" onClick={openCreate}>
                  <LayoutGrid className="size-4" aria-hidden="true" />
                  新增面板
                </Button>
              }
            />
          </CardContent>
        </DashboardCard>
      ) : (
        <div className={PANEL_GRID_CLASSES}>
          {panels.panels.map((spec, index) => (
            <div key={spec.id} className={PANEL_SPAN_CLASSES[spec.layout.colSpan]}>
              <PanelRenderer
                spec={spec}
                context={context}
                actions={
                  <PanelControls
                    index={index}
                    total={panels.panels.length}
                    onEdit={() => openEdit(spec)}
                    onDuplicate={() => duplicate(spec)}
                    onMove={(delta) => move(index, delta)}
                    onDelete={() => setPendingDelete(spec.id)}
                  />
                }
              />
            </div>
          ))}
          {panels.unsupported.map((panel) => (
            <div key={panel.id} className="col-span-1 sm:col-span-2 lg:col-span-3 xl:col-span-3">
              <UnsupportedPanelCard panel={panel} />
            </div>
          ))}
        </div>
      )}

      <PanelEditor
        open={editorOpen}
        onOpenChange={setEditorOpen}
        catalog={catalog.data}
        initial={editing}
        onSave={handleSave}
      />

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除这个面板？</AlertDialogTitle>
            <AlertDialogDescription>
              只删除面板定义，不影响已采集的统计与会话数据。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction onClick={confirmDelete}>删除</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
