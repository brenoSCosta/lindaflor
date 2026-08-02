import { MiniMap, type MiniMapProps } from "@xyflow/react";

import { FLOW_DATA_SLOT } from "@/components/ui/flow/core/constants";
import { cn } from "@/lib/utils";

export type FlowMiniMapProps = MiniMapProps;

export function FlowMiniMap({
  className,
  nodeColor = "var(--muted)",
  maskColor = "color-mix(in oklab, var(--background) 60%, transparent)",
  ...props
}: FlowMiniMapProps) {
  return (
    <MiniMap
      data-slot={FLOW_DATA_SLOT.minimap}
      className={cn(
        "bg-card border-border rounded-md border shadow-sm",
        className,
      )}
      nodeColor={nodeColor}
      maskColor={maskColor}
      {...props}
    />
  );
}
