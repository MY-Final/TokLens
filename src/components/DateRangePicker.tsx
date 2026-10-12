import { useId, useRef, useState } from "react";
import { CalendarDays } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  createCustomRange,
  formatDateTimeInput,
  parseDateTimeInput,
  resolveRange,
} from "@/lib/ranges";
import type { DateRange, RangePreset } from "@/lib/ranges";

const PRESETS: ReadonlyArray<{ key: RangePreset; label: string }> = [
  { key: "today", label: "今天" },
  { key: "7d", label: "7 天" },
  { key: "week", label: "本周" },
  { key: "30d", label: "30 天" },
  { key: "month", label: "本月" },
  { key: "90d", label: "90 天" },
  { key: "all", label: "全部" },
];

type DateRangePickerProps = {
  range: DateRange;
  firstEventMs: number | null;
  nowMs: number;
  onChange: (range: DateRange) => void;
};

export function DateRangePicker({ range, firstEventMs, nowMs, onChange }: DateRangePickerProps) {
  const id = useId();
  const startRef = useRef<HTMLInputElement>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [startValue, setStartValue] = useState("");
  const [endValue, setEndValue] = useState("");

  const [error, setError] = useState<string | null>(null);
  const startMs = range.fromMs ?? range.dayStartMs;
  const interval = `${formatDateTimeInput(startMs).replace("T", " ")} — ${formatDateTimeInput(range.toMs - 1).replace("T", " ")}`;

  function setDraft(next: DateRange) {
    setStartValue(formatDateTimeInput(next.fromMs ?? next.dayStartMs));
    setEndValue(formatDateTimeInput(next.toMs - 1));
    setError(null);
  }

  function changeOpen(next: boolean) {
    if (next) setDraft(range);
    else {
      setStartValue("");
      setEndValue("");
      setError(null);
    }
    setIsOpen(next);
  }

  function confirm() {
    const fromMs = parseDateTimeInput(startValue);
    const endMs = parseDateTimeInput(endValue);
    if (fromMs === null || endMs === null) {
      setError(!startValue || !endValue ? "请选择起始和结束时间" : "请输入有效的日期和时间");
      return;
    }
    if (endMs < fromMs) {
      setError("结束时间不能早于起始时间");
      return;
    }
    try {
      const next = createCustomRange(fromMs, endMs + 60_000);
      onChange(next);
      changeOpen(false);
    } catch {
      setError("请输入有效的日期和时间");
    }
  }

  return (
    <div className="w-full max-w-max min-w-0 space-y-1 justify-self-end">
      <div
        className="flex flex-wrap items-center justify-end gap-0.5"
        role="group"
        aria-label="统计时间范围"
      >
        {PRESETS.map((preset) => (
          <Button
            key={preset.key}
            size="sm"
            className="h-7 px-1 text-xs"
            variant={range.key === preset.key ? "secondary" : "ghost"}
            aria-pressed={range.key === preset.key}
            onClick={() => onChange(resolveRange(preset.key, firstEventMs, new Date(nowMs)))}
          >
            {preset.label}
          </Button>
        ))}
        <Popover open={isOpen} onOpenChange={changeOpen}>
          <PopoverTrigger asChild>
            <Button
              variant={range.key === "custom" ? "secondary" : "outline"}
              size="icon-sm"
              className="size-7"
              title={`${range.label}：${interval}（本地时区）`}
              aria-label={`自定义日期范围，当前${range.label}：${interval}`}
              aria-pressed={range.key === "custom"}
            >
              <CalendarDays className="size-4" aria-hidden="true" />
              <span className="sr-only">自定义</span>
            </Button>
          </PopoverTrigger>
          <PopoverContent
            align="end"
            sideOffset={4}
            collisionPadding={16}
            className="w-[32rem] max-w-[calc(100vw-2rem)] p-4"
            aria-labelledby={`${id}-title`}
            onOpenAutoFocus={(event) => {
              event.preventDefault();
              startRef.current?.focus();
            }}
          >
            <form
              noValidate
              className="space-y-4"
              onSubmit={(event) => {
                event.preventDefault();
                confirm();
              }}
            >
              <h2 id={`${id}-title`} className="sr-only">
                日期范围
              </h2>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div className="min-w-0 space-y-2">
                  <label htmlFor={`${id}-start`} className="text-sm font-medium">
                    起始时间
                  </label>
                  <Input
                    ref={startRef}
                    id={`${id}-start`}
                    type="datetime-local"
                    step={60}
                    required
                    value={startValue}
                    className="w-full min-w-0 tabular-nums"
                    aria-invalid={error !== null}
                    aria-describedby={`${id}-note${error ? ` ${id}-error` : ""}`}
                    onChange={(event) => {
                      setStartValue(event.target.value);
                      setError(null);
                    }}
                  />
                </div>
                <div className="min-w-0 space-y-2">
                  <label htmlFor={`${id}-end`} className="text-sm font-medium">
                    结束时间
                  </label>
                  <Input
                    id={`${id}-end`}
                    type="datetime-local"
                    step={60}
                    required
                    value={endValue}
                    className="w-full min-w-0 tabular-nums"
                    aria-invalid={error !== null}
                    aria-describedby={`${id}-note${error ? ` ${id}-error` : ""}`}
                    onChange={(event) => {
                      setEndValue(event.target.value);
                      setError(null);
                    }}
                  />
                </div>
              </div>
              <p id={`${id}-note`} className="text-xs text-muted-foreground">
                本地时区 · 精确到分钟，包含结束分钟
              </p>
              {error && (
                <p id={`${id}-error`} role="alert" className="text-xs text-destructive">
                  {error}
                </p>
              )}
              <div className="flex justify-end">
                <Button type="submit" size="sm">
                  确认
                </Button>
              </div>
            </form>
          </PopoverContent>
        </Popover>
      </div>
      <p
        className="truncate text-right text-xs text-muted-foreground tabular-nums"
        title={`${range.label}：${interval}（本地时区）`}
      >
        {interval} · 本地时区
      </p>
    </div>
  );
}
