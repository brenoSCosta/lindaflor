import { NodeResizer, Position, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export type ResizableNodeProps = NodeProps & {
  minWidth?: number;
  minHeight?: number;
};

export function ResizableNode({
  data,
  selected,
  minWidth = 80,
  minHeight = 40,
}: ResizableNodeProps) {
  return (
    <BaseNode className="h-full w-full p-2 shadow-sm">
      <NodeResizer
        color="var(--primary)"
        isVisible={selected}
        minWidth={minWidth}
        minHeight={minHeight}
      />
      <div className="text-sm">{flowNodeLabel(data, "Node")}</div>
      <BaseHandle type="target" position={Position.Left} />
      <BaseHandle type="source" position={Position.Right} />
    </BaseNode>
  );
}
