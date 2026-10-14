/** 面板渲染器：取数 → 搬运 → 三态 → 组件（渲染器本身不感知具体类型）。 */

import type { ReactNode } from "react";

import { PanelShell, type PanelState } from "@/components/panels/PanelShell";
import { PANEL_ADAPTERS, PANEL_SKELETONS } from "@/components/panels/registry";
import { usePanelData } from "@/hooks/use-panels/use-panel-data";
import type { PanelRenderContext, PanelSpec } from "@/lib/panel-spec";

export function PanelRenderer({
  spec,
  context,
  actions,
}: {
  spec: PanelSpec;
  context: PanelRenderContext;
  actions?: ReactNode;
}) {
  const adapter = PANEL_ADAPTERS[spec.type];
  const result = usePanelData(spec, context);
  const data = result.data === null ? null : adapter.transform(result.data, spec, context);
  const state: PanelState = result.error
    ? { kind: "error", message: result.error, onRetry: result.reload }
    : data === null
      ? { kind: "loading" }
      : adapter.isEmpty(data)
        ? { kind: "empty" }
        : { kind: "ready" };
  return (
    <PanelShell spec={spec} state={state} skeletonByType={PANEL_SKELETONS} actions={actions}>
      {data === null ? null : adapter.render(data, spec, context)}
    </PanelShell>
  );
}
