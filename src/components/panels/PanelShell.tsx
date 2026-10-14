/**
 * 面板薄壳（P1 冻结契约）：`spec` + 统一 `PanelState` + `skeletonByType` + `explanation` slot。
 *
 * 三态：error → ErrorBlock；空 && 同步中 → 形状骨架；empty → EmptyBlock；ready → children。
 * P1 复用带卡片的既有组件（ready 直接渲染 children），P1.5 再收敛为 bare 变体，
 * 由本壳统一承担标题与口径来源（见 docs/generative-panel-spec.md ⑧/⑩）。
 */

import type { ReactNode } from "react";

import { DashboardCard } from "@/components/DashboardCard";
import { EmptyBlock, ErrorBlock } from "@/components/StateBlocks";
import { useSyncing } from "@/hooks/use-sync-status";
import { CardContent } from "@/components/ui/card";
import { panelTitle, type PanelSpec, type PanelType } from "@/lib/panel-spec";

export type PanelState =
  | { kind: "loading" }
  | { kind: "empty" }
  | { kind: "error"; message: string; onRetry: () => void }
  | { kind: "ready" };

export type PanelSkeletonMap = Partial<Record<PanelType, ReactNode>>;

export function PanelShell({
  spec,
  state,
  skeletonByType,
  explanation,
  actions,
  children,
}: {
  spec: PanelSpec;
  state: PanelState;
  skeletonByType: PanelSkeletonMap;
  explanation?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
}) {
  const syncing = useSyncing();
  const title = panelTitle(spec);
  const skeleton = skeletonByType[spec.type] ?? null;
  const showSkeleton = state.kind === "loading" || (state.kind === "empty" && syncing);
  return (
    <section className="flex min-w-0 flex-col gap-2" aria-label={title}>
      <div className="flex min-h-6 items-center justify-between gap-2">
        <h3 className="truncate text-sm font-medium" title={title}>
          {title}
        </h3>
        {actions ? <div className="flex shrink-0 items-center gap-1">{actions}</div> : null}
      </div>
      {state.kind === "error" ? (
        <DashboardCard className="py-5">
          <CardContent className="px-5">
            <ErrorBlock message={state.message} onRetry={state.onRetry} />
          </CardContent>
        </DashboardCard>
      ) : showSkeleton ? (
        <DashboardCard className="py-5">
          <CardContent className="px-5">{skeleton}</CardContent>
        </DashboardCard>
      ) : state.kind === "empty" ? (
        <DashboardCard className="py-5">
          <CardContent className="px-5">
            <EmptyBlock
              title="当前范围没有数据"
              description="调整时间范围，或在数据源管理执行一次同步后重试。"
            />
          </CardContent>
        </DashboardCard>
      ) : (
        children
      )}
      {explanation ? (
        <div className="text-xs leading-relaxed text-muted-foreground">{explanation}</div>
      ) : null}
    </section>
  );
}
