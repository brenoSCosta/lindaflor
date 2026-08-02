import { Position, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export const FLOW_DRAG_HANDLE_CLASS = "drag-handle__custom";

export function DragHandleNode({ data }: NodeProps) {
  return (
    <BaseNode className="relative px-3 py-2 shadow-sm">
      <div
        className={`${FLOW_DRAG_HANDLE_CLASS} text-muted-foreground absolute top-1 right-1 cursor-grab text-xs`}
      >
        ⠿
      </div>
      <div className="pr-4">{flowNodeLabel(data, "Node")}</div>
      <BaseHandle type="target" position={Position.Left} />
      <BaseHandle type="source" position={Position.Right} />
    </BaseNode>
  );
}
