/** 通用异步数据加载：显式 key 决定何时重新请求；loading 由状态派生，避免级联渲染。 */

import { useCallback, useEffect, useRef, useState } from "react";

import { toErrorMessage } from "@/lib/api";

export type AsyncData<T> = {
  data: T | null;
  error: string | null;
  loading: boolean;
  reload: () => void;
};

type Snapshot<T> = { key: string; data: T | null; error: string | null };

export function useAsyncData<T>(loader: () => Promise<T>, key: string): AsyncData<T> {
  const [snapshot, setSnapshot] = useState<Snapshot<T> | null>(null);
  const [nonce, setNonce] = useState(0);
  const loaderRef = useRef(loader);

  // 每次渲染后同步最新的 loader（不读取 ref 于渲染期）。
  useEffect(() => {
    loaderRef.current = loader;
  });

  const requestKey = `${key}#${nonce}`;
  useEffect(() => {
    let active = true;
    loaderRef
      .current()
      .then((data) => {
        if (active) {
          setSnapshot({ key: requestKey, data, error: null });
        }
      })
      .catch((cause: unknown) => {
        if (active) {
          setSnapshot({ key: requestKey, data: null, error: toErrorMessage(cause) });
        }
      });
    return () => {
      active = false;
    };
  }, [requestKey]);

  const reload = useCallback(() => setNonce((value) => value + 1), []);
  const fresh = snapshot?.key === requestKey ? snapshot : null;

  return {
    data: fresh?.data ?? null,
    error: fresh?.error ?? null,
    // 当 key 变化或首次加载时视为加载中（派生值，不需要在 effect 里同步 setState）。
    loading: fresh === null,
    reload,
  };
}
