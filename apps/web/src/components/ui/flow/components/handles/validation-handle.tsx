import type { ComponentProps } from "react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { cn } from "@/lib/utils";

export type ValidationHandleProps = ComponentProps<typeof BaseHandle> & {
  tone?: "success" | "primary";
};

export function ValidationHandle({
  className,
  tone = "success",
  ...props
}: ValidationHandleProps) {
  return (
    <BaseHandle
      data-slot="flow-validation-handle"
      className={cn(
        tone === "success" ? "bg-success!" : "bg-primary!",
        className,
      )}
      {...props}
    />
  );
}
