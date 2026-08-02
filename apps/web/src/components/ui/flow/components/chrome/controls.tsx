import { Controls, type ControlProps } from "@xyflow/react";

import { FLOW_DATA_SLOT } from "@/components/ui/flow/core/constants";
import { cn } from "@/lib/utils";

export type FlowControlsProps = ControlProps;

export function FlowControls({ className, ...props }: FlowControlsProps) {
  return (
    <Controls
      data-slot={FLOW_DATA_SLOT.controls}
      className={cn(
        "border-border bg-card text-foreground shadow-sm",
        className,
      )}
      {...props}
    />
  );
}
