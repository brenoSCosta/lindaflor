import { Position, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export function TurboNode({ data }: NodeProps) {
  return (
    <div data-slot="flow-turbo-node" className="relative rounded-lg p-0.5">
      <div
        className="absolute inset-0 animate-spin rounded-lg opacity-80"
        style={{
          background:
            "conic-gradient(from 180deg at 50% 50%, var(--primary), var(--accent, var(--primary)), var(--primary))",
          animationDuration: "3s",
        }}
      />
      <div className="bg-background relative rounded-[calc(var(--radius-lg)-1px)] px-4 py-3 text-sm font-semibold">
        {flowNodeLabel(data, "Turbo")}
        <BaseHandle type="target" position={Position.Left} />
        <BaseHandle type="source" position={Position.Right} />
      </div>
    </div>
  );
}
