import { Position, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export function MultiHandleNode({ data }: NodeProps) {
  return (
    <BaseNode className="px-4 py-3 shadow-sm">
      {flowNodeLabel(data, "Node")}
      <BaseHandle
        type="target"
        position={Position.Left}
        id="t1"
        style={{ top: "30%" }}
      />
      <BaseHandle
        type="target"
        position={Position.Left}
        id="t2"
        style={{ top: "70%" }}
      />
      <BaseHandle
        type="source"
        position={Position.Right}
        id="s1"
        style={{ top: "30%" }}
      />
      <BaseHandle
        type="source"
        position={Position.Right}
        id="s2"
        style={{ top: "70%" }}
      />
    </BaseNode>
  );
}
