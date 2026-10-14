import { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { FilterSelectContent } from "@/components/FilterSelectContent";
import { CLIENT_LABELS, clientLabel, isClientKey } from "@/lib/client-labels";
import type { SessionBrowserState, SessionFilters, SessionSort } from "@/lib/usage-types";

const SEARCH_DEBOUNCE_MS = 250;

export function SessionFiltersToolbar({
  state,
  onChange,
}: {
  state: SessionBrowserState;
  onChange: (filters: Partial<SessionFilters>) => void;
}) {
  const { client, search, sort } = state;
  const clientValue = client ?? "all";
  const [searchDraft, setSearchDraft] = useState(search);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    if (debounceRef.current === undefined) setSearchDraft(search);
  }, [search]);

  useEffect(
    () => () => {
      if (debounceRef.current !== undefined) clearTimeout(debounceRef.current);
    },
    [],
  );

  const emitSearch = (value: string) => {
    debounceRef.current = undefined;
    onChange({ search: value });
  };

  const changeSearch = (value: string) => {
    setSearchDraft(value);
    if (debounceRef.current !== undefined) clearTimeout(debounceRef.current);
    if (value.trim() === "") {
      emitSearch(value);
      return;
    }
    debounceRef.current = setTimeout(() => emitSearch(value), SEARCH_DEBOUNCE_MS);
  };

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-3">
        <Input
          value={searchDraft}
          onChange={(event) => changeSearch(event.target.value)}
          placeholder="搜索项目、模型或会话 ID"
          aria-label="搜索项目、模型或会话 ID"
          className="h-9 w-72 max-w-full min-w-0 flex-1 basis-64"
        />
        <div className="flex flex-wrap items-center gap-3">
          <Select value={sort} onValueChange={(value) => onChange({ sort: value as SessionSort })}>
            <SelectTrigger className="h-9 w-40 shrink-0" aria-label="会话排序">
              <SelectValue />
            </SelectTrigger>
            <FilterSelectContent>
              <SelectItem value="newest">最近开始</SelectItem>
              <SelectItem value="oldest">最早开始</SelectItem>
              <SelectItem value="tokens_desc">Token 最高</SelectItem>
              <SelectItem value="requests_desc">请求最多</SelectItem>
            </FilterSelectContent>
          </Select>
          <Select
            value={clientValue}
            onValueChange={(value) => {
              onChange({ client: isClientKey(value) ? value : null });
            }}
          >
            <SelectTrigger className="h-9 w-44 shrink-0" aria-label="选择客户端">
              <SelectValue />
            </SelectTrigger>
            <FilterSelectContent>
              <SelectItem value="all">全部客户端</SelectItem>
              {Object.keys(CLIENT_LABELS)
                .filter(isClientKey)
                .map((entry) => (
                  <SelectItem key={entry} value={entry}>
                    {clientLabel(entry)}
                  </SelectItem>
                ))}
            </FilterSelectContent>
          </Select>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 text-xs text-muted-foreground">
        <span>{state.requestTime ? "区间内有已记录请求的会话" : "按会话开始时间"}</span>
        <Button
          size="sm"
          variant="ghost"
          className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground"
          onClick={() => onChange({ requestTime: !state.requestTime })}
        >
          切换为{state.requestTime ? "会话开始时间" : "请求发生时间"}
        </Button>
        {state.model !== undefined ? (
          <Button size="sm" variant="outline" onClick={() => onChange({ model: undefined })}>
            模型：{state.model ?? "未记录"} ×
          </Button>
        ) : null}
        {state.projectPath !== undefined ? (
          <Button
            size="sm"
            variant="outline"
            className="max-w-full"
            onClick={() => onChange({ projectPath: undefined })}
            title={state.projectPath ?? "未记录项目"}
          >
            <span className="truncate">项目：{state.projectPath ?? "未记录"}</span> ×
          </Button>
        ) : null}
      </div>
    </div>
  );
}
