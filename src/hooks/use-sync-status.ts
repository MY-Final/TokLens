import { createContext, useContext } from "react";

/**
 * 采集 / 同步进行中信号。仅用于“视图数据为空且仍在采集”时显示骨架：
 * 有数据的视图必须保持数据，不得因后台同步闪回骨架（见 docs/ui-guidelines.md）。
 */
export const SyncStatusContext = createContext<boolean>(false);

export function useSyncing(): boolean {
  return useContext(SyncStatusContext);
}
