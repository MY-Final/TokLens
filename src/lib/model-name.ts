/** 模型名展示层归一化：剥离来源/通道前缀，来源单独作为副信息展示。 */

export type NormalizedModel = {
  /** 归一化后的模型名，例如 deepseek-flash。 */
  model: string;
  /** 来源或通道，例如 deepseek-official；无前缀时为 null。 */
  source: string | null;
  /** 原始模型串，未知模型为 null。 */
  raw: string | null;
};

/**
 * 归一化模型名：取最后一个 `/` 之后的部分作为模型名，之前的前缀作为来源。
 * 只影响展示与聚合口径，不改变后端返回的原始模型串。
 */
export function normalizeModelName(raw: string | null): NormalizedModel {
  if (!raw || raw.trim().length === 0) {
    return { model: "未知", source: null, raw: null };
  }
  const index = raw.lastIndexOf("/");
  if (index <= 0 || index >= raw.length - 1) {
    return { model: raw, source: null, raw };
  }
  return { model: raw.slice(index + 1), source: raw.slice(0, index), raw };
}
