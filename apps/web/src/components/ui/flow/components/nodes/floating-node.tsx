import { Position, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export function FloatingNode({ data }: NodeProps) {
  return (
    <BaseNode className="px-4 py-3 shadow-sm">
      {flowNodeLabel(data, "Node")}
      <BaseHandle type="source" position={Position.Top} className="opacity-0" />
      <BaseHandle type="target" position={Position.Top} className="opacity-0" />
    </BaseNode>
  );
}
