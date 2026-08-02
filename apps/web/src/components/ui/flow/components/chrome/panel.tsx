import { Panel, type PanelProps } from "@xyflow/react";

import { FLOW_DATA_SLOT } from "@/components/ui/flow/core/constants";
import { cn } from "@/lib/utils";

export type FlowPanelProps = PanelProps;

export function FlowPanel({ className, children, ...props }: FlowPanelProps) {
  return (
    <Panel
      data-slot={FLOW_DATA_SLOT.panel}
      className={cn(
        "bg-background/90 text-foreground rounded-md border px-2 py-1.5 text-sm shadow-sm",
        className,
      )}
      {...props}
    >
      {children}
    </Panel>
  );
}
