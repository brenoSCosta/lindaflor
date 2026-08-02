import { ReactFlowProvider } from "@xyflow/react";

import type { FlowProviderProps } from "@/components/ui/flow/core/types";

import "@/components/ui/flow/core/styles.css";

export function FlowProvider({ children }: FlowProviderProps) {
  return <ReactFlowProvider>{children}</ReactFlowProvider>;
}
