import { Position, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export function TouchNode({ data }: NodeProps) {
  return (
    <BaseNode className="px-4 py-3 shadow-sm">
      {flowNodeLabel(data, "Node")}
      <BaseHandle
        type="target"
        position={Position.Left}
        className="h-4! w-4!"
      />
      <BaseHandle
        type="source"
        position={Position.Right}
        className="h-4! w-4!"
      />
    </BaseNode>
  );
}
