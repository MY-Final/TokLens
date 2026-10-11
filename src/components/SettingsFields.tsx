import type { ReactNode } from "react";
import { DashboardCard } from "@/components/DashboardCard";
import { CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Select, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { FilterSelectContent } from "@/components/FilterSelectContent";

export function SettingsSection({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <DashboardCard>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent className="divide-y divide-border/50">{children}</CardContent>
    </DashboardCard>
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
    <div className="flex flex-wrap items-center justify-between gap-4 py-4 first:pt-0 last:pb-0">
      <div className="min-w-0 flex-1">
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
