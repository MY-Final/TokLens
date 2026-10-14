/** 面板定义加载 / 保存 / 删除 / 排序；加载时逐项校验，非法项整体隔离、不半渲染。 */

import { useCallback, useEffect, useRef, useState } from "react";

import {
  deletePanelSpec,
  getPanelSpecs,
  reorderPanels,
  savePanelSpec,
  toErrorMessage,
} from "@/lib/api";
import {
  PANEL_SPEC_VERSION,
  readPanelSpecVersion,
  validatePanelSpec,
  type PanelSpec,
  type UnsupportedPanel,
} from "@/lib/panel-spec";

export type PanelsController = {
  panels: PanelSpec[];
  unsupported: UnsupportedPanel[];
  invalidCount: number;
  loading: boolean;
  error: string | null;
  reload: () => Promise<void>;
  save: (spec: PanelSpec) => Promise<void>;
  remove: (id: string) => Promise<void>;
  reorder: (ids: string[]) => Promise<void>;
};

type PanelsState = {
  panels: PanelSpec[];
  unsupported: UnsupportedPanel[];
  invalidCount: number;
};

function toUnsupported(value: unknown, specVersion: number): UnsupportedPanel {
  const record = (value ?? {}) as Record<string, unknown>;
  const id = typeof record.id === "string" && record.id ? record.id : `v${specVersion}`;
  const title =
    typeof record.title === "string" && record.title.trim()
      ? record.title
      : `面板（版本 ${specVersion}）`;
  return { id, title, specVersion };
}

export function usePanels(): PanelsController {
  const [state, setState] = useState<PanelsState>({
    panels: [],
    unsupported: [],
    invalidCount: 0,
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);

  const accept = useCallback((raw: unknown) => {
    const list = Array.isArray(raw) ? raw : [];
    const panels: PanelSpec[] = [];
    const unsupported: UnsupportedPanel[] = [];
    let invalidCount = 0;
    for (const item of list) {
      const version = readPanelSpecVersion(item);
      if (version !== null && version > PANEL_SPEC_VERSION) {
        unsupported.push(toUnsupported(item, version));
        continue;
      }
      const result = validatePanelSpec(item);
      if (result.ok) panels.push(result.spec);
      else invalidCount += 1;
    }
    panels.sort((left, right) => left.layout.order - right.layout.order);
    setState({ panels, unsupported, invalidCount });
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const raw = await getPanelSpecs();
      if (!mounted.current) return;
      accept(raw);
      setError(null);
    } catch (cause: unknown) {
      if (mounted.current) setError(toErrorMessage(cause));
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [accept]);

  useEffect(() => {
    mounted.current = true;
    // Defer the initial load out of the effect body to avoid a synchronous setState cascade.
    void Promise.resolve().then(() => {
      if (mounted.current) void reload();
    });
    return () => {
      mounted.current = false;
    };
  }, [reload]);

  const save = useCallback(
    async (spec: PanelSpec) => {
      accept(await savePanelSpec(spec));
    },
    [accept],
  );

  const remove = useCallback(
    async (id: string) => {
      accept(await deletePanelSpec(id));
    },
    [accept],
  );

  const reorder = useCallback(
    async (ids: string[]) => {
      accept(await reorderPanels(ids));
    },
    [accept],
  );

  return {
    panels: state.panels,
    unsupported: state.unsupported,
    invalidCount: state.invalidCount,
    loading,
    error,
    reload,
    save,
    remove,
    reorder,
  };
}
