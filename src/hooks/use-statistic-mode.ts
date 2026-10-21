import { createContext, useContext } from "react";

/** 全局统计口径：请求时间归集与会话主时间归集。 */
export type StatisticMode = "requestTime" | "sessionSummary";

export const StatisticModeContext = createContext<StatisticMode>("requestTime");

const MODE_COPY: Record<
  StatisticMode,
  { label: string; attribution: string; explain: string; tooltipBasis: string }
> = {
  requestTime: {
    label: "按请求时间",
    attribution: "按请求时间归集",
    explain:
      "按请求实际发生时间统计：每条请求按自己的发生时间计入所在区间；没有请求明细的会话按会话开始日归集。",
    tooltipBasis: "请求时间",
  },
  sessionSummary: {
    label: "按会话汇总",
    attribution: "按会话主时间归集",
    explain:
      "按会话归属的主时间归集：整段会话计入会话主时间所在区间，与会话内请求的实际发生时间可能不同。",
    tooltipBasis: "会话主时间",
  },
};

/**
 * 统计口径的单一来源：模块副标题、图注、tooltip 与空态都从这里取文案，
 * 不允许各模块自行硬编码归集口径。
 */
export function useStatisticMode() {
  const mode = useContext(StatisticModeContext);
  const copy = MODE_COPY[mode];
  return {
    mode,
    isRequestTime: mode === "requestTime",
    /** 控件名称，如「按请求时间」。 */
    label: copy.label,
    /** 归集口径短句，用于副标题与图注。 */
    attribution: copy.attribution,
    /** 口径说明，用于口径旁的浮层与数据说明。 */
    explain: copy.explain,
    /** 浮框中的口径行前缀。 */
    tooltipBasis: copy.tooltipBasis,
  };
}
