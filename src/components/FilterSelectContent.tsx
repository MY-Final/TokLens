import type { ComponentProps } from "react";

import { SelectContent } from "@/components/ui/select";
import { cn } from "@/lib/utils";

/** Filter menus open below the trigger rather than covering it with the selected item. */
export function FilterSelectContent({ className, ...props }: ComponentProps<typeof SelectContent>) {
  return (
    <SelectContent
      position="popper"
      side="bottom"
      align="start"
      collisionPadding={8}
      {...props}
      className={cn(
        "w-(--radix-select-trigger-width) rounded-lg border-border/60 shadow-lg [&_[data-slot=select-item]]:rounded-md [&_[data-slot=select-item]]:py-2",
        className,
      )}
    />
  );
}
