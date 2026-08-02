import type { NodeProps } from "@xyflow/react";
import { Position } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";
import { cn } from "@/lib/utils";

export type LabelNodeData = {
  label?: string;
};

type LabelNodeProps = NodeProps & {
  className?: string;
  showSourceHandle?: boolean;
  showTargetHandle?: boolean;
  sourcePosition?: Position;
  targetPosition?: Position;
};

export function LabelNode({
  data,
  className,
  showSourceHandle = true,
  showTargetHandle = true,
  sourcePosition = Position.Bottom,
  targetPosition = Position.Top,
}: LabelNodeProps) {
  return (
    <div
      data-slot="flow-label-node"
      className={cn(
        "bg-card text-card-foreground rounded-md border px-4 py-3 text-sm shadow-sm",
        className,
      )}
    >
      {flowNodeLabel(data, "Node")}
      {showTargetHandle ? (
        <BaseHandle type="target" position={targetPosition} />
      ) : null}
      {showSourceHandle ? (
        <BaseHandle type="source" position={sourcePosition} />
      ) : null}
    </div>
  );
}
