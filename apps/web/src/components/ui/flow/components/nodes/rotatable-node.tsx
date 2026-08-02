import { Position, type Node, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";

export type RotatableNodeData = {
  label: string;
  rotation: number;
  onRotate: (id: string, rotation: number) => void;
};

export type RotatableNodeType = Node<RotatableNodeData, "rotatable">;

export function RotatableNode({ id, data }: NodeProps<RotatableNodeType>) {
  return (
    <BaseNode
      className="px-4 py-3 shadow-sm"
      style={{ transform: `rotate(${data.rotation}deg)` }}
    >
      <div className="text-sm">{data.label}</div>
      <input
        aria-label="Node rotation"
        className="nodrag mt-2 w-full"
        type="range"
        min={0}
        max={360}
        value={data.rotation}
        onChange={(e) => {
          data.onRotate(id, Number(e.target.value));
        }}
      />
      <BaseHandle type="target" position={Position.Left} />
      <BaseHandle type="source" position={Position.Right} />
    </BaseNode>
  );
}
