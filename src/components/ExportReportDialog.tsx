import { useEffect, useId, useRef, useState } from "react";
import { Download, LoaderCircle } from "lucide-react";
import { DateRangePicker } from "@/components/DateRangePicker";
import { FilterSelectContent } from "@/components/FilterSelectContent";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { exportStatistics, getStorageStatus, toErrorMessage } from "@/lib/api";
import { CLIENT_LABELS, clientLabel, isClientKey } from "@/lib/client-labels";
import { formatNumber } from "@/lib/format";
import type { DateRange } from "@/lib/ranges";
import type { ExportResult, ReportFormat } from "@/lib/report-types";
import type { ClientKey } from "@/lib/usage-types";

const CLIENTS = Object.keys(CLIENT_LABELS).filter(isClientKey);

export function ExportReportDialog({
  range,
  nowMs,
  firstEventMs,
}: {
  range: DateRange;
  nowMs: number;
  firstEventMs: number | null;
}) {
  const id = useId();
  const inFlight = useRef(false);
  const [open, setOpen] = useState(false);
  const [selectedRange, setSelectedRange] = useState(range);
  const [frozenNowMs, setFrozenNowMs] = useState(nowMs);
  const [client, setClient] = useState<ClientKey | null>(null);
  const [format, setFormat] = useState<ReportFormat>("csv");
  const [directory, setDirectory] = useState("");
  const [includeEstimated, setIncludeEstimated] = useState(false);
  const [storageAttempt, setStorageAttempt] = useState(0);
  const [storageLoading, setStorageLoading] = useState(false);
  const [storageError, setStorageError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ExportResult | null>(null);

  useEffect(() => {
    if (!open) return;
    let active = true;
    getStorageStatus()
      .then((status) => {
        if (active) setDirectory((current) => current || status.directory);
      })
      .catch((cause: unknown) => {
        if (active) setStorageError(toErrorMessage(cause));
      })
      .finally(() => {
        if (active) setStorageLoading(false);
      });
    return () => {
      active = false;
    };
  }, [open, storageAttempt]);

  function changeOpen(next: boolean) {
    if (inFlight.current) return;
    if (next) {
      setSelectedRange({ ...range });
      setFrozenNowMs(nowMs);
      setClient(null);
      setFormat("csv");
      setIncludeEstimated(false);
      setDirectory("");
      setStorageLoading(true);
      setStorageError(null);
      setError(null);
      setResult(null);
    }
    setOpen(next);
  }

  async function save() {
    if (inFlight.current || storageLoading || !directory.trim()) return;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(
        await exportStatistics({
          directory: directory.trim(),
          format,
          fromMs: selectedRange.fromMs,
          toMs: selectedRange.toMs,
          client,
          includeEstimated,
        }),
      );
    } catch (cause: unknown) {
      setError(toErrorMessage(cause));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }

  return (
    <>
      <Button
        variant="ghost"
        size="icon-sm"
        title="导出统计报表"
        aria-label="导出统计报表"
        onClick={() => changeOpen(true)}
      >
        <Download className="size-4" aria-hidden="true" />
      </Button>
      <Dialog open={open} onOpenChange={changeOpen}>
        <DialogContent className="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>导出统计报表</DialogTitle>
            <DialogDescription>
              仅在点击导出后写入本地文件，包含统计口径版本，不含正文或会话 ID。
            </DialogDescription>
          </DialogHeader>
          <form
            className="space-y-4"
            onSubmit={(event) => {
              event.preventDefault();
              void save();
            }}
          >
            <fieldset disabled={busy} className="min-w-0 space-y-4 disabled:opacity-60">
              <div className="space-y-2">
                <p className="text-sm font-medium">日期范围</p>
                <DateRangePicker
                  range={selectedRange}
                  nowMs={frozenNowMs}
                  firstEventMs={firstEventMs}
                  onChange={(next) => {
                    setSelectedRange(next);
                    setResult(null);
                    setError(null);
                  }}
                />
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-2">
                  <label htmlFor={`${id}-client`} className="text-sm font-medium">
                    客户端
                  </label>
                  <Select
                    value={client ?? "all"}
                    disabled={busy}
                    onValueChange={(value) => {
                      setClient(isClientKey(value) ? value : null);
                      setResult(null);
                      setError(null);
                    }}
                  >
                    <SelectTrigger id={`${id}-client`} className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <FilterSelectContent>
                      <SelectItem value="all">全部客户端</SelectItem>
                      {CLIENTS.map((key) => (
                        <SelectItem key={key} value={key}>
                          {clientLabel(key)}
                        </SelectItem>
                      ))}
                    </FilterSelectContent>
                  </Select>
                </div>
                <div className="space-y-2">
                  <label htmlFor={`${id}-format`} className="text-sm font-medium">
                    文件格式
                  </label>
                  <Select
                    value={format}
                    disabled={busy}
                    onValueChange={(value) => {
                      if (value === "csv" || value === "json" || value === "markdown") {
                        setFormat(value);
                        setResult(null);
                        setError(null);
                      }
                    }}
                  >
                    <SelectTrigger id={`${id}-format`} className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <FilterSelectContent>
                      <SelectItem value="csv">CSV</SelectItem>
                      <SelectItem value="json">JSON</SelectItem>
                      <SelectItem value="markdown">Markdown</SelectItem>
                    </FilterSelectContent>
                  </Select>
                </div>
              </div>
              <div className="space-y-2">
                <label htmlFor={`${id}-directory`} className="text-sm font-medium">
                  保存目录（绝对路径）
                </label>
                <Input
                  id={`${id}-directory`}
                  value={directory}
                  onChange={(event) => {
                    setDirectory(event.target.value);
                    setResult(null);
                    setError(null);
                  }}
                  disabled={storageLoading}
                  placeholder={storageLoading ? "正在读取存储目录…" : "例如 D:\\Reports"}
                  className="font-mono text-xs"
                />
                {storageError && (
                  <div className="space-y-1">
                    <p role="alert" className="text-xs text-destructive">
                      默认目录读取失败：{storageError}
                    </p>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={storageLoading}
                      onClick={() => {
                        setStorageLoading(true);
                        setStorageError(null);
                        setStorageAttempt((value) => value + 1);
                      }}
                    >
                      重试读取目录
                    </Button>
                  </div>
                )}
                <p className="text-xs text-muted-foreground">
                  默认使用当前统计存储目录；生成新文件，不覆盖已有文件。
                </p>
              </div>
              <div className="flex items-center justify-between gap-3">
                <label htmlFor={`${id}-estimated`} className="text-sm">
                  包含估算数据（按来源等级分行）
                </label>
                <Switch
                  id={`${id}-estimated`}
                  checked={includeEstimated}
                  onCheckedChange={(checked) => {
                    setIncludeEstimated(checked);
                    setResult(null);
                    setError(null);
                  }}
                  disabled={busy}
                />
              </div>
            </fieldset>
            <p className="text-xs text-muted-foreground">
              按请求时间筛选，以 UTC 日期分组；导出日期、客户端、会话主模型、项目、请求数与 Token
              及来源等级。未知时间请求不纳入报表，其计数为所选客户端全历史；估算排除计数只含所选日期。
            </p>
            {error && (
              <p role="alert" className="text-sm text-destructive">
                导出失败：{error}。请检查目录后重试。
              </p>
            )}
            {result && (
              <div role="status" className="space-y-2 rounded-lg border p-3 text-sm">
                <p>
                  {result.rowCount === 0
                    ? "已导出空报表"
                    : `已导出 ${formatNumber(result.rowCount)} 行`}
                </p>
                <p className="font-mono text-xs break-all select-text">{result.path}</p>
                <p className="text-xs text-muted-foreground">
                  未纳入：未知时间请求 {formatNumber(result.untimedRequests)}；估算请求{" "}
                  {formatNumber(result.excludedEstimatedRequests)}。
                </p>
              </div>
            )}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={busy}
                onClick={() => changeOpen(false)}
              >
                关闭
              </Button>
              <Button type="submit" disabled={busy || storageLoading || !directory.trim()}>
                {busy && <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />}
                {busy ? "正在导出…" : "导出"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}
