import { useId, type ReactNode } from "react";
import { Switch } from "@/components/ui/switch";
import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { FilterSelectContent } from "@/components/FilterSelectContent";

export function SettingsSection({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-4">
      <div className="space-y-2">
        <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
        {description && (
          <p className="text-sm leading-relaxed text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="divide-y divide-border/60 rounded-xl border border-border/70 bg-card px-4 shadow-xs lg:px-5">
        {children}
      </div>
    </section>
  );
}

export function SettingsRow({
  title,
  description,
  descriptionId,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
  descriptionId?: string;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-3 py-4">
      <div className="min-w-[min(100%,8rem)] flex-1">
        <p className="text-sm font-medium">{title}</p>
        {description && (
          <p id={descriptionId} className="mt-1 text-xs leading-relaxed text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      <div className="ml-auto flex min-h-8 max-w-full shrink-0 items-center justify-end">
        {children}
      </div>
    </div>
  );
}

export function SettingsSwitch({
  title,
  description,
  checked,
  disabled,
  onChange,
}: {
  title: string;
  description?: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  const descriptionId = useId();
  return (
    <SettingsRow title={title} description={description} descriptionId={descriptionId}>
      <Switch
        aria-label={title}
        aria-describedby={description ? descriptionId : undefined}
        className="data-[size=default]:h-4 data-[size=default]:w-7 [&_[data-slot=switch-thumb]]:size-3.5"
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
      />
    </SettingsRow>
  );
}

export function SettingsSelect<T extends string>({
  title,
  description,
  value,
  options,
  disabled,
  onChange,
}: {
  title: string;
  description?: string;
  value: T;
  options: ReadonlyArray<{ value: T; label: string }>;
  disabled: boolean;
  onChange: (value: T) => void;
}) {
  const descriptionId = useId();
  return (
    <SettingsRow title={title} description={description} descriptionId={descriptionId}>
      <Select
        value={value}
        disabled={disabled}
        onValueChange={(value) => {
          const option = options.find((option) => option.value === value);
          if (option) onChange(option.value);
        }}
      >
        <SelectTrigger
          className="w-44 pr-1.25"
          aria-label={title}
          aria-describedby={description ? descriptionId : undefined}
        >
          <SelectValue />
        </SelectTrigger>
        <FilterSelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </FilterSelectContent>
      </Select>
    </SettingsRow>
  );
}
