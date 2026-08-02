import { Position, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export function TailwindNode({ data }: NodeProps) {
  return (
    <div
      data-slot="flow-tailwind-node"
      className="border-primary/30 from-primary/5 to-accent/10 text-foreground rounded-xl border bg-linear-to-br px-4 py-3 text-sm font-medium shadow-md"
    >
      {flowNodeLabel(data, "Node")}
      <BaseHandle
        type="target"
        position={Position.Left}
        className="bg-primary!"
      />
      <BaseHandle
        type="source"
        position={Position.Right}
        className="bg-accent!"
      />
    </div>
  );
}
