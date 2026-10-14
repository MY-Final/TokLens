/**
 * 12 列响应式网格的静态宽度映射。
 *
 * Tailwind v4 只扫描源码里的字面量类名，`col-span-${n}` 动态拼接不会生成，
 * 因此每种 colSpan 必须落成完整静态类（见 docs/generative-panel-spec.md ⑤）。
 */

import type { PanelColSpan } from "@/lib/panel-spec";

export const PANEL_GRID_CLASSES =
  "grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-6 xl:grid-cols-12";

export const PANEL_SPAN_CLASSES: Record<PanelColSpan, string> = {
  3: "col-span-1 sm:col-span-1 lg:col-span-3 xl:col-span-3",
  4: "col-span-1 sm:col-span-2 lg:col-span-4 xl:col-span-4",
  6: "col-span-1 sm:col-span-2 lg:col-span-3 xl:col-span-6",
  12: "col-span-1 sm:col-span-2 lg:col-span-6 xl:col-span-12",
};
