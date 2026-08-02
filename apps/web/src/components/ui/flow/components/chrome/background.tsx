import { Background, type BackgroundProps } from "@xyflow/react";

import { FLOW_DATA_SLOT } from "@/components/ui/flow/core/constants";
import { cn } from "@/lib/utils";

export type FlowBackgroundProps = BackgroundProps;

export function FlowBackground({
  className,
  color = "var(--muted-foreground)",
  gap = 16,
  ...props
}: FlowBackgroundProps) {
  return (
    <Background
      data-slot={FLOW_DATA_SLOT.background}
      className={cn(className)}
      color={color}
      gap={gap}
      {...props}
    />
  );
}
