import { ReactFlow } from "@xyflow/react";

import {
  FLOW_DATA_SLOT,
  FLOW_DEFAULT_PRO_OPTIONS,
} from "@/components/ui/flow/core/constants";
import type { FlowProps } from "@/components/ui/flow/core/types";
import { useFlowColorMode } from "@/components/ui/flow/hooks/use-flow-color-mode";
import { cn } from "@/lib/utils";

import "@/components/ui/flow/core/styles.css";

export function Flow({
  className,
  colorMode: colorModeProp,
  proOptions = FLOW_DEFAULT_PRO_OPTIONS,
  children,
  ...props
}: FlowProps) {
  const appColorMode = useFlowColorMode();
  const colorMode = colorModeProp ?? appColorMode;

  return (
    <ReactFlow
      data-slot={FLOW_DATA_SLOT.root}
      className={cn("bg-background text-foreground", className)}
      colorMode={colorMode}
      proOptions={proOptions}
      {...props}
    >
      {children}
    </ReactFlow>
  );
}
