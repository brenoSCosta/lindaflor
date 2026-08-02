import {
  useStore,
  type ConnectionLineComponentProps,
  type Node,
} from "@xyflow/react";

import { FLOW_DEFAULT_NODE_SIZE } from "@/components/ui/flow/core/constants";

export function MultiConnectionLine({
  fromX,
  fromY,
  toX,
  toY,
}: ConnectionLineComponentProps) {
  const origins = useStore((s) => {
    const selected: Node[] = [];
    for (const node of s.nodes) {
      if (node.selected) selected.push(node);
    }
    return selected;
  });

  return (
    <g data-slot="flow-multi-connection-line">
      <path
        fill="none"
        stroke="var(--primary)"
        strokeWidth={1.5}
        d={`M${fromX},${fromY} L${toX},${toY}`}
      />
      {origins.map((node) => {
        const w = node.measured?.width ?? FLOW_DEFAULT_NODE_SIZE.width;
        const h = node.measured?.height ?? FLOW_DEFAULT_NODE_SIZE.height;
        const x = node.position.x + w / 2;
        const y = node.position.y + h / 2;
        return (
          <path
            key={node.id}
            fill="none"
            stroke="var(--primary)"
            strokeWidth={1}
            strokeOpacity={0.5}
            d={`M${x},${y} L${toX},${toY}`}
          />
        );
      })}
    </g>
  );
}
