import { Position, type Node, type NodeProps } from "@xyflow/react";
import type { ChangeEvent } from "react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";

export type ColorSelectorNodeData = {
  color: string;
  onColorChange: (event: ChangeEvent<HTMLInputElement>) => void;
};

export function ColorSelectorNode({
  data,
}: NodeProps<Node<ColorSelectorNodeData>>) {
  return (
    <BaseNode
      className="border-border p-3 shadow-sm"
      style={{ background: data.color }}
    >
      <BaseHandle type="target" position={Position.Left} />
      <div className="text-primary-foreground mb-2 text-xs font-medium">
        Color
      </div>
      <input
        aria-label="Node color"
        className="nodrag h-8 w-full"
        type="color"
        value={data.color}
        onChange={(event) => {
          data.onColorChange(event);
        }}
      />
      <BaseHandle type="source" position={Position.Right} />
    </BaseNode>
  );
}
