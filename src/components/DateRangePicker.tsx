import { useId, useRef, useState } from "react";
import { CalendarDays, ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import {
  NATURAL_PRESETS,
  RELATIVE_PRESETS,
  createCustomRange,
  formatDateTimeInput,
  parseDateTimeInput,
  rangeSpanLabel,
  resolveRange,
} from "@/lib/ranges";
import type { DateRange, RangePreset } from "@/lib/ranges";
import { timeZoneName, timeZoneShortLabel } from "@/lib/format";
import { cn } from "@/lib/utils";

type Preset = { key: RangePreset; label: string };

type DateRangePickerProps = {
  range: DateRange;
  firstEventMs: number | null;
  nowMs: number;
  onChange: (range: DateRange) => void;
};

/** 预设分组：方向键在组内切换并立即生效。 */
function PresetGroup({
  label,
  presets,
  activeKey,
  onSelect,
}: {
  label: string;
  presets: ReadonlyArray<Preset>;
  activeKey: DateRange["key"];
  onSelect: (key: RangePreset) => void;
}) {
  const groupRef = useRef<HTMLDivElement>(null);
  function handleKeyDown(event: React.KeyboardEvent<HTMLButtonElement>, index: number) {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    const next = (index + (event.key === "ArrowRight" ? 1 : -1) + presets.length) % presets.length;
    const key = presets[next]?.key;
    if (!key) return;
    onSelect(key);
    groupRef.current?.querySelectorAll<HTMLButtonElement>("button")[next]?.focus();
  }
  return (
    <div className="space-y-1.5">
      <p className="px-1 text-xs text-muted-foreground">{label}</p>
      <div ref={groupRef} role="group" aria-label={label} className="flex flex-wrap gap-1.5">
        {presets.map((preset, index) => {
          const active = activeKey === preset.key;
          return (
            <Button
              key={preset.key}
              type="button"
              size="sm"
              variant={active ? "default" : "outline"}
              aria-pressed={active}
              className={cn(
                "h-8 rounded-md px-2.5 text-xs",
                active && "bg-primary text-primary-foreground hover:bg-primary/90",
              )}
              onClick={() => onSelect(preset.key)}
              onKeyDown={(event) => handleKeyDown(event, index)}
            >
              {preset.label}
            </Button>
          );
        })}
      </div>
    </div>
  );
}

export function DateRangePicker({ range, firstEventMs, nowMs, onChange }: DateRangePickerProps) {
  const id = useId();
  const startRef = useRef<HTMLInputElement>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [showCustom, setShowCustom] = useState(range.key === "custom");
  const [startValue, setStartValue] = useState("");
  const [endValue, setEndValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const zone = timeZoneShortLabel(nowMs);
  const zoneName = timeZoneName();
  const span = rangeSpanLabel(range);
  const summary = `${range.label} · ${span} · ${zone}`;

  function setDraft(next: DateRange) {
    setStartValue(formatDateTimeInput(next.fromMs ?? next.dayStartMs));
    setEndValue(formatDateTimeInput(next.toMs - 1));
    setError(null);
  }

  function changeOpen(next: boolean) {
    if (next) {
      setDraft(range);
      setShowCustom(range.key === "custom");
    } else {
      setStartValue("");
      setEndValue("");
      setError(null);
    }
    setIsOpen(next);
  }

  function selectPreset(key: RangePreset) {
    onChange(resolveRange(key, firstEventMs, new Date(nowMs)));
    setShowCustom(false);
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
    <Popover open={isOpen} onOpenChange={changeOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          className="h-9 max-w-[min(26rem,100%)] min-w-0 shrink justify-start gap-2 px-3 font-normal"
          title={`${summary}（${zoneName}）`}
          aria-label={`时间范围：${summary}，时区 ${zoneName}`}
        >
          <CalendarDays className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <span className="shrink-0 font-medium text-foreground">{range.label}</span>
          <span className="min-w-0 truncate text-muted-foreground">· {span}</span>
          <span className="shrink-0 text-muted-foreground">· {zone}</span>
          <ChevronDown className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        sideOffset={6}
        collisionPadding={16}
        className="w-[25rem] max-w-[calc(100vw-2rem)] space-y-3 p-3"
        aria-labelledby={`${id}-title`}
      >
        <h2 id={`${id}-title`} className="sr-only">
          时间范围
        </h2>
        <PresetGroup
          label="相对范围"
          presets={RELATIVE_PRESETS}
          activeKey={range.key}
          onSelect={selectPreset}
        />
        <PresetGroup
          label="自然周期"
          presets={NATURAL_PRESETS}
          activeKey={range.key}
          onSelect={selectPreset}
        />
        <Separator />
        <div className="space-y-2">
          <Button
            type="button"
            variant={range.key === "custom" ? "secondary" : "ghost"}
            aria-expanded={showCustom}
            className="h-8 w-full justify-start rounded-md px-2.5 text-xs"
            onClick={() => setShowCustom((value) => !value)}
          >
            自定义日期
            {range.key === "custom" ? <span className="text-muted-foreground">· 当前</span> : null}
          </Button>
          {showCustom ? (
            <form
              noValidate
              className="space-y-3"
              onSubmit={(event) => {
                event.preventDefault();
                confirm();
              }}
            >
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                <div className="min-w-0 space-y-1.5">
                  <label htmlFor={`${id}-start`} className="text-xs text-muted-foreground">
                    起始时间
                  </label>
                  <Input
                    ref={startRef}
                    id={`${id}-start`}
                    type="datetime-local"
                    step={60}
                    required
                    value={startValue}
                    className="h-9 w-full min-w-0 text-xs tabular-nums"
                    aria-invalid={error !== null}
                    aria-describedby={`${id}-note${error ? ` ${id}-error` : ""}`}
                    onChange={(event) => {
                      setStartValue(event.target.value);
                      setError(null);
                    }}
                  />
                </div>
                <div className="min-w-0 space-y-1.5">
                  <label htmlFor={`${id}-end`} className="text-xs text-muted-foreground">
                    结束时间
                  </label>
                  <Input
                    id={`${id}-end`}
                    type="datetime-local"
                    step={60}
                    required
                    value={endValue}
                    className="h-9 w-full min-w-0 text-xs tabular-nums"
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
                {zone} · {zoneName} · 精确到分钟，包含结束分钟
              </p>
              {error ? (
                <p id={`${id}-error`} role="alert" className="text-xs text-destructive">
                  {error}
                </p>
              ) : null}
              <div className="flex justify-end">
                <Button type="submit" size="sm" className="h-8">
                  确认
                </Button>
              </div>
            </form>
          ) : null}
        </div>
      </PopoverContent>
    </Popover>
  );
}
