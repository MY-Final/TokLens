/** 面板取数：复用 `useAsyncData` 的 key/scope 语义（口径变化隐藏旧数据，后台刷新保留数据）。 */

import { useAsyncData, type AsyncData } from "@/hooks/use-async-data";
import { runPanelQuery } from "@/lib/api";
import {
  buildPanelQueryArgs,
  panelQueryScope,
  stableStringify,
  type PanelRenderContext,
  type PanelSpec,
} from "@/lib/panel-spec";

export function usePanelData(spec: PanelSpec, context: PanelRenderContext): AsyncData<unknown> {
  const scope = panelQueryScope(spec, context);
  const options = stableStringify(spec.typeOptions);
  const key = `${spec.id}:${scope}:${options}:${context.refreshToken}`;
  return useAsyncData(
    () => runPanelQuery(spec, buildPanelQueryArgs(spec, context)),
    key,
    `${spec.id}:${scope}`,
  );
}
