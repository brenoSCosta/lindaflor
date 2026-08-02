import { Handle } from "@xyflow/react";
import type { ComponentProps } from "react";

import { cn } from "@/lib/utils";

export type BaseHandleProps = ComponentProps<typeof Handle>;

export function BaseHandle({ className, children, ...props }: BaseHandleProps) {
  return (
    <Handle
      data-slot="flow-base-handle"
      {...props}
      className={cn(
        "size-2.75 rounded-full border border-border bg-muted transition",
        className,
      )}
    >
      {children}
    </Handle>
  );
}
