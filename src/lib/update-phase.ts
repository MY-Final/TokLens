import type { UpdatePhase } from "@/lib/settings-types";

const LABELS: Record<UpdatePhase, string> = {
  disabled: "更新不可用",
  idle: "检查是否有新版本",
  checking: "正在检查更新…",
  up_to_date: "已是最新版本",
  available: "发现新版本",
  downloading: "正在下载更新…",
  downloaded: "更新已准备好",
  installing: "正在安装更新…",
  unavailable: "暂时无法检查更新",
  failed: "更新未完成",
};

/** 更新状态的最小结构，便于纯函数判断当前阶段。 */
export type UpdatePhaseSource = {
  state: { phase: UpdatePhase } | null;
  operation: "check" | "download" | "install" | null;
};

/** 进行中的操作优先于后端状态，避免阶段显示滞后。 */
export function resolveUpdatePhase(source: UpdatePhaseSource): UpdatePhase | null {
  if (source.operation === "check") return "checking";
  if (source.operation === "download") return "downloading";
  if (source.operation === "install") return "installing";
  return source.state?.phase ?? null;
}

export function updatePhaseLabel(phase: UpdatePhase | null): string {
  return phase ? LABELS[phase] : "读取更新状态…";
}

/** 失败原因：错误文案由操作栏展示，标题区不再重复阶段。 */
export function updateErrorText(source: {
  error: string | null;
  state: { error: string | null } | null;
}): string | null {
  return source.error ?? source.state?.error ?? null;
}
