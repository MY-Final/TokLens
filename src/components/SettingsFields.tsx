import type { ReactNode } from "react";
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
    <section>
      <div className="space-y-2 border-b border-border/60 pb-5">
        <h2 className="text-xl font-semibold tracking-tight">{title}</h2>
        {description && (
          <p className="text-sm leading-relaxed text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="divide-y divide-border/50">{children}</div>
    </section>
  );
}

export function SettingsRow({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-3 py-5">
      <div className="min-w-[min(100%,10rem)] flex-1">
        <p className="text-sm font-medium">{title}</p>
        {description && (
          <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{description}</p>
        )}
      </div>
      {children}
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
  return (
    <SettingsRow title={title} description={description}>
      <Switch aria-label={title} checked={checked} disabled={disabled} onCheckedChange={onChange} />
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
  return (
    <SettingsRow title={title} description={description}>
      <Select
        value={value}
        disabled={disabled}
        onValueChange={(value) => {
          const option = options.find((option) => option.value === value);
          if (option) onChange(option.value);
        }}
      >
        <SelectTrigger className="w-44" aria-label={title}>
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
