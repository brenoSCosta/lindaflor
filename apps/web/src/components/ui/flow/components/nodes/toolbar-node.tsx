import { NodeToolbar, Position, type NodeProps } from "@xyflow/react";
import type { ReactNode } from "react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export type ToolbarNodeProps = NodeProps & {
  toolbar?: ReactNode;
  toolbarPosition?: Position;
  toolbarVisible?: boolean;
};

export function ToolbarNode({
  data,
  toolbar,
  toolbarPosition = Position.Top,
  toolbarVisible = true,
}: ToolbarNodeProps) {
  return (
    <BaseNode className="px-4 py-3 shadow-sm">
      {toolbar ? (
        <NodeToolbar isVisible={toolbarVisible} position={toolbarPosition}>
          {toolbar}
        </NodeToolbar>
      ) : null}
      <div>{flowNodeLabel(data, "Node")}</div>
      <BaseHandle type="target" position={Position.Left} />
      <BaseHandle type="source" position={Position.Right} />
    </BaseNode>
  );
}
