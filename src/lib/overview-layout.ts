/** 核心指标卡片的响应式列数：3 个一行，4 个在窄屏折成两行。 */
export function overviewKpiGridClass(count: number): string {
  return count >= 4 ? "grid-cols-2 min-[960px]:grid-cols-4" : "grid-cols-1 min-[720px]:grid-cols-3";
}
